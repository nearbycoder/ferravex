//! A small cave expedition: recover three crystals, wake two altars, find daylight.
use ferravex_core::FixedClock;
use ferravex_physics::character::{
    CharacterSettings, CharacterShape, Obstacle, ObstacleShape, QueryBounds, Terrain,
    move_and_slide,
};
use ferravex_play::camera::{Input3, Mode3, NoObstructions, Rig3, Settings3, Target3};
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    ActionKind, Binding, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
type Aabb2 = ferravex_core::collision::Aabb2<Vec2>;
use ferravex_render::{
    demo::Demo,
    material::{MaterialOptions, ShaderMaterial, ShaderSource},
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
};
use std::collections::VecDeque;

const TILE: f32 = 2.7;
const RADIUS: f32 = 0.29;
const SPEED: f32 = 4.3;
const MAP: [&str; 15] = [
    "#############",
    "#####...#####",
    "#####.X.#####",
    "######G######",
    "###.......###",
    "###A...B..###",
    "###.......###",
    "######.######",
    "#....#.#....#",
    "#.C.......C.#",
    "#....#.#....#",
    "###....C..###",
    "#####...#####",
    "#####.S.#####",
    "#############",
];
const MINT: Color = Color::from_rgba(108, 239, 209, 255);
const INK: Color = Color::from_rgba(20, 26, 29, 255);
const WARM: Color = Color::from_rgba(202, 165, 107, 255);

fn center(x: usize, z: usize) -> Vec2 {
    vec2(x as f32 * TILE, z as f32 * TILE)
}
fn at(marker: u8) -> Vec2 {
    for (z, row) in MAP.iter().enumerate() {
        if let Some(x) = row.bytes().position(|c| c == marker) {
            return center(x, z);
        }
    }
    panic!("map is missing marker");
}

fn forward(yaw: f32) -> Vec2 {
    vec2(yaw.sin(), -yaw.cos())
}
fn world(p: Vec2, height: f32) -> Vec3 {
    vec3(p.x, height, p.y)
}

#[derive(Clone, Copy, Default)]
struct Input {
    movement: Vec2,
    turn: f32,
    interact: bool,
}

