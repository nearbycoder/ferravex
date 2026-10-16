//! Playable first-party reference scene. Platform code consumes the same Rust state on native/Web.
#[cfg(target_arch = "wasm32")]
mod interactive;
mod peer;
use ferravex_core::FixedClock;
use ferravex_engine::play::particles::{ParticleDesc, ParticlePool};
use ferravex_engine::render::audio::{self, Cue, GameAudio, Surface};
use ferravex_engine::{
    dynamics::{BodyDesc, BodyHandle, PhysicsWorld, Shape},
    replay::{Frame, Replay, checksum},
};
use ferravex_fps::{Arena, GameEvent, InputFrame, Match, forward};
use ferravex_graphics::{
    Camera, ClipDepth, Color, Mat4, Mesh, MeshCache, MeshId, RenderItem, SoftwareRenderer, Vec3,
    vec3,
};
use std::sync::Mutex;
#[derive(Clone)]
struct Object {
    mesh: MeshId,
    position: Vec3,
    scale: Vec3,
    solid: Option<usize>,
}
pub struct Lab {
    pub game: Match,
    pub cache: MeshCache,
    objects: Vec<Object>,
    physics: PhysicsWorld<3>,
    balls: Vec<(BodyHandle, MeshId)>,
    terrain: Vec<BodyHandle>,
    clock: FixedClock,
    time: f32,
    pub camera: Camera,
    pub items: Vec<RenderItem>,
    pub instances: Vec<f32>,
    pub shadow_instances: Vec<f32>,
    pub matrix: [f32; 16],
    pub light: [f32; 16],
    pub status: [f32; 11],
    meshes: Vec<MeshId>,
    mesh_data: Vec<Vec<f32>>,
    sound: GameAudio,
    last_reload: u16,
    pub audio: Vec<[f32; 2]>,
    replay: Replay,
    player_mesh: MeshId,
    head_mesh: MeshId,
    pub kills: u32,
    recording: bool,
    export: Vec<u8>,
    particles: ParticlePool,
    peer: peer::Peer,
    network_out: Vec<u8>,
    network_in: Vec<u8>,
}
impl Default for Lab {
    fn default() -> Self {
        Self::new()
    }
}
impl Lab {
    pub fn new() -> Self {
        ferravex_engine::render::set_quality(ferravex_engine::render::lighting::Quality::Medium);
        let mut cache = MeshCache::new();
        let colors = [
            0xcbbfa6, 0xa29071, 0x586b67, 0x8c613c, 0x3d4547, 0xc58658, 0xe4d7bd, 0x6b7650,
            0x4d6875, 0xd3b56b,
        ];
        let mut meshes: Vec<_> = colors
            .into_iter()
            .map(|c| {
                cache
                    .insert(Mesh::cuboid(Vec3::ONE, Color::from_hex(c)))
                    .unwrap()
            })
            .collect();
        let player_mesh = meshes[8];
        let head_mesh = cache
            .insert(Mesh::sphere(0.5, 4, 6, Color::from_hex(0xc5ad8b)))
            .unwrap();
        meshes.push(head_mesh);
        let round = cache
            .insert(Mesh::sphere(1., 5, 8, Color::from_hex(0xd3b56b)))
            .unwrap();
        meshes.push(round);
        let mut objects = Vec::new();
        let mut add = |material: usize, p: [f32; 3], s: [f32; 3], solid| {
            objects.push(Object {
                mesh: meshes[material],
                position: p.into(),
                scale: s.into(),
                solid,
            })
        };
        add(0, [0., -0.25, 0.], [48., 0.5, 56.], None);
        let arena = Arena::default();
        for (i, wall) in arena.solids.iter().enumerate() {
            add(
                if i < 4 { 0 } else { 2 },
                wall.center.to_array(),
                (wall.half_extents * 2.).to_array(),
                Some(i),
            );
        }
        // Layered stone facades and blue-gray shutters give simple geometry a believable scale.
        for side in [-1., 1.] {
            for z in [-21., -12., -3., 6., 15., 24.] {
                add(1, [side * 21.95, 2.8, z], [0.18, 0.2, 7.5], None);
                add(6, [side * 21.85, 5.8, z], [0.3, 0.35, 8.5], None);
                add(4, [side * 21.84, 3.9, z], [0.18, 1.55, 1.3], None);
                add(2, [side * 21.7, 3.9, z], [0.18, 1.35, 1.1], None);
                add(1, [side * 21.7, 3.0, z], [0.5, 0.15, 1.7], None);
            }
        }
        for x in [-18., -12., -6., 0., 6., 12., 18.] {
            add(6, [x, 5.9, -25.9], [5.8, 0.4, 0.4], None);
            add(2, [x, 4.4, -25.85], [1.45, 1.4, 0.2], None);
        }
        // Timber cargo, inset doors, lintels, terracotta and a striped shade.
        for (x, z) in [
            (-16., 8.),
            (-14., 8.),
            (-16., 10.),
            (13., -5.),
            (14., -7.),
            (-5., -18.),
        ] {
            add(3, [x, 0.65, z], [1.35, 1.3, 1.35], None);
            for y in [0.16, 1.12] {
                add(1, [x, y, z + 0.69], [1.4, 0.09, 0.1], None);
            }
        }
        for x in [-12., 12.] {
            add(4, [x, 1.5, -25.8], [2.5, 3., 0.25], None);
            add(1, [x, 3.1, -25.6], [3., 0.35, 0.45], None);
        }
        for i in 0..8 {
            add(
                if i % 2 == 0 { 0 } else { 5 },
                [-18. + i as f32 * 0.55, 4.2, -8.],
                [0.55, 0.09, 5.],
                None,
            );
        }
        for z in [-10., -6.] {
            add(3, [-18., 2., z], [0.13, 4., 0.13], None);
        }
        add(9, [7., 0.012, 18.], [6., 0.025, 0.13], None);
        add(9, [10., 0.012, 15.], [0.13, 0.025, 6.], None);
        let mut physics = PhysicsWorld::<3>::new([0., -9.81, 0.]);
        physics
            .insert(BodyDesc::fixed(
                Shape::cuboid([24., 0.5, 28.]),
                [0., -0.5, 0.],
            ))
            .unwrap();
        let terrain = arena
            .solids
            .iter()
            .map(|wall| {
                physics
                    .insert(BodyDesc::fixed(
                        Shape::cuboid(wall.half_extents.to_array()),
                        wall.center.to_array(),
                    ))
                    .unwrap()
            })
            .collect();
        let mut balls = Vec::new();
        for i in 0..6 {
            let mut desc = BodyDesc::dynamic(
                Shape::ball(0.32),
                [5. + i as f32 * 0.74, 3. + i as f32 * 0.5, 15.],
                1.,
            );
            desc.restitution = 0.65;
            desc.ccd = true;
            let id = physics.insert(desc).unwrap();
            balls.push((id, round));
        }
        let mesh_data = meshes
            .iter()
            .map(|id| {
                let mesh = cache.get(*id).unwrap();
                mesh.indices
                    .iter()
                    .flat_map(|&i| {
                        let v = mesh.vertices[i as usize];
                        v.position
                            .to_array()
                            .into_iter()
                            .chain(v.normal.to_array())
                            .chain(v.color.to_array())
                    })
                    .collect()
            })
            .collect();
        let mut game = Match::new(arena);
        for id in 0..5 {
            game.spawn_player(id);
        }
        let camera = Camera::perspective(
            vec3(-12., 10., 24.),
            vec3(0., 1., 0.),
            Vec3::Y,
            1.05,
            16. / 9.,
            0.08,
            120.,
        );
        let mut app = Self {
            game,
            cache,
            objects,
            physics,
            balls,
            terrain,
            clock: FixedClock::new(64., 8),
            time: 0.,
            camera,
            items: Vec::with_capacity(256),
            instances: Vec::with_capacity(4096),
            shadow_instances: Vec::with_capacity(4096),
            matrix: [0.; 16],
            light: (Mat4::orthographic_rh_gl(-40., 40., -40., 40., 1., 140.)
                * Mat4::look_at_rh(vec3(-30., 51., 24.), Vec3::ZERO, Vec3::Y))
            .to_cols_array(),
            status: [0.; 11],
            meshes,
            mesh_data,
            sound: GameAudio::new(),
            last_reload: 0,
            audio: Vec::new(),
            replay: Replay::default(),
            player_mesh,
            head_mesh,
            kills: 0,
            recording: false,
            export: Vec::new(),
            particles: ParticlePool::new(96),
            peer: peer::Peer::default(),
            network_out: Vec::new(),
            network_in: vec![0; 1200],
        };
        app.rebuild(16. / 9., 0);
        app
    }
    pub fn update(&mut self, seconds: f64, input: InputFrame, view: u32, aspect: f32) {
        let step = self.clock.advance(seconds);
        for _ in 0..step.steps {
            self.time += step.dt;
            let command = self.peer.command(input.sanitized());
            let mut inputs = vec![(0, command)];
            self.peer.elapsed += step.dt;
            if self.peer.mode == 1 {
                if let Some(remote) = self.peer.queue.pop_latest() {
                    self.peer.last = remote;
                    self.peer.acknowledged = true;
                }
                let remote = if self.peer.elapsed < 2. {
                    self.peer.last
                } else {
                    InputFrame::default()
                };
                inputs.push((1, remote));
            }
            for bot in self
                .game
                .players
                .iter()
                .filter(|p| p.id > 0 && !(self.peer.mode == 1 && p.id == 1))
            {
                let phase = self.time * 0.3 + bot.id as f32 * 1.4;
                inputs.push((
                    bot.id,
                    InputFrame {
                        move_x: phase.sin() * 0.7,
                        move_z: 0.25,
                        yaw: phase.sin() * 2.,
                        ..Default::default()
                    },
                ));
            }
            let events = if self.peer.mode == 2 {
                let arena = &self.game.arena;
                if ferravex_fps::step_player(
                    self.game.players.iter_mut().find(|p| p.id == 0).unwrap(),
                    command,
                    arena,
                ) {
                    self.sound.cue(Cue::Fire, 0.75);
                }
                self.game.tick = self.game.tick.wrapping_add(1);
                Vec::new()
            } else {
                self.game.step(&inputs)
            };
            for event in events {
                match event {
                    GameEvent::Shot { origin, end, .. } => {
                        self.sound.at(Cue::Fire, origin, 0.8);
                        for i in 0..6 {
                            let phase = i as f32 * 2.4 + self.time;
                            let _ = self.particles.spawn(ParticleDesc {
                                position: end.to_array(),
                                velocity: [phase.sin() * 2., 1.8, phase.cos() * 2.],
                                acceleration: [0., -8., 0.],
                                lifetime: 0.4,
                                start_size: 0.07,
                                end_size: 0.01,
                                ..Default::default()
                            });
                        }
                    }
                    GameEvent::Killed { attacker: 0, .. } => self.kills += 1,
                    _ => {}
                }
            }
            self.physics.step(step.dt).unwrap();
            self.particles.update(step.dt).unwrap();
            if self.recording {
                let player = self.game.player(0).unwrap();
                let bytes: Vec<_> = player
                    .position
                    .to_array()
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                self.replay.record(Frame {
                    tick: u64::from(self.game.tick),
                    input: inputs[0].1,
                    state_hash: checksum(&bytes),
                });
            }
        }
        self.rebuild(aspect, view);
        let player = self.game.player(0).unwrap();
        if player.reload_ticks > 0 && self.last_reload == 0 {
            self.sound.cue(Cue::Reload, 0.6);
        }
        self.last_reload = player.reload_ticks;
        audio::listener(
            self.camera.position,
            vec3(input.yaw.cos(), 0., input.yaw.sin()),
        );
        self.sound.walk(
            player.position,
            player.grounded,
            view != 0,
            Surface::Stone,
            1.65,
        );
        // Native reference callers retain the PCM buffer. The browser consumes
        // the shared pull ABI, so it must not advance a second mixer here.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let count = if seconds.is_finite() {
                (seconds.clamp(0., 0.1) * 48000.) as usize
            } else {
                0
            };
            self.audio.resize(count, [0.; 2]);
            for block in self.audio.chunks_mut(audio::MAX_PULL_FRAMES) {
                audio::render(block, audio::SAMPLE_RATE);
            }
        }
    }
    fn rebuild(&mut self, aspect: f32, view: u32) {
        let aspect = if aspect.is_finite() {
            aspect.clamp(0.2, 5.)
        } else {
            16. / 9.
        };
        let player = self.game.player(0).unwrap();
        self.camera = match view {
            1 => {
                let eye = player.eye_position();
                Camera::perspective(
                    eye,
                    eye + forward(player.yaw, player.pitch),
                    Vec3::Y,
                    1.15,
                    aspect,
                    0.06,
                    120.,
                )
            }
            2 => Camera {
                view_projection: Mat4::orthographic_rh(
                    -31. * aspect,
                    31. * aspect,
                    -31.,
                    31.,
                    0.1,
                    100.,
                ) * Mat4::look_at_rh(vec3(0., 60., 0.), Vec3::ZERO, -Vec3::Z),
                position: vec3(0., 60., 0.),
                clip_depth: ClipDepth::ZeroToOne,
            },
            _ => {
                let angle = self.time * 0.025;
                let eye = vec3(-13. + angle.sin() * 8., 11., 24.);
                Camera::perspective(eye, vec3(0., 1., 0.), Vec3::Y, 1., aspect, 0.08, 120.)
            }
        };
        self.items.clear();
        for o in &self.objects {
            self.items.push(RenderItem::new(
                o.mesh,
                Mat4::from_scale_rotation_translation(
                    o.scale,
                    ferravex_graphics::Quat::IDENTITY,
                    o.position,
                ),
            ));
        }
        for (handle, mesh) in &self.balls {
            let p = self.physics.body(*handle).unwrap().position();
            self.items.push(RenderItem::new(
                *mesh,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.32),
                    ferravex_graphics::Quat::IDENTITY,
                    p.into(),
                ),
            ));
        }
        for p in self
            .game
            .players
            .iter()
            .filter(|p| p.is_alive() && (view != 1 || p.id != 0))
        {
            let rot = ferravex_graphics::Quat::from_rotation_y(-p.yaw);
            let sway = (self.time * 8. + p.id as f32).sin() * 0.06;
            for (offset, size, mesh) in [
                (vec3(0., 1., 0.), vec3(0.56, 0.72, 0.32), self.player_mesh),
                (vec3(0., 1.65, 0.), Vec3::splat(0.35), self.head_mesh),
                (
                    vec3(-0.17, 0.34 + sway, 0.),
                    vec3(0.19, 0.68, 0.22),
                    self.meshes[4],
                ),
                (
                    vec3(0.17, 0.34 - sway, 0.),
                    vec3(0.19, 0.68, 0.22),
                    self.meshes[4],
                ),
                (
                    vec3(0.34, 1.12, -0.2),
                    vec3(0.12, 0.16, 0.65),
                    self.meshes[4],
                ),
            ] {
                self.items.push(RenderItem::new(
                    mesh,
                    Mat4::from_scale_rotation_translation(size, rot, p.position + rot * offset),
                ));
            }
        }
        for p in self.particles.iter() {
            self.items.push(RenderItem::new(
                self.meshes[9],
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(p.size),
                    ferravex_graphics::Quat::IDENTITY,
                    p.position.into(),
                ),
            ));
        }
        if view == 1 {
            let rot = ferravex_graphics::Quat::from_rotation_y(-player.yaw)
                * ferravex_graphics::Quat::from_rotation_x(player.pitch);
            let bob = (self.time * 10.).sin() * player.velocity.length().min(7.) * 0.0008;
            let eye = player.eye_position();
            for (offset, size, material) in [
                (vec3(0.22, -0.22 + bob, -0.43), vec3(0.12, 0.15, 0.35), 4),
                (vec3(0.22, -0.18 + bob, -0.68), vec3(0.045, 0.045, 0.32), 4),
                (vec3(0.22, -0.32 + bob, -0.37), vec3(0.08, 0.16, 0.09), 3),
            ] {
                self.items.push(RenderItem::new(
                    self.meshes[material],
                    Mat4::from_scale_rotation_translation(size, rot, eye + rot * offset),
                ));
            }
        }
        self.shadow_instances.clear();
        for item in &self.items {
            self.shadow_instances.push(item.mesh.index as f32);
            self.shadow_instances.extend(item.transform.to_cols_array());
        }
        self.instances.clear();
        let frustum = self.camera.frustum().unwrap();
        for item in &self.items {
            if frustum.intersects_bounds(self.cache.bounds(item.mesh).unwrap(), item.transform) {
                self.instances.push(item.mesh.index as f32);
                self.instances.extend(item.transform.to_cols_array());
            }
        }
        self.matrix = self.camera.view_projection.to_cols_array();
        self.status = [
            player.health as f32,
            player.ammo as f32,
            self.kills as f32,
            self.game.tick as f32,
            (self.instances.len() / 17) as f32,
            self.items.len() as f32,
            player.reload_ticks as f32,
            self.time,
            self.camera.position.x,
            self.camera.position.y,
            self.camera.position.z,
        ];
    }
    pub fn render(&self, width: u32, height: u32) -> SoftwareRenderer {
        let mut renderer = SoftwareRenderer::new(width, height).unwrap();
        self.render_into(&mut renderer);
        renderer
    }
    /// Reuse framebuffer/depth/scratch storage across frames.
    pub fn render_into(&self, renderer: &mut SoftwareRenderer) {
        renderer.settings.look = ferravex_engine::render::look();
        renderer.settings.clear_color = Color::from_hex(0xa9b8ba);
        renderer.settings.ambient = 0.5;
        renderer.settings.diffuse = 0.5;
        renderer.settings.fog = Some(ferravex_graphics::Fog {
            color: Color::from_hex(0xa9b8ba),
            start: 40.,
            end: 120.,
        });
        renderer
            .render(&self.cache, &self.items, self.camera)
            .unwrap();
    }
    pub fn edit(&mut self, index: usize, x: f32, y: f32, z: f32) -> bool {
        if index == 0
            || index >= self.objects.len()
            || ![x, y, z].iter().all(|v| v.is_finite() && v.abs() < 100.)
        {
            return false;
        }
        let o = &mut self.objects[index];
        o.position = vec3(x, y, z);
        if let Some(i) = o.solid {
            self.game.arena.solids[i].center = o.position;
            self.physics
                .body_mut(self.terrain[i])
                .unwrap()
                .set_position(o.position.to_array())
                .unwrap();
        }
        true
    }
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
    pub fn object(&self, index: usize) -> Option<[f32; 6]> {
        self.objects.get(index).map(|o| {
            [
                o.position.x,
                o.position.y,
                o.position.z,
                o.scale.x,
                o.scale.y,
                o.scale.z,
            ]
        })
    }
    pub fn replay_bytes(&self) -> Vec<u8> {
        self.replay.encode()
    }
}
static LAB: Mutex<Option<Lab>> = Mutex::new(None);
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_button_mask(kind: u32) -> u32 {
    match kind {
        0 => u32::from(ferravex_fps::BUTTON_FIRE),
        1 => u32::from(ferravex_fps::BUTTON_JUMP),
        2 => u32::from(ferravex_fps::BUTTON_RELOAD),
        _ => 0,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_init() {
    *LAB.lock().unwrap() = Some(Lab::new());
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_step(
    dt: f64,
    x: f32,
    z: f32,
    yaw: f32,
    pitch: f32,
    buttons: u32,
    view: u32,
    aspect: f32,
) {
    if let Some(lab) = LAB.lock().unwrap().as_mut() {
        lab.update(
            dt,
            InputFrame {
                move_x: x,
                move_z: z,
                yaw,
                pitch,
                buttons: buttons as u8,
                ..Default::default()
            },
            view,
            aspect,
        );
    }
}
macro_rules! buffer {
    ($ptr:ident,$len:ident,$field:ident,$ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $ptr() -> *const $ty {
            LAB.lock().unwrap().as_ref().unwrap().$field.as_ptr()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn $len() -> usize {
            LAB.lock().unwrap().as_ref().unwrap().$field.len()
        }
    };
}
buffer!(
    ferravex_shadow_instances,
    ferravex_shadow_instances_len,
    shadow_instances,
    f32
);
buffer!(ferravex_instances, ferravex_instances_len, instances, f32);
buffer!(ferravex_matrix, ferravex_matrix_len, matrix, f32);
buffer!(ferravex_light, ferravex_light_len, light, f32);
buffer!(ferravex_status, ferravex_status_len, status, f32);
buffer!(ferravex_audio, ferravex_audio_len, audio, [f32; 2]);
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mesh_count() -> usize {
    LAB.lock().unwrap().as_ref().unwrap().mesh_data.len()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mesh(index: usize) -> *const f32 {
    LAB.lock().unwrap().as_ref().unwrap().mesh_data[index].as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mesh_len(index: usize) -> usize {
    LAB.lock().unwrap().as_ref().unwrap().mesh_data[index].len()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_object_count() -> usize {
    LAB.lock().unwrap().as_ref().unwrap().object_count()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_object(index: usize, axis: usize) -> f32 {
    LAB.lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .object(index)
        .and_then(|v| v.get(axis).copied())
        .unwrap_or(0.)
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_edit(index: usize, x: f32, y: f32, z: f32) -> u32 {
    u32::from(LAB.lock().unwrap().as_mut().unwrap().edit(index, x, y, z))
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_record(enabled: u32) {
    LAB.lock().unwrap().as_mut().unwrap().recording = enabled != 0;
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_export_replay() -> usize {
    let mut lock = LAB.lock().unwrap();
    let lab = lock.as_mut().unwrap();
    lab.export = lab.replay_bytes();
    lab.export.len()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_replay_bytes() -> *const u8 {
    LAB.lock().unwrap().as_ref().unwrap().export.as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_mode(mode: u32) {
    LAB.lock().unwrap().as_mut().unwrap().peer.set_mode(mode);
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_out() -> *const u8 {
    let mut lock = LAB.lock().unwrap();
    let lab = lock.as_mut().unwrap();
    lab.network_out = lab.peer.packet(&lab.game);
    lab.network_out.as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_out_len() -> usize {
    LAB.lock().unwrap().as_ref().unwrap().network_out.len()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_in() -> *mut u8 {
    LAB.lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .network_in
        .as_mut_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_receive(len: usize) -> u32 {
    let mut lock = LAB.lock().unwrap();
    let lab = lock.as_mut().unwrap();
    if len > 1200 {
        return 0;
    }
    u32::from(lab.peer.receive(&lab.network_in[..len], &mut lab.game))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_runs_with_owned_physics_and_renders_nonempty() {
        let mut lab = Lab::new();
        for _ in 0..640 {
            lab.update(1. / 64., InputFrame::default(), 0, 16. / 9.);
        }
        assert_eq!(lab.game.tick, 640);
        assert_eq!(lab.status[0], 100.);
        let renderer = lab.render(160, 90);
        assert!(
            renderer
                .framebuffer
                .depths()
                .iter()
                .filter(|d| d.is_finite())
                .count()
                > 2000
        );
        assert!(!lab.edit(usize::MAX, 0., 0., 0.));
    }
    #[test]
    fn movement_recording_and_editor_apply_real_state() {
        let mut lab = Lab::new();
        lab.recording = true;
        let start = lab.game.player(0).unwrap().position;
        for _ in 0..64 {
            lab.update(
                1. / 64.,
                InputFrame {
                    move_z: 1.,
                    ..Default::default()
                },
                1,
                1.,
            );
        }
        assert!((lab.game.player(0).unwrap().position - start).length() > 5.);
        assert_eq!(
            Replay::decode(&lab.replay_bytes()).unwrap().frames().len(),
            64
        );
        let wall = lab.objects.iter().position(|o| o.solid == Some(0)).unwrap();
        assert!(lab.edit(wall, -20., 3., 0.));
        assert_eq!(lab.game.arena.solids[0].center.x, -20.);
    }
}
