//! A small cave expedition: recover three crystals, wake two altars, find daylight.
//!
//! The host uses the shared session clock; the objective graph owns puzzle rules.
//! Inactive frames never advance the expedition timer:
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! let mut session = GameSession::new(60.0, 8, false);
//! assert_eq!(session.state(), SessionState::Ready);
//! session.start();
//! assert_eq!(session.advance(1.0 / 60.0).steps, 1);
//! session.set_paused(true);
//! assert_eq!(session.advance(30.0).steps, 0);
//! session.restart();
//! assert_eq!(session.tick(), 0);
//! ```
use ferravex_core::session::{GameSession, SessionState};
use ferravex_physics::character::{
    CharacterSettings, CharacterShape, Obstacle, ObstacleShape, QueryBounds, Terrain,
    move_and_slide,
};
use ferravex_play::ai::{Candidate, Observer, Sight, TargetSelector, VisibilityQuery, WorkBudget};
use ferravex_play::camera::{Input3, Mode3, NoObstructions, Rig3, Settings3, Target3};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::objective::{ObjectiveProgress, ObjectiveState};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interactable {
    Crystal(usize),
    Altar(u8),
    Gate,
}
impl Interactable {
    fn objective(self) -> usize {
        match self {
            Self::Crystal(i) => i,
            Self::Altar(i) => 3 + usize::from(i),
            Self::Gate => 5,
        }
    }
    fn from_id(id: u64) -> Option<Self> {
        match id {
            1..=3 => Some(Self::Crystal(id as usize - 1)),
            4..=5 => Some(Self::Altar(id as u8 - 4)),
            6 => Some(Self::Gate),
            _ => None,
        }
    }
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
    focus: TargetSelector,
    switches: u8,
    gate: Aabb2,
    won: bool,
    elapsed: f32,
    notice: String,
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
            // A challenger must be more than 20 cm closer. Range/visibility
            // remain mandatory, and facing never restricts nearby exploration.
            focus: TargetSelector::new(0.2).expect("finite focus margin"),
            switches: 0,
            gate: Aabb2::from_min_size(at(b'G') - vec2(TILE * 0.5, 0.175), vec2(TILE, 0.35)),
            won: false,
            elapsed: 0.0,
            notice: "Recover three crystals. Wake the numbered altars.".into(),
            notice_time: 8.0,
        }
    }
    fn count(&self) -> usize {
        self.collected.iter().filter(|&&c| c).count()
    }
    fn open(&self) -> bool {
        matches!(
            self.objectives.state(5),
            Some(ObjectiveState::Ready | ObjectiveState::Complete)
        )
    }
    fn crystal_progress(&self) -> (u16, u16) {
        self.objectives
            .iter()
            .take(3)
            .fold((0, 0), |(done, total), view| {
                let ObjectiveProgress::Event { count, required } = view.progress else {
                    unreachable!("crystals are authored event objectives");
                };
                (done + count, total + required)
            })
    }
    fn completed_mask(&self) -> u64 {
        self.objectives
            .iter()
            .filter(|view| view.state == ObjectiveState::Complete)
            .fold(0, |mask, view| mask | 1 << view.index)
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
    fn message(&mut self, text: impl Into<String>) {
        self.notice = text.into();
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
    fn target_position(&self, target: Interactable) -> Vec2 {
        match target {
            Interactable::Crystal(i) => self.crystals[i],
            Interactable::Altar(i) => at(if i == 0 { b'A' } else { b'B' }),
            // Query the near face, not the center of the solid closed gate.
            Interactable::Gate => at(b'G') + vec2(0., 0.22),
        }
    }
    fn target_ready(&self, target: Interactable) -> bool {
        self.objectives
            .inspect(target.objective())
            .is_some_and(|view| view.state == ObjectiveState::Ready)
    }
    // The prompt, world highlight and accepted interaction share one target.
    fn target(&self) -> Option<Interactable> {
        self.select_focus().1
    }
    fn select_focus(&self) -> (TargetSelector, Option<Interactable>) {
        let mut selector = self.focus;
        if self.won {
            selector.reset();
            return (selector, None);
        }
        let candidates = self
            .crystals
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.collected[*i])
            .map(|(i, _)| Interactable::Crystal(i))
            .chain((self.switches..2).map(Interactable::Altar))
            .chain(std::iter::once(Interactable::Gate));
        let candidates = candidates
            .map(|target| Candidate {
                id: target.objective() as u64 + 1,
                position: world(self.target_position(target), 0.),
                groups: 1,
            })
            .filter(|candidate| {
                // Preserve the original strict interaction distance at its boundary.
                candidate
                    .position
                    .distance_squared(world(self.position, 0.))
                    < 1.7 * 1.7
            });
        let report = selector
            .select_visible(
                candidates,
                Observer {
                    position: world(self.position, 0.),
                    forward: Vec3::Z,
                },
                Sight::new(1.7, std::f32::consts::TAU, 1).expect("cave interaction range"),
                &mut WorkBudget::new(6, 6, 0),
                &mut CaveVisibility(self),
                |candidate| -candidate.position.distance(world(self.position, 0.)),
            )
            .expect("finite authored cave candidates");
        debug_assert!(
            !report.incomplete,
            "all six cave objects fit the focus budget"
        );
        (
            selector,
            report
                .target
                .and_then(|target| Interactable::from_id(target.id)),
        )
    }
    fn objective_hint(&self) -> String {
        let (count, required) = self.crystal_progress();
        match self
            .objectives
            .iter()
            .find(|view| view.state != ObjectiveState::Complete)
            .map(|view| view.index)
        {
            Some(0..=2) => format!(
                "Search the side chambers: {} / {required} crystals remain",
                required - count
            ),
            Some(3) => "Find altar I in the northern chamber".into(),
            Some(4) => "Altar I is awake. Find altar II".into(),
            Some(_) => "Seal open. Walk through to the daylight".into(),
            None => "Expedition complete".into(),
        }
    }
    fn blocker(&self, target: Interactable) -> String {
        let view = self
            .objectives
            .inspect(target.objective())
            .expect("authored objective");
        let missing = view.blockers.missing_all;
        let (count, required) = self.crystal_progress();
        if missing & 7 != 0 {
            format!("Altar I locked / Three crystals required ({count} / {required} recovered)")
        } else if missing & (1 << 3) != 0 {
            format!("Altar II locked / Wake altar I first ({count} / {required} crystals)")
        } else {
            format!(
                "Seal locked / Awaken both numbered altars ({} / 2 awake)",
                self.switches
            )
        }
    }
    fn interact(&mut self) {
        let (focus, target) = self.select_focus();
        self.focus = focus;
        let Some(target) = target else {
            return;
        };
        if !self.target_ready(target) {
            self.message(self.blocker(target));
            return;
        }
        match target {
            Interactable::Crystal(i) => {
                self.objective_event(i);
                self.message(match self.count() {
                    1 => "Crystal 1 / 3 recovered. Two remain in the side chambers.",
                    2 => "Crystal 2 / 3 recovered. One more to find.",
                    _ => "All three recovered. Find altar I, then altar II.",
                });
            }
            Interactable::Altar(0) => {
                self.objective_event(3);
                self.message("Altar I awakened. The second altar is ready.");
            }
            Interactable::Altar(_) => {
                self.objective_event(4);
                self.message("The seal is open. Walk through to the daylight.");
            }
            Interactable::Gate => self.message(self.objective_hint()),
        }
    }
    fn prompt(&self, interact_hint: &str) -> Option<String> {
        self.target().map(|target| {
            if !self.target_ready(target) {
                return self.blocker(target);
            }
            match target {
                Interactable::Crystal(_) => format!("{interact_hint} - Recover crystal"),
                Interactable::Altar(0) => format!("{interact_hint} - Awaken altar I"),
                Interactable::Altar(_) => format!("{interact_hint} - Awaken altar II"),
                Interactable::Gate => "Seal open / Walk through to daylight".into(),
            }
        })
    }
    fn step(&mut self, input: Input, dt: f32) {
        if self.won {
            return;
        }
        self.notice_time = (self.notice_time - dt).max(0.0);
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
        self.focus = self.select_focus().0;
        if input.interact {
            self.interact();
        }
        if self.open() && self.position.distance(at(b'X')) < 1.2 {
            self.objective_event(5);
            self.message("Expedition complete.");
        }
    }
}