fn objectives() -> ferravex_play::objective::ObjectiveGraph<6> {
    use ferravex_play::objective::*;
    ObjectiveGraph::new(
        1,
        std::array::from_fn(|i| ObjectiveSpec {
            id: i as u16 + 1,
            all: match i {
                3 => 7,
                4 => 1 << 3,
                5 => 1 << 4,
                _ => 0,
            },
            any: 0,
            exclusive_group: 0,
            kind: ObjectiveKind::Event {
                id: i as u16 + 1,
                count: 1,
            },
        }),
    )
    .unwrap()
}
struct Game {
    position: Vec2,
    velocity: Vec2,
    yaw: f32,
    walls: Vec<Aabb2>,
    crystals: Vec<Vec2>,
    collected: [bool; 3],
    objectives: ferravex_play::objective::ObjectiveGraph<6>,
    switches: u8,
    gate: Aabb2,
    won: bool,
    elapsed: f32,
    notice: &'static str,
    notice_time: f32,
}
impl Game {
    fn new() -> Self {
        let mut walls = Vec::new();
        let mut crystals = Vec::new();
        for (z, row) in MAP.iter().enumerate() {
            for (x, tile) in row.bytes().enumerate() {
                if tile == b'#' {
                    walls.push(Aabb2::from_min_size(
                        center(x, z) - Vec2::splat(TILE * 0.5),
                        Vec2::splat(TILE),
                    ));
                } else if tile == b'C' {
                    crystals.push(center(x, z));
                }
            }
        }
        Self {
            position: at(b'S'),
            velocity: Vec2::ZERO,
            yaw: 0.0,
            walls,
            crystals,
            collected: [false; 3],
            objectives: objectives(),
            switches: 0,
            gate: Aabb2::from_min_size(at(b'G') - vec2(TILE * 0.5, 0.175), vec2(TILE, 0.35)),
            won: false,
            elapsed: 0.0,
            notice: "Recover three crystals. Wake the numbered altars.",
            notice_time: 8.0,
        }
    }
    fn count(&self) -> usize {
        self.collected.iter().filter(|&&c| c).count()
    }
    fn open(&self) -> bool {
        self.switches == 2 && self.count() == 3
    }
    fn obstacles(&self) -> impl Iterator<Item = &Aabb2> {
        self.walls
            .iter()
            .chain((!self.open()).then_some(&self.gate))
    }
    fn visible(&self, p: Vec2) -> bool {
        !self
            .obstacles()
            .any(|wall| wall.cast_segment(self.position, p).is_some())
    }
    fn near(&self, p: Vec2) -> bool {
        self.position.distance_squared(p) < 1.7 * 1.7 && self.visible(p)
    }
    fn message(&mut self, text: &'static str) {
        self.notice = text;
        self.notice_time = 3.5;
    }
    fn objective_event(&mut self, index: usize) {
        self.objectives
            .event(index as u16 + 1, 1, self.objectives.last_event_serial() + 1)
            .unwrap();
        self.sync_objectives();
    }
    fn sync_objectives(&mut self) {
        for (i, collected) in self.collected.iter_mut().enumerate() {
            *collected = self.objectives.complete(i);
        }
        self.switches = if self.objectives.complete(4) {
            2
        } else if self.objectives.complete(3) {
            1
        } else {
            0
        };
        self.won = self.objectives.complete(5);
    }
    fn interact(&mut self) {
        for i in 0..self.crystals.len() {
            if !self.collected[i] && self.near(self.crystals[i]) {
                self.objective_event(i);
                self.message(if self.count() == 3 {
                    "All three recovered. Find altar I, then altar II."
                } else {
                    "Crystal recovered."
                });
                return;
            }
        }
        if self.near(at(b'A')) {
            if self.count() < 3 {
                self.message("Altar I needs all three crystals.");
            } else if self.switches == 0 {
                self.objective_event(3);
                self.message("Altar I awakened. The second altar is ready.");
            }
        } else if self.near(at(b'B')) {
            if self.switches == 0 {
                self.message("Wake altar I first.");
            } else if self.switches == 1 {
                self.objective_event(4);
                self.message("The seal is open. Follow the daylight.");
            }
        }
    }
    fn prompt(&self) -> Option<&'static str> {
        if self.won {
            return None;
        }
        if self
            .crystals
            .iter()
            .enumerate()
            .any(|(i, &p)| !self.collected[i] && self.near(p))
        {
            Some("E  /  Recover crystal")
        } else if self.near(at(b'A')) && self.switches == 0 {
            Some(if self.count() == 3 {
                "E  /  Awaken altar I"
            } else {
                "Altar I  /  Three crystals required"
            })
        } else if self.near(at(b'B')) && self.switches < 2 {
            Some(if self.switches == 1 {
                "E  /  Awaken altar II"
            } else {
                "Altar II  /  Wake altar I first"
            })
        } else {
            None
        }
    }
    fn step(&mut self, input: Input, dt: f32) {
        self.notice_time = (self.notice_time - dt).max(0.0);
        if self.won {
            return;
        }
        self.elapsed += dt;
        self.yaw += input.turn * dt;
        let front = forward(self.yaw);
        let right = vec2(-front.y, front.x);
        let movement = input.movement.clamp_length_max(1.0);
        let desired = (front * movement.y + right * movement.x) * SPEED;
        let weight = ferravex_play::motion::response(
            if desired.length_squared() < 0.01 {
                24.
            } else {
                18.
            },
            dt,
        );
        self.velocity = self.velocity.lerp(desired, weight);
        let delta = self.velocity * dt;
        let motion = move_and_slide(
            &CaveTerrain(self),
            CharacterShape::Capsule {
                radius: RADIUS,
                height: 1.75,
            },
            CharacterSettings {
                step_height: 0.,
                ground_snap: 0.,
                ..Default::default()
            },
            world(self.position, 0.),
            world(delta, 0.),
            true,
        )
        .expect("authored cave character terrain");
        let next = vec2(motion.position.x, motion.position.z);
        if dt > 0. {
            // Wall sliding must not retain hidden velocity into a blocked wall.
            self.velocity = (next - self.position) / dt;
        }
        self.position = next;
        if input.interact {
            self.interact();
        }
        if self.open() && self.position.distance(at(b'X')) < 1.2 {
            self.objective_event(5);
            self.message("Expedition complete.");
        }
    }
}

struct CaveTerrain<'a>(&'a Game);
impl CaveTerrain<'_> {
    fn box_at(&self, id: usize) -> Option<Obstacle> {
        let wall = if id == self.0.walls.len() {
            if self.0.open() {
                return None;
            }
            self.0.gate
        } else {
            *self.0.walls.get(id)?
        };
        let center = (wall.min + wall.max) * 0.5;
        let half = (wall.max - wall.min) * 0.5;
        Some(Obstacle::cuboid(
            id as u64 + 1,
            world(center, 1.5),
            vec3(half.x, 1.5, half.y),
        ))
    }
}
impl Terrain for CaveTerrain<'_> {
    fn visit(&self, bounds: QueryBounds, visitor: &mut dyn FnMut(Obstacle)) {
        visitor(Obstacle {
            shape: ObstacleShape::Plane { normal: Vec3::Y },
            ..Obstacle::cuboid(0, Vec3::ZERO, Vec3::ONE)
        });
        for i in 0..=self.0.walls.len() {
            if let Some(o) = self.box_at(i)
                && bounds.intersects(o.bounds())
            {
                visitor(o);
            }
        }
    }
    fn obstacle(&self, id: u64) -> Option<Obstacle> {
        if id == 0 {
            Some(Obstacle {
                shape: ObstacleShape::Plane { normal: Vec3::Y },
                ..Obstacle::cuboid(0, Vec3::ZERO, Vec3::ONE)
            })
        } else {
            self.box_at((id - 1) as usize)
        }
    }
}