#[derive(Default)]
struct Progress {
    completed: u64,
    pickups: usize,
    altars: u8,
    won: bool,
}

// Host lifecycle stays testable without a window, audio device or renderer.
struct Expedition {
    game: Game,
    session: GameSession,
    pilot: Pilot,
    autoplay: bool,
    discard_frame: bool,
}
impl Expedition {
    fn new(autoplay: bool) -> Self {
        Self {
            game: Game::new(),
            session: GameSession::new(60., 8, autoplay),
            pilot: Pilot::default(),
            autoplay,
            discard_frame: false,
        }
    }
    fn start(&mut self) {
        if self.session.start() {
            self.discard_frame = true;
        }
    }
    fn restart(&mut self) {
        self.game = Game::new();
        self.game
            .message("Fresh expedition. All crystals and altars reset.");
        self.session.restart();
        self.pilot = Pilot::default();
        self.discard_frame = true;
    }
    fn set_paused(&mut self, paused: bool) {
        if self.session.set_paused(paused) {
            self.game.velocity = Vec2::ZERO;
            // A browser may deliver a long frame on refocus. Discard that frame
            // as well as all paused frames, including its stale input edges.
            self.discard_frame = true;
        }
    }
    fn advance(&mut self, elapsed: f64, sample_input: impl FnOnce() -> Input) -> Progress {
        let mut progress = Progress::default();
        if !self.session.is_playing() {
            return progress;
        }
        if std::mem::take(&mut self.discard_frame) {
            return progress;
        }
        let frame = self.session.advance(elapsed);
        if frame.steps == 0 {
            return progress;
        }
        // Leave pending edges in PlayerInput until a real tick. Its existing
        // focus/context handling then also clears taps across host suspension.
        let mut input = sample_input();
        let previous_completed = self.game.completed_mask();
        for _ in 0..frame.steps {
            let command = if self.autoplay {
                self.pilot.input(&self.game)
            } else {
                Input {
                    // One render edge is spent on one tick, even if another
                    // interactable becomes reachable during catch-up.
                    interact: std::mem::take(&mut input.interact),
                    ..input
                }
            };
            let collected = self.game.count();
            let switches = self.game.switches;
            self.game.step(command, frame.dt);
            progress.pickups += self.game.count() - collected;
            progress.altars += self.game.switches - switches;
            if self.game.won {
                progress.won = self.session.finish(true);
                break;
            }
        }
        progress.completed = self.game.completed_mask() & !previous_completed;
        progress
    }
}