/// The smoke driver plans a route, then emits the same movement/turn/E inputs as a player.
#[derive(Default)]
struct Pilot {
    goal: Option<Vec2>,
    path: VecDeque<Vec2>,
}
impl Pilot {
    fn input(&mut self, game: &Game) -> Input {
        if game.won {
            return Input::default();
        }
        let goal = game
            .crystals
            .iter()
            .enumerate()
            .find(|(i, _)| !game.collected[*i])
            .map(|(_, &p)| p)
            .unwrap_or_else(|| {
                at(if game.switches == 0 {
                    b'A'
                } else if game.switches == 1 {
                    b'B'
                } else {
                    b'X'
                })
            });
        if self.goal != Some(goal) {
            self.goal = Some(goal);
            self.path = route(game, goal);
        }
        while self
            .path
            .front()
            .is_some_and(|p| game.position.distance(*p) < 0.18)
        {
            self.path.pop_front();
        }
        let target = self.path.front().copied().unwrap_or(goal);
        let delta = target - game.position;
        let angle = delta.x.atan2(-delta.y);
        let turn = (angle - game.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        Input {
            movement: vec2(
                0.0,
                if turn.abs() < 0.15 && delta.length() > 0.06 {
                    1.0
                } else {
                    0.0
                },
            ),
            turn: (turn * 10.0).clamp(-3.5, 3.5),
            interact: game.near(goal),
        }
    }
}
fn route(game: &Game, goal: Vec2) -> VecDeque<Vec2> {
    use ferravex_play::{
        ai::WorkBudget,
        navigation::{NavMesh, NavTriangle},
    };
    let mut faces = Vec::new();
    // Door state controls the actual traversable surface at planning time.
    for (z, row) in MAP.iter().enumerate() {
        for (x, tile) in row.bytes().enumerate() {
            if tile == b'#' || (tile == b'G' && !game.open()) {
                continue;
            }
            let a = world(vec2((x as f32 - 0.5) * TILE, (z as f32 - 0.5) * TILE), 0.);
            let b = world(vec2((x as f32 + 0.5) * TILE, (z as f32 - 0.5) * TILE), 0.);
            let c = world(vec2((x as f32 - 0.5) * TILE, (z as f32 + 0.5) * TILE), 0.);
            let d = world(vec2((x as f32 + 0.5) * TILE, (z as f32 + 0.5) * TILE), 0.);
            faces.push(NavTriangle([a, b, c]));
            faces.push(NavTriangle([b, d, c]));
        }
    }
    let mesh = NavMesh::welded(faces).unwrap();
    let Ok(mut search) = mesh.search(world(game.position, 0.), world(goal, 0.)) else {
        return VecDeque::new();
    };
    // This offline pilot plans only on goal changes, with a hard work allowance.
    if search.step(&mesh, &mut WorkBudget::new(0, 0, 10000)) != Ok(true) {
        return VecDeque::new();
    }
    search
        .route(&mesh)
        .unwrap()
        .iter()
        .map(|p| vec2(p.position.x, p.position.z))
        .collect()
}

/// Small triangular rock faces, bounded to 14 cm beyond the collision slab.
/// Flat normals and vertex colors provide material detail without texture assets.
fn faceted_wall(color: Color, seed: usize) -> Mesh3d {
    let mut vertices = Vec::new();
    for (side, (normal, tangent)) in [
        (Vec3::X, Vec3::Z),
        (-Vec3::X, -Vec3::Z),
        (Vec3::Z, -Vec3::X),
        (-Vec3::Z, Vec3::X),
    ]
    .iter()
    .enumerate()
    {
        for band in 0..3 {
            let bottom = -2.1 + band as f32 * 1.4;
            let top = bottom + 1.4;
            let origin = *normal * TILE * 0.5;
            let corners = [
                origin - *tangent * TILE * 0.5 + Vec3::Y * bottom,
                origin + *tangent * TILE * 0.5 + Vec3::Y * bottom,
                origin + *tangent * TILE * 0.5 + Vec3::Y * top,
                origin - *tangent * TILE * 0.5 + Vec3::Y * top,
            ];
            let middle = origin
                + *normal * 0.14
                + *tangent * (((seed + band + side) % 3) as f32 - 1.0) * 0.35
                + Vec3::Y * (bottom + 0.63);
            for face in 0..4 {
                let points = [corners[face], corners[(face + 1) % 4], middle];
                let mut face_normal = (points[1] - points[0])
                    .cross(points[2] - points[0])
                    .normalize();
                if face_normal.dot(*normal) < 0.0 {
                    face_normal = -face_normal;
                }
                let shade = 0.83 + ((seed + side * 3 + face + band * 2) % 5) as f32 * 0.045;
                for point in points {
                    let mut vertex = Vertex::new2(
                        point,
                        Vec2::ZERO,
                        Color::new(color.r * shade, color.g * shade, color.b * shade, 1.0),
                    );
                    vertex.normal = face_normal.extend(0.0);
                    vertices.push(vertex);
                }
            }
        }
    }
    Mesh3d::from_mesh(Mesh {
        indices: (0..vertices.len() as u16).collect(),
        vertices,
        texture: None,
    })
}

struct Piece {
    mesh: Mesh3d,
    position: Vec3,
    rotation: Quat,
    wall_sides: Option<[bool; 4]>,
}
struct Scene {
    pulse: ShaderMaterial,
    #[cfg(not(target_arch = "wasm32"))]
    watcher: std::cell::RefCell<Option<ferravex_render::material::MaterialFileWatcher>>,
    stone: Vec<Piece>,
    crystal: Mesh3d,
    tip: Mesh3d,
    pedestal: Mesh3d,
    rune: Mesh3d,
    gate: Mesh3d,
    daylight: Mesh3d,
}
impl Scene {
    fn new() -> Self {
        let mut stone = Vec::new();
        for (z, row) in MAP.iter().enumerate() {
            for (x, tile) in row.bytes().enumerate() {
                let p = center(x, z);
                let hash = (x * 17 + z * 31) % 7;
                if tile == b'#' {
                    // Same order as faceted_wall: +X, -X, +Z, -Z.
                    let sides = [
                        (x as i32 + 1, z as i32),
                        (x as i32 - 1, z as i32),
                        (x as i32, z as i32 + 1),
                        (x as i32, z as i32 - 1),
                    ]
                    .map(|(a, b)| {
                        (0..13).contains(&a)
                            && (0..15).contains(&b)
                            && MAP[b as usize].as_bytes()[a as usize] != b'#'
                    });
                    if !sides.iter().any(|&open| open) {
                        continue;
                    }
                    let c = Color::from_rgba(
                        91 + hash as u8 * 5,
                        69 + hash as u8 * 3,
                        55 + hash as u8 * 2,
                        255,
                    );
                    stone.push(Piece {
                        wall_sides: Some(sides),
                        mesh: faceted_wall(c, hash),
                        position: world(p, 2.1),
                        rotation: Quat::IDENTITY,
                    });
                } else {
                    let shade = 45 + hash as u8 * 2;
                    stone.push(Piece {
                        wall_sides: None,
                        mesh: Mesh3d::cuboid(
                            vec3(TILE, 0.22, TILE),
                            Color::from_rgba(shade, shade - 5, shade - 8, 255),
                        ),
                        position: world(p, -0.12),
                        rotation: Quat::IDENTITY,
                    });
                    stone.push(Piece {
                        wall_sides: None,
                        mesh: Mesh3d::cuboid(
                            vec3(TILE, 0.4, TILE),
                            Color::from_rgba(61, 48, 41, 255),
                        ),
                        position: world(p, 4.3),
                        rotation: Quat::IDENTITY,
                    });
                    // Ceiling formations frame the view without blocking the walking plane.
                    if hash < 3 && tile == b'.' {
                        stone.push(Piece {
                            wall_sides: None,
                            mesh: Mesh3d::cone(
                                0.55,
                                0.8 + hash as f32 * 0.25,
                                Color::from_rgba(84, 64, 48, 255),
                                5,
                            ),
                            position: world(p + vec2(0.6, 0.7), 3.5),
                            rotation: Quat::from_rotation_x(std::f32::consts::PI),
                        });
                    }
                }
            }
        }
        let occlusion = ferravex_render::ao::AoScene::new(stone.iter().map(|p| {
            (
                &p.mesh.mesh,
                Mat4::from_rotation_translation(p.rotation, p.position),
            )
        }))
        .expect("cave AO scene");
        for p in &mut stone {
            occlusion
                .bake(
                    &mut p.mesh.mesh,
                    Mat4::from_rotation_translation(p.rotation, p.position),
                    ferravex_render::ao::AoSettings {
                        radius: 1.5,
                        strength: 0.3,
                        ..Default::default()
                    },
                )
                .expect("cave vertex AO");
            if let Some(sides) = p.wall_sides {
                // Bake against complete occluders first. Only the faces touching
                // walkable cells can be seen from the collision-bounded camera.
                // Three height bands × four triangles × three vertices per side.
                const SIDE_VERTICES: usize = 3 * 4 * 3;
                p.mesh
                    .mesh
                    .indices
                    .retain(|&i| sides[i as usize / SIDE_VERTICES]);
            }
            p.mesh.invalidate_gpu();
        }
        // Bake once, then merge nearby opaque pieces into 3x3-tile chunks.
        // Authored AO and normals survive; distant chunks retain useful bounds.
        let mut chunks = std::collections::BTreeMap::<(i32, i32), Vec<Piece>>::new();
        for piece in stone {
            let key = (
                (piece.position.x / (TILE * 3.)).floor() as i32,
                (piece.position.z / (TILE * 3.)).floor() as i32,
            );
            chunks.entry(key).or_default().push(piece);
        }
        let stone = chunks
            .into_values()
            .map(|pieces| Piece {
                mesh: Mesh3d::combine(pieces.iter().map(|p| {
                    (
                        &p.mesh.mesh,
                        Mat4::from_rotation_translation(p.rotation, p.position),
                    )
                }))
                .expect("bounded static cave chunk"),
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                wall_sides: None,
            })
            .collect();
        let pulse = ShaderMaterial::new(
            ShaderSource::fragment(include_str!("../shaders/crystal.frag")),
            MaterialOptions {
                blend: ferravex_render::material::BlendMode::Opaque,
                ..Default::default()
            },
        )
        .expect("crystal material");
        #[cfg(not(target_arch = "wasm32"))]
        let watcher = std::env::var_os("FERRAVEX_CRYSTAL_SHADER").map(|path| {
            ferravex_render::material::MaterialFileWatcher::new(pulse.clone(), None, path.into())
        });
        Self {
            pulse,
            #[cfg(not(target_arch = "wasm32"))]
            watcher: std::cell::RefCell::new(watcher),
            stone,
            crystal: Mesh3d::frustum(0.32, 0.24, 0.8, MINT, 6),
            tip: Mesh3d::cone(0.25, 0.55, Color::from_rgba(160, 250, 235, 255), 6),
            pedestal: Mesh3d::cylinder(0.7, 0.65, Color::from_rgba(112, 99, 79, 255), 8),
            rune: Mesh3d::cuboid(vec3(0.075, 0.025, 0.4), MINT),
            gate: Mesh3d::cuboid(vec3(TILE, 3.9, 0.35), Color::from_rgba(56, 102, 101, 255)),
            daylight: Mesh3d::cuboid(vec3(2.4, 3.5, 0.08), Color::from_rgba(225, 235, 182, 255)),
        }
    }
    fn draw(&self, game: &Game, material: &LitMaterial, eye: Vec3, time: f32) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(watcher) = self.watcher.borrow_mut().as_mut()
            && let Err(error) = watcher.poll()
        {
            eprintln!("Crystal shader reload: {error}");
        }
        let mut lights = [ferravex_render::lighting::PointLight {
            position: Vec3::ZERO,
            color: MINT,
            radius: 4.8,
            intensity: 1.8,
        }; 3];
        let mut count = 0;
        for (i, &p) in game.crystals.iter().enumerate() {
            if !game.collected[i] {
                lights[count].position = world(p, 1.35);
                count += 1;
            }
        }
        ferravex_render::set_point_lights(&lights[..count]).unwrap();
        let settings = LitSettings {
            camera_position: eye,
            light_direction: vec3(-0.3, 0.8, 0.4),
            fog_color: INK,
            fog_start: 7.0,
            fog_end: 27.0,
            ambient: 0.58,
        };
        material.begin(settings);
        for piece in &self.stone {
            piece.mesh.draw(piece.position, piece.rotation);
        }
        for marker in *b"AB" {
            self.pedestal.draw(world(at(marker), 0.325), Quat::IDENTITY);
        }
        if !game.open() {
            self.gate.draw(world(at(b'G'), 1.95), Quat::IDENTITY);
        }
        // Full ambient keeps crystal facets luminous without adding a texture or post-process.
        material.begin(LitSettings {
            ambient: 1.0,
            ..settings
        });
        self.pulse
            .begin([[time, 0., 0., 0.], [0.; 4], [0.; 4], [0.; 4]])
            .unwrap();
        for (i, &p) in game.crystals.iter().enumerate() {
            if game.collected[i] {
                continue;
            }
            let height = 0.95 + (time * 1.6 + i as f32).sin() * 0.09;
            let rotation = Quat::from_rotation_y(time * 0.22 + i as f32);
            self.crystal.draw(world(p, height), rotation);
            self.tip.draw(world(p, height + 0.65), rotation);
            self.tip.draw(
                world(p, height - 0.65),
                rotation * Quat::from_rotation_x(std::f32::consts::PI),
            );
        }
        self.pulse.end();
        for (index, marker) in b"AB".iter().enumerate() {
            let p = at(*marker);
            for bar in 0..=index {
                self.rune.draw(
                    world(
                        p + vec2((bar as f32 - index as f32 * 0.5) * 0.19, 0.0),
                        0.665,
                    ),
                    Quat::IDENTITY,
                );
            }
            if game.switches as usize > index {
                self.tip
                    .draw(world(p, 1.2), Quat::from_rotation_y(time * 0.3));
            }
        }
        if !game.open() {
            for i in 0..3 {
                self.rune.draw(
                    world(at(b'G') + vec2((i as f32 - 1.0) * 0.35, 0.22), 1.65),
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                );
            }
        }
        self.daylight
            .draw(world(at(b'X') + vec2(0.0, -0.7), 1.75), Quat::IDENTITY);
        material.end();
    }
}

fn text_center(text: &str, y: f32, size: u16, color: Color) {
    let x = (screen_width() - measure_text(text, None, size, 1.0).width) * 0.5;
    draw_text(text, x + 1.0, y + 1.0, size as f32, BLACK);
    draw_text(text, x, y, size as f32, color);
}
fn window_conf() -> Conf {
    ferravex_render::window_conf("Ferravex / Crystal Cavern")
}
ferravex_render::game!(window_conf, game);
async fn game() {
    let font = get_default_font();
    let characters: Vec<char> = (' '..='~').collect();
    for size in [17, 20, 22, 23, 24, 38] {
        font.populate_font_cache(&characters, size);
    }
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut score = ferravex_render::audio::AdaptiveScore::new(90, 17).unwrap();
    use ferravex_play::audio::{AcousticProfile, AcousticScene, AcousticZone};
    let acoustics = AcousticScene::new([AcousticZone {
        min: vec3(0., -2., 19.),
        max: vec3(35., 6., 40.),
        feather: 4.,
        profile: AcousticProfile {
            gain: 0.9,
            low_pass_hz: 3500.,
        },
    }])
    .unwrap();
    ferravex_render::set_quality(ferravex_render::lighting::Quality::Medium);
    let mut game = Game::new();
    let mut view_rig = Rig3::new(Settings3 {
        position_half_life: 0.,
        target_half_life: 0.,
        fov_half_life: 0.,
        ..Default::default()
    })
    .unwrap();
    let scene = Scene::new();
    let material = LitMaterial::new().expect("Ferravex cave shader");
    let mut clock = FixedClock::new(60.0, 8);
    let mut pilot = Pilot::default();
    let mut pitch = -0.03_f32;
    let mut player_input = PlayerInput::preset("crystal-cavern", Preset::Explore);
    player_input.set_mouse_look_button(Some(1)).unwrap();
    for (name, key) in [
        ("move_left", KeyCode::A),
        ("move_right", KeyCode::D),
        ("move_forward", KeyCode::W),
        ("move_back", KeyCode::S),
    ] {
        let mut bindings = player_input.bindings(name).unwrap().to_vec();
        bindings.retain(|b| {
            !matches!(b.code, InputCode::Key(_)) || b.code == InputCode::Key(key as u16)
        });
        player_input.set_default_bindings(name, bindings).unwrap();
    }
    for (name, key, scale) in [
        ("look_rate_x", KeyCode::Left, -0.72),
        ("look_rate_x", KeyCode::Right, 0.72),
        ("look_rate_y", KeyCode::Up, -0.5),
        ("look_rate_y", KeyCode::Down, 0.5),
    ] {
        player_input
            .add_default_binding(
                name,
                Binding {
                    code: InputCode::Key(key as u16),
                    scale,
                    deadzone: 0.,
                },
            )
            .unwrap();
    }
    player_input
        .global_action(
            "help",
            "Toggle help",
            ActionKind::Button,
            vec![Binding::digital(InputCode::Key(KeyCode::H as u16))],
        )
        .unwrap();
    let mut help = true;
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[17, 20, 22, 23, 24, 38]);
        let w = screen_width();
        let h = screen_height();
        player_input
            .set_touch_layout(&[
                Control {
                    region: Region::Circle {
                        center: vec2(w - 65., h - 90.),
                        radius: 48.,
                    },
                    output: Output::Button(4),
                },
                Control {
                    region: Region::Rect(Rect::new(0., 0., w * 0.42, h)),
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: 54.,
                        floating: true,
                    },
                },
                Control {
                    region: Region::Rect(Rect::new(w * 0.42, 0., w * 0.58, h)),
                    output: Output::Look { x: 0, y: 1 },
                },
            ])
            .unwrap();
        player_input.sample_play();
        let dt = if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        let restarted =
            player_input.pressed("restart") || game.won && player_input.pressed("confirm");
        if restarted {
            game = Game::new();
            clock.reset();
            pilot = Pilot::default();
            view_rig.cut();
            pitch = -0.03;
        }
        if player_input.pressed("help") {
            help = !help;
        }
        let movement = player_input.movement();
        let look = player_input.look(dt);
        if !demo.autoplay && !restarted {
            game.yaw += look.x;
            pitch = (pitch - look.y).clamp(-0.8, 0.8);
        }
        let mut input = Input {
            movement: if restarted { Vec2::ZERO } else { movement },
            ..Default::default()
        };
        let frame = clock.advance(dt as f64);
        for _ in 0..frame.steps {
            if demo.autoplay {
                input = pilot.input(&game);
            } else {
                input.interact = player_input.pressed("interact");
            }
            let collected = game.count();
            let switches = game.switches;
            let won = game.won;
            game.step(input, frame.dt);
            if game.count() > collected {
                sound.cue(Cue::Pickup, 0.55);
            }
            if game.switches != switches {
                sound.cue(Cue::Impact(Surface::Metal), 0.4);
            }
            if game.won && !won {
                sound.cue(Cue::Success, 0.65);
            }
        }
        sound.set_acoustics(acoustics.sample(world(game.position, 0.), false).unwrap());
        score.update(dt.min(0.25), (game.count() as f32 / 3.).clamp(0., 1.));
        sound.walk(
            world(game.position, 0.),
            true,
            !game.won && (demo.autoplay || player_input.gameplay_active()),
            Surface::Stone,
            1.45,
        );
        clear_background(INK);
        let view = view_rig
            .update(
                dt,
                Input3 {
                    target: Target3::at(world(game.position, 0.)),
                    mode: Mode3::FirstPerson {
                        eye_offset: Vec3::Y * 1.55,
                        yaw: game.yaw,
                        pitch,
                    },
                    fov_y: 67_f32.to_radians(),
                },
                &NoObstructions,
            )
            .expect("valid cavern view")
            .pose;
        let eye = view.position;
        set_camera(&Camera3D {
            position: eye,
            target: view.target,
            up: view.up,
            fovy: view.fov_y,
            z_near: 0.06,
            z_far: 80.,
            ..Default::default()
        });
        scene.draw(&game, &material, eye, demo.time());
        set_default_camera();
        draw_text("CRYSTAL CAVERN", 26.0, 36.0, 22.0, WARM);
        draw_text(&format!("{} / 3", game.count()), 26.0, 63.0, 24.0, MINT);
        if !game.won {
            draw_circle(
                screen_width() * 0.5,
                screen_height() * 0.5,
                1.7,
                Color::new(0.8, 0.9, 0.85, 0.6),
            );
            if let Some(prompt) = game.prompt() {
                text_center(prompt, screen_height() * 0.65, 23, MINT);
            }
            if game.notice_time > 0.0 {
                text_center(game.notice, screen_height() - 70.0, 20, WARM);
            }
            if help {
                text_center(
                    "WASD  Move    Arrows / drag  Look    E  Interact    R  Restart    H  Hide help",
                    screen_height() - 28.0,
                    17,
                    Color::new(0.78, 0.79, 0.74, 1.0),
                );
            }
        } else {
            draw_rectangle(
                0.0,
                screen_height() * 0.34,
                screen_width(),
                150.0,
                Color::new(0.03, 0.08, 0.08, 0.86),
            );
            text_center("DAYLIGHT", screen_height() * 0.34 + 52.0, 38, MINT);
            text_center(
                &format!("Three crystals recovered  /  {:.0}s", game.elapsed),
                screen_height() * 0.34 + 86.0,
                22,
                WARM,
            );
            text_center(
                "R  /  Explore again",
                screen_height() * 0.34 + 120.0,
                20,
                WHITE,
            );
        }
        if player_input.touch_seen() && !player_input.touch_customized() && !game.won {
            let (origin, delta) = player_input
                .touch_stick(0, 1)
                .unwrap_or((vec2(90., h - 105.), Vec2::ZERO));
            let tint = Color::new(0.8, 0.9, 0.85, 0.45);
            draw_circle_lines(origin.x, origin.y, 54., 2., tint);
            draw_circle(
                origin.x + delta.x * 38.,
                origin.y + delta.y * 38.,
                15.,
                tint,
            );
            draw_circle_lines(w - 65., h - 90., 44., 2., tint);
            draw_text("USE", w - 82., h - 84., 17., tint);
        }
        player_input.draw_settings();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
    if demo.autoplay {
        println!(
            "Crystal Cavern: crystals={}, altars={}, won={}, elapsed={:.2}",
            game.count(),
            game.switches,
            game.won,
            game.elapsed
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exploration_accelerates_quickly_and_release_has_a_short_stop() {
        let mut game = Game::new();
        for _ in 0..20 {
            game.step(
                Input {
                    movement: Vec2::Y,
                    ..Default::default()
                },
                1. / 120.,
            );
        }
        assert!(game.velocity.length() > SPEED * 0.9);
        let released = game.position;
        for _ in 0..45 {
            game.step(Input::default(), 1. / 120.);
        }
        assert!(game.velocity.length() < 0.01);
        assert!(released.distance(game.position) < 0.25);
    }

    #[test]
    fn walls_block_even_large_steps() {
        let mut game = Game::new();
        game.step(
            Input {
                movement: vec2(1.0, 0.0),
                ..Input::default()
            },
            10.0,
        );
        assert!(game.position.x < center(8, 13).x - TILE * 0.5 - RADIUS);
        assert!(game.position.x > at(b'S').x);
    }
    #[test]
    fn objective_checkpoint_resumes_the_actual_altar_sequence() {
        use ferravex_play::{objective::ObjectiveSnapshot, persistence::SaveData};
        let mut game = Game::new();
        for p in game.crystals.clone() {
            game.position = p;
            game.interact();
        }
        game.position = at(b'A');
        game.interact();
        let bytes = game.objectives.snapshot().encode(1).unwrap();
        let mut restored = Game::new();
        restored
            .objectives
            .restore(ObjectiveSnapshot::decode(&bytes).unwrap().0)
            .unwrap();
        restored.sync_objectives();
        assert_eq!(restored.count(), 3);
        assert_eq!(restored.switches, 1);
        restored.position = at(b'B');
        restored.interact();
        assert!(restored.open());
        restored.position = at(b'X');
        restored.step(Input::default(), 1. / 120.);
        assert!(restored.won);
    }
    #[test]
    fn gate_requires_crystals_and_ordered_altars() {
        let mut game = Game::new();
        game.position = at(b'B');
        game.interact();
        assert_eq!(game.switches, 0);
        game.position = at(b'A');
        game.interact();
        assert_eq!(game.switches, 0);
        for p in game.crystals.clone() {
            game.position = p;
            game.interact();
        }
        assert_eq!(game.count(), 3);
        game.position = at(b'B');
        game.interact();
        assert!(!game.open());
        game.position = at(b'A');
        game.interact();
        assert_eq!(game.switches, 1);
        game.position = at(b'B');
        game.interact();
        assert!(game.open());
    }
    #[test]
    fn sealed_gate_blocks_exit_and_interaction_through_walls() {
        let mut game = Game::new();
        game.position = center(6, 4);
        game.step(
            Input {
                movement: Vec2::Y,
                ..Input::default()
            },
            2.0,
        );
        assert!(game.position.y > game.gate.max.y + RADIUS);
        assert!(!game.won);
        game.position = center(2, 7);
        assert!(!game.visible(game.crystals[0]));
    }
    #[test]
    fn autoplay_solves_with_player_inputs_and_restart_clears_state() {
        let mut game = Game::new();
        let mut pilot = Pilot::default();
        for _ in 0..12_000 {
            game.step(pilot.input(&game), 1.0 / 60.0);
            if game.won {
                break;
            }
        }
        assert!(
            game.won,
            "position={:?}, crystals={}, switches={}",
            game.position,
            game.count(),
            game.switches
        );
        assert!(game.elapsed < 120.0);
        game = Game::new();
        assert_eq!(game.count(), 0);
        assert_eq!(game.switches, 0);
        assert!(!game.won);
        assert_eq!(game.position, at(b'S'));
    }
}