struct CaveVisibility<'a>(&'a Game);
impl VisibilityQuery for CaveVisibility<'_> {
    fn clear(&mut self, from: Vec3, to: Vec3) -> bool {
        !self.0.obstacles().any(|wall| {
            wall.cast_segment(vec2(from.x, from.z), vec2(to.x, to.z))
                .is_some()
        })
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

// Cosmetic phases are separate from expedition time. Suppressed time is never
// accumulated, so resuming a preference cannot jump through hidden animation.
// There are no transient flashes, particles, camera impulses or FeedbackController
// in this exploration game; accepted audio remains independent of these phases.
#[derive(Default)]
struct Presentation {
    motion_time: f32,
    glow_time: f32,
    pulse: bool,
}
impl Presentation {
    fn advance(&mut self, elapsed: f32, policy: FeedbackPolicy) {
        let dt = if elapsed.is_finite() {
            elapsed.clamp(0., 0.25)
        } else {
            0.
        };
        if !policy.reduced_motion() {
            self.motion_time += dt * policy.intensity();
        }
        self.pulse = !policy.reduced_flash() && policy.intensity() > 0.;
        if self.pulse {
            self.glow_time += dt * policy.intensity();
        }
    }
}

// Formatting is bounded and reads live bindings without consuming input. A long
// binding list points to Controls instead of truncating into a misleading key.
fn action_hint(input: &PlayerInput, name: &str) -> String {
    // Leave room for the action in a 21-column, 320px overlay. Larger lists
    // remain available in Controls rather than displacing essential guidance.
    let mut bytes = [0; 12];
    input
        .format_hint(name, &mut bytes)
        .unwrap_or("See Controls")
        .into()
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
    fn draw(
        &self,
        game: &Game,
        material: &LitMaterial,
        eye: Vec3,
        presentation: &Presentation,
        active: bool,
    ) {
        let time = presentation.motion_time;
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
        // Bypass the authored pulse shader for reduced flash. Freezing its time
        // alone still pulses as bobbing changes the shader's world.y input.
        if presentation.pulse {
            self.pulse
                .begin([
                    [presentation.glow_time, 0., 0., 0.],
                    [0.; 4],
                    [0.; 4],
                    [0.; 4],
                ])
                .unwrap();
        }
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
        if presentation.pulse {
            self.pulse.end();
        }
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
        if active && let Some(target) = game.target() {
            let p = game.target_position(target);
            let color = if game.target_ready(target) {
                MINT
            } else {
                WARM
            };
            // A steady ground ring identifies the exact prompted object.
            // Depth-tested geometry cannot reveal targets through cave walls.
            for segment in 0..24 {
                let point = |n: u32| {
                    let angle = n as f32 * std::f32::consts::TAU / 24.;
                    world(p + vec2(angle.cos(), angle.sin()) * 0.85, 0.035)
                };
                draw_line_3d(point(segment), point(segment + 1), color);
            }
        }
        material.end();
    }
}

fn text_center(text: &str, y: f32, size: u16, color: Color) {
    let width = get_default_font()
        .try_metrics(size, 1.)
        .expect("nonzero HUD size")
        .measure_line(text)
        .expect("bounded HUD text")
        .width
        .max(1.);
    let scale = ((screen_width() - 32.).max(1.) / width).min(1.);
    let x = (screen_width() - width * scale) * 0.5;
    for (offset, tint) in [(1., BLACK), (0., color)] {
        draw_text_ex(
            text,
            x + offset,
            y + offset,
            TextParams {
                font_size: size,
                font_scale: scale,
                color: tint,
            },
        );
    }
}
fn wrapped_text(ui: &mut ferravex_render::ui::Ui, text: &str, top: f32, height: f32, color: Color) {
    use ferravex_render::ui::{Align, TextStyle, UiRect};
    ui.text(
        UiRect {
            min: [16., top],
            size: [(screen_width() - 32.).max(1.), height],
        },
        text,
        TextStyle {
            size: 18.,
            color,
            align: Align::Center,
            wrap: true,
            shadow: true,
        },
    )
    .expect("bounded expedition overlay");
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
    let mut expedition = Expedition::new(demo.autoplay);
    let mut presentation = Presentation::default();
    let mut overlay_text = ferravex_render::ui::Ui::new();
    let mut view_rig = Rig3::new(Settings3 {
        position_half_life: 0.,
        target_half_life: 0.,
        fov_half_life: 0.,
        ..Default::default()
    })
    .unwrap();
    let scene = Scene::new();
    let material = LitMaterial::new().expect("Ferravex cave shader");
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
    let mut confirm_requested = false;
    loop {
        // Read on every presentation frame, before accepted feedback and draws,
        // including Ready, Controls, paused and frames with no fixed update.
        let policy = ferravex_render::preferences::feedback_policy();
        ferravex_render::ui::warm_up_font_sizes(&[17, 20, 22, 23, 24, 38]);
        let w = screen_width();
        let h = screen_height();
        overlay_text.begin(
            ferravex_render::ui::Viewport::responsive([w.max(1.), h.max(1.)], [0.; 4])
                .expect("positive cave viewport"),
        );
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
        let clicked_confirm = std::mem::take(&mut confirm_requested);
        let confirm = player_input.pressed("confirm") || clicked_confirm;
        let activate = confirm && !expedition.session.is_playing();
        let restarted = player_input.pressed("restart")
            || expedition.session.state() == SessionState::Won && confirm;
        if restarted {
            expedition.restart();
            presentation = Presentation::default();
            sound.reset();
            view_rig.cut();
            pitch = -0.03;
        } else if confirm && expedition.session.state() == SessionState::Ready {
            expedition.start();
        }
        if restarted || activate {
            // Suppress keys held across start/retry/resume, then restore the
            // gameplay context. PlayerInput retains the manual pause reason.
            player_input.set_paused(true);
            player_input.set_paused(false);
            player_input.sample(true);
        }
        expedition.set_paused(!demo.autoplay && !player_input.gameplay_active());
        if player_input.pressed("help") {
            help = !help;
        }
        let elapsed = demo.dt();
        let dt = if expedition.session.is_playing() && !expedition.discard_frame {
            elapsed
        } else {
            0.
        };
        let movement = player_input.movement();
        let look = player_input.look(dt);
        if !demo.autoplay && expedition.session.is_playing() && !expedition.discard_frame {
            expedition.game.yaw += look.x;
            pitch = (pitch - look.y).clamp(-0.8, 0.8);
        }
        let previous_elapsed = expedition.game.elapsed;
        let progress = expedition.advance(elapsed as f64, || Input {
            movement,
            interact: player_input.pressed("interact"),
            ..Default::default()
        });
        presentation.advance(expedition.game.elapsed - previous_elapsed, policy);
        for index in 0..6 {
            if progress.completed & (1 << index) != 0 {
                ferravex_render::performance::event(
                    ferravex_play::performance::DiagnosticKind::Objective,
                    index as f32,
                );
            }
        }
        if progress.pickups > 0 {
            sound.cue(Cue::Pickup, 0.55);
        }
        if progress.altars > 0 {
            sound.cue(Cue::Impact(Surface::Metal), 0.4);
        }
        if progress.won {
            sound.cue(Cue::Success, 0.65);
        }
        let game = &expedition.game;
        sound.set_acoustics(acoustics.sample(world(game.position, 0.), false).unwrap());
        score.update(dt.min(0.25), (game.count() as f32 / 3.).clamp(0., 1.));
        sound.walk(
            world(game.position, 0.),
            true,
            expedition.session.is_playing(),
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
        scene.draw(
            game,
            &material,
            eye,
            &presentation,
            expedition.session.is_playing(),
        );
        set_default_camera();
        let use_hint = action_hint(&player_input, "interact");
        let confirm_hint = action_hint(&player_input, "confirm");
        let retry_hint = action_hint(&player_input, "restart");
        let pause_hint = action_hint(&player_input, "pause");
        let help_hint = action_hint(&player_input, "help");
        draw_rectangle(0., 0., w, 105., Color::new(0.03, 0.08, 0.08, 0.8));
        draw_text("CRYSTAL CAVERN", 26.0, 36.0, 22.0, WARM);
        text_center(
            &format!(
                "Crystals {} / 3    Altars {} / 2    Seal {}",
                game.count(),
                game.switches,
                if game.open() { "OPEN" } else { "LOCKED" }
            ),
            64.,
            20,
            MINT,
        );
        text_center(&game.objective_hint(), 90., 17, WHITE);
        // A steady six-part bar includes walking into daylight, not just opening
        // the seal. It reads accepted graph completion without mutating it.
        let bar_width = (w - 32.).max(1.);
        for view in game.objectives.iter() {
            draw_rectangle(
                16. + view.index as f32 * bar_width / 6.,
                100.,
                (bar_width / 6. - 2.).max(1.),
                3.,
                if view.state == ObjectiveState::Complete {
                    MINT
                } else {
                    Color::new(0.2, 0.3, 0.3, 1.)
                },
            );
        }
        if expedition.session.is_playing() {
            let target_color = game
                .target()
                .map_or(Color::new(0.8, 0.9, 0.85, 0.6), |target| {
                    if game.target_ready(target) {
                        MINT
                    } else {
                        WARM
                    }
                });
            draw_circle(
                screen_width() * 0.5,
                screen_height() * 0.5,
                if game.target().is_some() { 3. } else { 1.7 },
                target_color,
            );
            if let Some(prompt) = game.prompt(&use_hint) {
                // Locked altar copy occupies four lines at 320px. Keep it
                // above the notice and preserve a gap before the help rows.
                let prompt_top = (h * 0.55).min(h - 230.).max(114.);
                draw_rectangle(
                    0.,
                    prompt_top - 8.,
                    w,
                    88.,
                    Color::new(0.03, 0.08, 0.08, 0.85),
                );
                wrapped_text(&mut overlay_text, &prompt, prompt_top, 72., target_color);
            }
            if game.notice_time > 0.0 {
                wrapped_text(&mut overlay_text, &game.notice, h - 150., 72., WARM);
            }
            if help {
                text_center(
                    &format!("{use_hint}: Use    {pause_hint}: Pause"),
                    h - 56.,
                    17,
                    Color::new(0.78, 0.79, 0.74, 1.0),
                );
                text_center(
                    &format!("{retry_hint}: Retry    {help_hint}: Help"),
                    h - 34.,
                    17,
                    WHITE,
                );
                text_center("Move / look bindings: Controls (F2)", h - 12., 17, WHITE);
            }
        } else {
            let top = (h * 0.28).max(110.).min((h - 240.).max(0.));
            draw_rectangle(
                0.0,
                top,
                screen_width(),
                240.0,
                Color::new(0.03, 0.08, 0.08, 0.94),
            );
            let (title, detail, action) = match expedition.session.state() {
                SessionState::Ready => (
                    "THE EXPEDITION",
                    "Recover 3 crystals. Wake altar I, then II. Find daylight.".to_owned(),
                    "Begin expedition",
                ),
                SessionState::Paused => (
                    "EXPEDITION PAUSED",
                    "Your crystals and expedition time are safe.".to_owned(),
                    "Resume expedition",
                ),
                _ => (
                    "DAYLIGHT",
                    format!(
                        "All 6 objectives complete. Expedition time: {:.1}s",
                        game.elapsed
                    ),
                    "Explore again",
                ),
            };
            text_center(title, top + 45., 38, MINT);
            wrapped_text(&mut overlay_text, &detail, top + 62., 60., WARM);
            let guidance = match expedition.session.state() {
                SessionState::Ready => {
                    format!("{use_hint}: Use nearby.\nMove/look: Controls.")
                }
                SessionState::Paused => {
                    format!("{pause_hint}: Resume.\n{retry_hint}: Reset all.")
                }
                _ => format!("{retry_hint}: Retry.\nResets crystals,\naltars and time."),
            };
            wrapped_text(&mut overlay_text, &guidance, top + 120., 60., WHITE);
            confirm_requested |= ferravex_render::ui::button_with_options(
                Rect::new(
                    (w - (w - 32.).clamp(1., 320.)) * 0.5,
                    top + 184.,
                    (w - 32.).clamp(1., 320.),
                    44.,
                ),
                &format!("{confirm_hint} / {action}"),
                ferravex_render::ui::ButtonOptions {
                    enabled: !player_input.controls_open(),
                    selected: true,
                },
            );
        }
        if player_input.touch_seen()
            && !player_input.touch_customized()
            && expedition.session.is_playing()
        {
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
        overlay_text.finish(false);
        // finish only resolves UI input; queued descriptions, prompts and
        // notices need an explicit paint before the Controls overlay.
        overlay_text.paint();
        player_input.draw_controls();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
    if demo.autoplay {
        let game = &expedition.game;
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
mod journey200_138;

#[cfg(test)]
mod journey200_tests;

#[cfg(test)]
mod journey200_193;

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::input::{ActionMap, ContextId};

    fn interaction_map() -> (ActionMap, usize, InputCode) {
        let mut map = ActionMap::default();
        let key = InputCode::Key(KeyCode::E as u16);
        let action = map
            .named_action(
                "interact",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(key)],
            )
            .unwrap();
        (map, action, key)
    }

    fn playing() -> Expedition {
        let mut expedition = Expedition::new(false);
        expedition.start();
        expedition.advance(0., Input::default);
        expedition
    }

    #[test]
    fn ready_and_start_do_not_charge_menu_time_or_reuse_input() {
        let mut expedition = Expedition::new(false);
        expedition.game.position = expedition.game.crystals[0];
        let input = Input {
            interact: true,
            ..Default::default()
        };
        expedition.advance(90., || input);
        assert_eq!(expedition.session.state(), SessionState::Ready);
        assert_eq!(expedition.game.elapsed, 0.);
        assert_eq!(expedition.game.count(), 0);
        expedition.start();
        expedition.advance(90., || input);
        assert_eq!(expedition.session.tick(), 0);
        expedition.advance(1. / 60., Input::default);
        assert_eq!(expedition.session.tick(), 1);
        assert_eq!(expedition.game.count(), 0);
    }

    #[test]
    fn interaction_tap_survives_frames_without_a_tick() {
        let mut expedition = playing();
        expedition.game.position = expedition.game.crystals[0];
        let (mut map, action, key) = interaction_map();
        map.feed(key, 1.);
        map.feed(key, 0.);
        for _ in 0..2 {
            expedition.advance(0.001, || Input {
                interact: map.consume_tick(action).pressed,
                ..Default::default()
            });
        }
        assert_eq!(expedition.game.count(), 0);
        assert!(map.peek(action).pressed);
        let progress = expedition.advance(0.015, || Input {
            interact: map.consume_tick(action).pressed,
            ..Default::default()
        });
        assert_eq!(progress.pickups, 1);
        assert_eq!(expedition.game.count(), 1);
        assert!(!map.peek(action).pressed);
    }

    #[test]
    fn one_interaction_edge_cannot_collect_multiple_targets_during_catchup() {
        let mut expedition = playing();
        let p = expedition.game.crystals[0];
        expedition.game.crystals.fill(p);
        expedition.game.position = p;
        let input = Input {
            interact: true,
            ..Default::default()
        };
        let progress = expedition.advance(0.12, || input);
        assert!(expedition.session.tick() > 1);
        assert_eq!(progress.pickups, 1);
        assert_eq!(expedition.game.count(), 1);
        expedition.advance(0.1, Input::default);
        assert_eq!(expedition.game.count(), 1);
        expedition.advance(1. / 60., || input);
        assert_eq!(expedition.game.count(), 2);
    }

    #[test]
    fn pause_and_refocus_clear_pending_input_without_advancing_time() {
        let mut expedition = playing();
        expedition.game.position = expedition.game.crystals[0];
        expedition.game.velocity = Vec2::Y * SPEED;
        let input = Input {
            interact: true,
            movement: Vec2::Y,
            ..Default::default()
        };
        expedition.advance(0.001, || input);
        let position = expedition.game.position;
        let notice_time = expedition.game.notice_time;
        expedition.set_paused(true);
        assert_eq!(expedition.game.velocity, Vec2::ZERO);
        expedition.advance(120., || input);
        assert_eq!(expedition.game.elapsed, 0.);
        assert_eq!(expedition.game.notice_time, notice_time);
        assert_eq!(expedition.game.position, position);
        expedition.set_paused(false);
        expedition.advance(120., || input);
        assert_eq!(expedition.session.tick(), 0);
        expedition.advance(1. / 60., Input::default);
        assert_eq!(expedition.session.tick(), 1);
        assert_eq!(expedition.game.count(), 0);
        assert_eq!(expedition.game.position, position);
    }

    #[test]
    fn focus_loss_between_render_frames_discards_a_buffered_tap() {
        let mut expedition = playing();
        expedition.game.position = expedition.game.crystals[0];
        let (mut map, action, key) = interaction_map();
        map.feed(key, 1.);
        expedition.advance(0.001, || Input {
            interact: map.consume_tick(action).pressed,
            ..Default::default()
        });
        assert!(map.peek(action).pressed);
        // PlayerInput clears its map on a focus-epoch change even when the host
        // delivers no render frame while unfocused. No second latch survives.
        map.lose_focus();
        expedition.advance(1. / 60., || Input {
            interact: map.consume_tick(action).pressed,
            ..Default::default()
        });
        assert_eq!(expedition.game.count(), 0);
    }

    #[test]
    fn nearest_visible_target_drives_prompt_and_accepted_collection() {
        let mut game = Game::new();
        let p = game.crystals[0];
        game.crystals[1] = p + vec2(0.8, 0.);
        game.position = p + vec2(0.65, 0.);
        assert_eq!(game.target(), Some(Interactable::Crystal(1)));
        assert!(game.prompt("E").unwrap().contains("Recover crystal"));
        game.interact();
        assert!(!game.collected[0]);
        assert!(game.collected[1]);
        assert!(game.notice.contains("1 / 3"));
        let corner = center(5, 8);
        game.position = corner + vec2(-1.65, 1.);
        game.crystals[0] = corner + vec2(-1., 1.65);
        // Nearby floor points separated by the authored wall's corner.
        assert!(game.position.distance(game.crystals[0]) < 1.7);
        assert!(!game.visible(game.crystals[0]));
        assert_ne!(game.target(), Some(Interactable::Crystal(0)));
    }

    #[test]
    fn altar_and_gate_prompts_explain_prerequisites_and_exit() {
        let mut game = Game::new();
        game.position = at(b'A');
        assert!(!game.target_ready(game.target().unwrap()));
        assert!(
            game.prompt("E")
                .unwrap()
                .contains("Three crystals required")
        );
        game.interact();
        assert_eq!(game.switches, 0);
        for i in 0..3 {
            game.objective_event(i);
        }
        assert!(game.target_ready(game.target().unwrap()));
        assert!(game.prompt("E").unwrap().contains("Awaken altar I"));
        game.interact();
        assert_eq!(game.target(), None);
        game.position = at(b'B');
        assert!(game.prompt("E").unwrap().contains("Awaken altar II"));
        game.interact();
        assert!(game.open());
        game.position = at(b'G') + vec2(0., 1.);
        assert_eq!(game.target(), Some(Interactable::Gate));
        assert!(game.prompt("E").unwrap().contains("Walk through"));
        assert!(
            !game.won,
            "Opening the gate does not complete the expedition"
        );
    }

    #[test]
    fn completion_is_once_and_retry_resets_the_entire_expedition() {
        let mut expedition = playing();
        for i in 0..5 {
            expedition.game.objective_event(i);
        }
        expedition.game.position = at(b'X');
        let progress = expedition.advance(0.1, Input::default);
        assert!(progress.won);
        assert_eq!(expedition.session.state(), SessionState::Won);
        // Catch-up stops on the winning tick, not the end of the render frame.
        assert!((expedition.game.elapsed - 1. / 60.).abs() < 0.0001);
        let elapsed = expedition.game.elapsed;
        let notice_time = expedition.game.notice_time;
        expedition.set_paused(true);
        assert!(!expedition.advance(30., Input::default).won);
        assert_eq!(expedition.game.elapsed, elapsed);
        assert_eq!(expedition.game.notice_time, notice_time);
        assert_eq!(expedition.session.state(), SessionState::Won);
        expedition.restart();
        assert_eq!(expedition.session.state(), SessionState::Playing);
        assert_eq!(expedition.session.tick(), 0);
        assert_eq!(expedition.game.count(), 0);
        assert_eq!(expedition.game.switches, 0);
        assert_eq!(expedition.game.elapsed, 0.);
        assert_eq!(expedition.game.position, at(b'S'));
        assert_eq!(expedition.game.velocity, Vec2::ZERO);
        assert_eq!(expedition.game.yaw, 0.);
        assert!(!expedition.game.open());
        assert!(expedition.pilot.path.is_empty());
        assert!(expedition.pilot.goal.is_none());
    }

    #[test]
    fn autoplay_completes_through_the_session_with_irregular_frames() {
        let mut expedition = Expedition::new(true);
        for frame in 0..12000 {
            expedition.advance([1. / 144., 1. / 30., 0.07][frame % 3], Input::default);
            if expedition.session.state() == SessionState::Won {
                break;
            }
        }
        assert_eq!(expedition.session.state(), SessionState::Won);
        assert_eq!(expedition.game.count(), 3);
        assert_eq!(expedition.game.switches, 2);
        assert!(expedition.game.elapsed < 120.);
    }

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
