//! Playable first-party reference scene. Platform code consumes the same Rust state on native/Web.
#[cfg(any(target_arch = "wasm32", test))]
mod hud;
#[cfg(any(target_arch = "wasm32", test))]
mod interactive;
#[cfg(test)]
mod journey200_tests;
mod orientation;
mod peer;
mod scene_edit;
mod training;
use ferravex_core::session::{GameSession, SessionState};
use ferravex_engine::play::{
    feedback::{FeedbackController, FeedbackEvent, FeedbackKind},
    objective::{ObjectiveGraph, ObjectiveView},
    particles::{ParticleDesc, ParticlePool},
    scene::{NodeId, Scene, Transform},
};
use ferravex_engine::render::audio::{self, Cue, GameAudio, Surface};
use ferravex_engine::render::preferences;
use ferravex_engine::{
    dynamics::{BodyDesc, BodyHandle, PhysicsWorld, Shape},
    replay::{Frame, Replay, checksum},
};
use ferravex_fps::{Arena, GameEvent, InputFrame, Match, forward};
use ferravex_graphics::{
    Camera, ClipDepth, Color, Mat4, Mesh, MeshCache, MeshId, RenderItem, SoftwareRenderer, Vec3,
    vec3,
};
pub use scene_edit::SceneEditError;
use std::cell::RefCell;
#[derive(Clone)]
struct Object {
    mesh: MeshId,
    node: NodeId,
    solid: Option<usize>,
}
/// Offline training milestones, measured from accepted simulation actions.
/// A reload counts only when its timer completes; walking into a wall adds no distance.
///
/// ```
/// let lab = owned_lab::Lab::new_interactive();
/// assert_eq!(lab.training_progress().shots, 0);
/// assert_eq!(lab.training_progress().distance, 0.0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrainingProgress {
    /// Horizontal distance traveled while alive, capped at the five-meter objective.
    pub distance: f32,
    /// Accepted shots, excluding dry fire and fire during reload.
    pub shots: u32,
    /// Confirmed hits on moving operators.
    pub hits: u32,
    /// Completed reload cycles.
    pub reloads: u32,
}
pub struct Lab {
    pub game: Match,
    pub cache: MeshCache,
    objects: Vec<Object>,
    scene: Scene,
    physics: PhysicsWorld<3>,
    balls: Vec<(BodyHandle, MeshId)>,
    terrain: Vec<BodyHandle>,
    session: GameSession,
    training: TrainingProgress,
    objectives: ObjectiveGraph<4>,
    feedback: FeedbackController<64>,
    feedback_serial: u64,
    target_selector: ferravex_engine::play::ai::TargetSelector,
    target_hint: Option<ferravex_engine::play::ai::Candidate>,
    hit_marker: f32,
    drill_notice: f32,
    orbit_angle: f32,
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
        let mut scene = Scene::new();
        let root = scene
            .create("courtyard", Transform::default(), None)
            .unwrap();
        let mut add = |material: usize, p: [f32; 3], s: [f32; 3], solid| {
            objects.push(Object {
                mesh: meshes[material],
                node: scene
                    .create(
                        format!("object-{}", objects.len()),
                        Transform {
                            translation: p,
                            scale: s,
                            ..Default::default()
                        },
                        Some(root),
                    )
                    .expect("bounded authored courtyard geometry"),
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
            scene,
            physics,
            balls,
            terrain,
            session: GameSession::new(ferravex_fps::TICK_RATE as f64, 8, true),
            training: TrainingProgress::default(),
            objectives: training::objectives(),
            feedback: FeedbackController::new(Default::default(), 144).unwrap(),
            feedback_serial: 0,
            target_selector: ferravex_engine::play::ai::TargetSelector::new(2.).unwrap(),
            target_hint: None,
            hit_marker: 0.,
            drill_notice: 0.,
            orbit_angle: 0.,
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
        preferences::sync_feedback(&mut app.feedback);
        app.rebuild(16. / 9., 0);
        app
    }
    /// Construct a training host that waits for input capture before advancing.
    /// [`Self::new`] still autoplays for existing software/headless callers.
    ///
    /// ```
    /// use ferravex_core::session::SessionState;
    /// use ferravex_fps::InputFrame;
    /// let mut lab = owned_lab::Lab::new_interactive();
    /// assert_eq!(lab.session_state(), SessionState::Ready);
    /// lab.update_interactive(1.0 / 64.0, InputFrame::default(), true, 1, 16.0 / 9.0);
    /// assert_eq!(lab.game.tick, 1);
    /// lab.update_interactive(10.0, InputFrame::default(), false, 1, 16.0 / 9.0);
    /// assert_eq!(lab.session_state(), SessionState::Paused);
    /// assert_eq!(lab.game.tick, 1);
    /// assert!(lab.reset_interactive());
    /// assert_eq!(lab.session_state(), SessionState::Ready);
    /// ```
    pub fn new_interactive() -> Self {
        let mut lab = Self::new();
        lab.session.reset();
        lab.sound.pause();
        lab
    }
    /// Simulation lifecycle; a connected peer remains Playing with local controls released.
    /// See [`Self::new_interactive`] for a pause/resume example.
    pub fn session_state(&self) -> SessionState {
        self.session.state()
    }
    /// Transport role and freshness, derived from accepted packets and fixed ticks.
    /// A local menu does not stop the peer freshness clock.
    ///
    /// ```
    /// let lab = owned_lab::Lab::new_interactive();
    /// assert_eq!(lab.peer_status(), "Offline training");
    /// ```
    pub fn peer_status(&self) -> &'static str {
        self.peer.status_label()
    }
    /// Offline progress for the current scene. Peer prediction/snapshots never award milestones.
    /// See [`TrainingProgress`] for an example.
    pub fn training_progress(&self) -> TrainingProgress {
        self.training
    }
    /// Reset scene edits, physics, replay, score and training to Ready. Refuse a live peer reset.
    /// See [`Self::new_interactive`] for an example; existing headless callers can use `Lab::new()`.
    pub fn reset_interactive(&mut self) -> bool {
        if self.peer.mode != 0 {
            return false;
        }
        self.sound.reset();
        *self = Self::new_interactive();
        true
    }
    /// Advance the interactive host. Inactive offline frames pause without accumulating time.
    /// With a peer, continue authority/prediction at 64 Hz and send neutral local commands,
    /// preserving aim. The match tick and network sequencing remain simulation-owned.
    /// See [`Self::new_interactive`] for a complete usage example.
    pub fn update_interactive(
        &mut self,
        seconds: f64,
        input: InputFrame,
        active: bool,
        view: u32,
        aspect: f32,
    ) {
        self.reconcile_interactive(active);
        let input = if active { input } else { self.neutral_input() };
        self.update(seconds, input, view, aspect);
    }
    fn neutral_input(&self) -> InputFrame {
        let player = self.game.player(0).unwrap();
        InputFrame {
            yaw: player.yaw,
            pitch: player.pitch,
            ..Default::default()
        }
    }
    fn reconcile_interactive(&mut self, active: bool) {
        if active || self.peer.mode != 0 {
            self.session.start();
            self.session.set_paused(false);
        } else {
            self.session.set_paused(true);
        }
    }
    pub fn update(&mut self, seconds: f64, input: InputFrame, view: u32, aspect: f32) {
        self.update_sampled(seconds, view, aspect, || input);
    }
    // The interactive adapter consumes edges here, only when a fixed tick exists.
    fn update_sampled(
        &mut self,
        seconds: f64,
        view: u32,
        aspect: f32,
        mut input: impl FnMut() -> InputFrame,
    ) {
        preferences::sync_feedback(&mut self.feedback);
        let presentation_dt = if seconds.is_finite() {
            seconds.clamp(0., 0.25) as f32
        } else {
            0.
        };
        let policy = self.feedback.policy();
        if policy.reduced_motion() {
            self.particles.clear();
        }
        if policy.reduced_flash() {
            self.hit_marker = 0.;
        }
        self.hit_marker = (self.hit_marker - presentation_dt).max(0.);
        self.drill_notice = (self.drill_notice - presentation_dt).max(0.);
        self.particles.update(presentation_dt).unwrap();
        if self.session.is_playing() {
            self.sound.resume();
        } else {
            self.sound.pause();
        }
        let step = self.session.advance(seconds);
        for _ in 0..step.steps {
            let progress_before = self.training;
            let before = self.game.player(0).unwrap();
            let previous_position = before.position;
            let was_alive = before.is_alive();
            let previous_reload = before.reload_ticks;
            self.time += step.dt;
            let command = self.peer.command(input().sanitized());
            let mut inputs = vec![(0, command)];
            self.peer.elapsed = (self.peer.elapsed + step.dt).min(60.);
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
                    let origin = self.game.player(0).unwrap().eye_position();
                    self.accept_feedback(FeedbackKind::Fire, origin);
                }
                self.game.tick = self.game.tick.wrapping_add(1);
                Vec::new()
            } else {
                self.game.step(&inputs)
            };
            for event in events {
                match event {
                    GameEvent::Shot {
                        player,
                        origin,
                        end,
                    } => {
                        if self.peer.mode == 0 && player == 0 {
                            self.training.shots = self.training.shots.saturating_add(1);
                        }
                        let particles = self.accept_feedback(FeedbackKind::Fire, origin);
                        for i in 0..particles.saturating_mul(3).min(6) {
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
                    GameEvent::Hit {
                        attacker: 0,
                        victim,
                        ..
                    } => {
                        if self.peer.mode == 0 {
                            self.training.hits = self.training.hits.saturating_add(1);
                        }
                        let position = self.game.player(victim).unwrap().position;
                        self.accept_feedback(FeedbackKind::Hit, position);
                        if !self.feedback.policy().reduced_flash() {
                            self.hit_marker = 0.18;
                        }
                    }
                    GameEvent::Killed { attacker: 0, .. } => self.kills += 1,
                    _ => {}
                }
            }
            if self.peer.mode == 0 {
                let player = self.game.player(0).unwrap();
                if was_alive && player.is_alive() {
                    let delta = player.position - previous_position;
                    // Respawns and externally corrected/teleported poses are not walking.
                    let distance = vec3(delta.x, 0., delta.z).length();
                    if distance.is_finite() && distance < 1. {
                        self.training.distance = (self.training.distance + distance).min(5.);
                    }
                    if previous_reload == 1 && player.reload_ticks == 0 {
                        self.training.reloads = self.training.reloads.saturating_add(1);
                    }
                }
            }
            self.sync_training(progress_before);
            self.update_orientation();
            self.physics.step(step.dt).unwrap();
            if !policy.reduced_motion() {
                self.orbit_angle += step.dt * 0.025;
            }
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
        // Sampling releases bounded feedback burst capacity; never controls simulation time.
        let _ = self.feedback.advance(presentation_dt).unwrap();
        self.rebuild(aspect, view);
        let player = self.game.player(0).unwrap();
        if player.reload_ticks > 0 && self.last_reload == 0 {
            self.sound.cue(Cue::Reload, 0.6);
        }
        self.last_reload = player.reload_ticks;
        audio::listener(
            self.camera.position,
            vec3(player.yaw.cos(), 0., player.yaw.sin()),
        );
        self.sound.walk(
            player.position,
            player.grounded,
            view != 0 && self.session.is_playing(),
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
                let angle = self.orbit_angle;
                let eye = vec3(-13. + angle.sin() * 8., 11., 24.);
                Camera::perspective(eye, vec3(0., 1., 0.), Vec3::Y, 1., aspect, 0.08, 120.)
            }
        };
        self.items.clear();
        for o in &self.objects {
            self.items.push(RenderItem::new(
                o.mesh,
                Mat4::from_cols_array(&self.scene.world_matrix(o.node).unwrap()),
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
            let sway = if self.feedback.policy().reduced_motion() {
                0.
            } else {
                (self.time * 8. + p.id as f32).sin() * 0.06
            };
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
            let bob = if self.feedback.policy().reduced_motion() {
                0.
            } else {
                (self.time * 10.).sin() * player.velocity.length().min(7.) * 0.0008
            };
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
    pub fn replay_bytes(&self) -> Vec<u8> {
        self.replay.encode()
    }
}
// The renderer, input and GameAudio runtime all belong to the calling thread.
// Returned ABI pointers borrow this thread's Lab storage until its next mutation,
// initialization/reset or thread exit; callers must not transfer them across threads.
thread_local! {
    static LAB: RefCell<Option<Lab>> = const { RefCell::new(None) };
}
fn with_lab<T>(read: impl FnOnce(&Lab) -> T) -> T {
    LAB.with(|cell| read(cell.borrow().as_ref().expect("initialize Courtyard first")))
}
fn with_lab_mut<T>(write: impl FnOnce(&mut Lab) -> T) -> T {
    LAB.with(|cell| {
        write(
            cell.borrow_mut()
                .as_mut()
                .expect("initialize Courtyard first"),
        )
    })
}
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
    LAB.with(|cell| *cell.borrow_mut() = Some(Lab::new()));
    #[cfg(any(target_arch = "wasm32", test))]
    interactive::reset_controls();
}
/// Browser training initialization; preserves the autoplay behavior of `ferravex_init`.
///
/// ```
/// owned_lab::ferravex_training_init();
/// assert_eq!(owned_lab::ferravex_training_state(), 0); // Ready
/// assert_eq!(owned_lab::ferravex_training_progress(), 0);
/// assert_eq!(owned_lab::ferravex_training_reset(), 1);
/// ```
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_training_init() {
    LAB.with(|cell| *cell.borrow_mut() = Some(Lab::new_interactive()));
    #[cfg(any(target_arch = "wasm32", test))]
    interactive::reset_controls();
}
/// Reset offline training, returning 1 on success and 0 while connected/uninitialized.
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_training_reset() -> u32 {
    let reset = LAB.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .is_some_and(Lab::reset_interactive)
    });
    #[cfg(any(target_arch = "wasm32", test))]
    if reset {
        interactive::reset_controls();
    }
    u32::from(reset)
}
/// Host state: 0 Ready, 1 Playing, 2 Paused, 3 Won, 4 Lost. No network payload changes.
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_training_state() -> u32 {
    match LAB.with(|cell| cell.borrow().as_ref().map(Lab::session_state)) {
        None | Some(SessionState::Ready) => 0,
        Some(SessionState::Playing) => 1,
        Some(SessionState::Paused) => 2,
        Some(SessionState::Won) => 3,
        Some(SessionState::Lost) => 4,
    }
}
/// Completed offline milestones: bit 0 move five meters, 1 shoot, 2 reload, 3 hit.
/// This is an additive host query; the existing status buffer keeps its layout.
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_training_progress() -> u32 {
    LAB.with(|cell| cell.borrow().as_ref().map_or(0, Lab::training_mask))
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
    LAB.with(|cell| {
        if let Some(lab) = cell.borrow_mut().as_mut() {
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
    });
}
macro_rules! buffer {
    ($ptr:ident,$len:ident,$field:ident,$ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $ptr() -> *const $ty {
            with_lab(|lab| lab.$field.as_ptr())
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn $len() -> usize {
            with_lab(|lab| lab.$field.len())
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
    with_lab(|lab| lab.mesh_data.len())
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mesh(index: usize) -> *const f32 {
    with_lab(|lab| lab.mesh_data[index].as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mesh_len(index: usize) -> usize {
    with_lab(|lab| lab.mesh_data[index].len())
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_object_count() -> usize {
    with_lab(|lab| lab.object_count())
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_object(index: usize, axis: usize) -> f32 {
    with_lab(|lab| {
        lab.object(index)
            .and_then(|v| v.get(axis).copied())
            .unwrap_or(0.)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_edit(index: usize, x: f32, y: f32, z: f32) -> u32 {
    u32::from(with_lab_mut(|lab| lab.edit(index, x, y, z)))
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_record(enabled: u32) {
    with_lab_mut(|lab| lab.recording = enabled != 0);
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_export_replay() -> usize {
    with_lab_mut(|lab| {
        lab.export = lab.replay_bytes();
        lab.export.len()
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_replay_bytes() -> *const u8 {
    with_lab(|lab| lab.export.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_mode(mode: u32) {
    with_lab_mut(|lab| lab.peer.set_mode(mode));
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_out() -> *const u8 {
    with_lab_mut(|lab| {
        lab.network_out = lab.peer.packet(&lab.game);
        lab.network_out.as_ptr()
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_out_len() -> usize {
    with_lab(|lab| lab.network_out.len())
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_in() -> *mut u8 {
    with_lab_mut(|lab| lab.network_in.as_mut_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_peer_receive(len: usize) -> u32 {
    with_lab_mut(|lab| {
        if len > lab.network_in.len() {
            return 0;
        }
        u32::from(lab.peer.receive(&lab.network_in[..len], &mut lab.game))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi_state_and_reset_are_isolated_per_calling_thread() {
        ferravex_init();
        ferravex_record(1);
        ferravex_step(1. / 64., 0., 0., 0., 0., 1, 1, 1.);
        let pointer = ferravex_matrix();
        let mesh = ferravex_mesh(0);
        let replay = with_lab(Lab::replay_bytes);
        assert_eq!(with_lab(|lab| lab.game.tick), 1);
        std::thread::spawn(|| {
            assert_eq!(ferravex_training_reset(), 0, "new thread has no lab");
            ferravex_training_init();
            interactive::ferravex_controls_step(1. / 64., 1, 2, 1., 320, 480);
            assert_eq!(with_lab(|lab| lab.game.tick), 1);
            assert_eq!(ferravex_training_reset(), 1);
            assert_eq!(with_lab(|lab| lab.game.tick), 0);
            assert_eq!(ferravex_training_state(), 0);
        })
        .join()
        .unwrap();
        assert_eq!(
            ferravex_matrix(),
            pointer,
            "read borrows leave inline ABI storage in place"
        );
        assert_eq!(ferravex_mesh(0), mesh);
        assert_eq!(with_lab(|lab| lab.game.tick), 1);
        assert_eq!(with_lab(Lab::replay_bytes), replay);
        assert_eq!(ferravex_training_state(), 1);
        assert_eq!(ferravex_training_reset(), 1);
        assert_eq!(ferravex_training_progress(), 0);
        assert_eq!(with_lab(|lab| lab.game.tick), 0);
    }

    #[test]
    fn preferences_apply_before_events_and_on_zero_tick_menu_frames() {
        use ferravex_engine::play::feedback::FeedbackPolicy;
        preferences::set_feedback_policy(FeedbackPolicy::default());
        let mut lab = Lab::new_interactive();
        lab.recording = true;
        let fire = InputFrame {
            buttons: ferravex_fps::BUTTON_FIRE,
            ..Default::default()
        };
        lab.update_interactive(1. / 64., fire, true, 1, 1.);
        assert_eq!(lab.particles.len(), 6);
        lab.hit_marker = 0.18;
        let replay = lab.replay_bytes();
        let progress = lab.training_progress();
        preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 0.5).unwrap());
        lab.update_interactive(0., InputFrame::default(), false, 1, 1.);
        assert_eq!(
            lab.particles.len(),
            0,
            "motion removes already-emitted sparks"
        );
        assert_eq!(
            lab.hit_marker, 0.18,
            "motion does not suppress flash independently"
        );
        assert!(lab.feedback.policy().reduced_motion());
        assert_eq!(lab.feedback.policy().intensity(), 0.5);
        assert!(lab.sound.is_paused());
        preferences::set_feedback_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        lab.update_interactive(0., InputFrame::default(), false, 1, 1.);
        assert_eq!(lab.hit_marker, 0.);
        assert_eq!(lab.particles.len(), 0, "re-enabling never revives sparks");
        assert_eq!(lab.replay_bytes(), replay);
        assert_eq!(lab.training_progress(), progress);
        lab.game.player_mut(0).unwrap().fire_cooldown = 0;
        lab.update_interactive(1. / 64., fire, true, 1, 1.);
        assert_eq!(
            lab.particles.len(),
            6,
            "flash reduction still allows motion"
        );
        assert_eq!(lab.training_progress().shots, 2);
        preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
        lab.game.player_mut(0).unwrap().fire_cooldown = 0;
        let orbit = lab.orbit_angle;
        lab.update_interactive(1. / 64., fire, true, 0, 1.);
        assert_eq!(
            lab.particles.len(),
            0,
            "new events observe this frame's policy"
        );
        assert_eq!(lab.orbit_angle, orbit);
        assert_eq!(lab.training_progress().shots, 3);
        assert!(lab.reset_interactive());
        assert!(
            lab.feedback.policy().reduced_motion(),
            "reset retains shared preferences"
        );
        assert_eq!(lab.hit_marker, 0.);
        preferences::set_feedback_policy(FeedbackPolicy::default());
    }

    #[test]
    fn tick_input_sampler_is_not_called_for_ready_paused_or_zero_tick_frames() {
        let mut lab = Lab::new_interactive();
        lab.update_sampled(1., 1, 1., || panic!("ready consumes no gameplay edges"));
        lab.reconcile_interactive(true);
        lab.update_sampled(1. / 128., 1, 1., || {
            panic!("fractional frame consumes no edges")
        });
        let mut calls = 0;
        lab.update_sampled(15. / 128., 1, 1., || {
            calls += 1;
            InputFrame {
                buttons: if calls == 1 {
                    ferravex_fps::BUTTON_FIRE
                } else {
                    0
                },
                ..Default::default()
            }
        });
        assert_eq!(calls, 8);
        assert_eq!(
            lab.training_progress().shots,
            1,
            "tap is not repeated after cooldown in a catch-up batch"
        );
        lab.reconcile_interactive(false);
        lab.update_sampled(120., 1, 1., || panic!("paused consumes no gameplay edges"));
    }
    #[test]
    fn reduced_presentation_preserves_simulation_replay_and_removes_weapon_bob() {
        use ferravex_engine::play::feedback::FeedbackPolicy;
        let mut standard = Lab::new();
        let mut reduced = Lab::new();
        standard.recording = true;
        reduced.recording = true;
        for tick in 0..32 {
            let input = InputFrame {
                move_x: 1.,
                buttons: if tick == 0 {
                    ferravex_fps::BUTTON_FIRE
                } else {
                    0
                },
                ..Default::default()
            };
            preferences::set_feedback_policy(FeedbackPolicy::default());
            standard.update(1. / 64., input, 1, 1.);
            preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
            reduced.update(1. / 64., input, 1, 1.);
        }
        assert_eq!(standard.game.tick, reduced.game.tick);
        assert_eq!(
            standard.game.player(0).unwrap().snapshot(),
            reduced.game.player(0).unwrap().snapshot()
        );
        assert_eq!(standard.training_progress(), reduced.training_progress());
        assert_eq!(standard.replay_bytes(), reduced.replay_bytes());
        let gun_y = |lab: &Lab| {
            lab.items[lab.items.len() - 3]
                .transform
                .transform_point3(Vec3::ZERO)
                .y
                - lab.game.player(0).unwrap().eye_position().y
        };
        assert!((gun_y(&reduced) + 0.22).abs() < 0.000001);
        assert!((gun_y(&standard) + 0.22).abs() > 0.001);
        preferences::set_feedback_policy(FeedbackPolicy::default());
    }
    #[test]
    fn interactive_pause_freezes_world_replay_and_training_without_catch_up() {
        let mut lab = Lab::new_interactive();
        lab.recording = true;
        let active_input = InputFrame {
            move_x: 1.,
            buttons: ferravex_fps::BUTTON_FIRE,
            ..Default::default()
        };
        lab.update_interactive(20., active_input, false, 1, 1.);
        assert_eq!(lab.session_state(), SessionState::Ready);
        assert_eq!(lab.game.tick, 0);
        lab.update_interactive(1. / 64., active_input, true, 1, 1.);
        let player = lab.game.player(0).unwrap().snapshot();
        let ball = lab.physics.body(lab.balls[0].0).unwrap().position();
        let progress = lab.training_progress();
        let replay = lab.replay_bytes();
        let time = lab.time;
        lab.update_interactive(120., active_input, false, 2, 1.);
        assert_eq!(lab.session_state(), SessionState::Paused);
        assert_eq!(lab.game.tick, 1);
        assert_eq!(lab.game.player(0).unwrap().snapshot(), player);
        assert_eq!(lab.physics.body(lab.balls[0].0).unwrap().position(), ball);
        assert_eq!(lab.training_progress(), progress);
        assert_eq!(lab.replay_bytes(), replay);
        assert_eq!(lab.time, time);
        lab.update_interactive(1. / 128., InputFrame::default(), true, 1, 1.);
        assert_eq!(lab.game.tick, 1, "paused wall time was discarded");
        lab.update_interactive(1. / 128., InputFrame::default(), true, 1, 1.);
        assert_eq!(lab.game.tick, 2);
        assert_eq!(lab.game.player(0).unwrap().ammo, player.ammo);
    }
    #[test]
    fn interactive_reset_clears_scene_replay_progress_and_fractional_time() {
        let mut lab = Lab::new_interactive();
        let original = lab.object(1).unwrap();
        lab.recording = true;
        lab.update_interactive(
            3. / 128.,
            InputFrame {
                buttons: ferravex_fps::BUTTON_FIRE,
                ..Default::default()
            },
            true,
            1,
            1.,
        );
        assert_eq!(lab.training_progress().shots, 1);
        assert!(lab.edit(1, -20., 2., 1.));
        lab.kills = 3;
        assert!(lab.reset_interactive());
        assert_eq!(lab.session_state(), SessionState::Ready);
        assert_eq!(lab.game.tick, 0);
        assert_eq!(lab.kills, 0);
        assert_eq!(lab.time, 0.);
        assert_eq!(lab.object(1).unwrap(), original);
        assert_eq!(
            lab.game.arena.solids[0].center,
            Vec3::from_slice(&original[..3])
        );
        assert_eq!(lab.training_progress(), TrainingProgress::default());
        assert_eq!(lab.training_mask(), 0);
        assert!(lab.replay.frames().is_empty());
        assert!(!lab.recording);
        assert_eq!(lab.game.player(0).unwrap().ammo, 30);
        lab.update_interactive(1. / 128., InputFrame::default(), true, 1, 1.);
        assert_eq!(lab.game.tick, 0, "reset discarded the old fractional tick");
    }
    #[test]
    fn local_menu_never_pauses_peer_authority_or_prediction() {
        for mode in [1, 2] {
            let mut lab = Lab::new_interactive();
            lab.peer.set_mode(mode);
            lab.game.player_mut(0).unwrap().yaw = 0.8;
            lab.game.player_mut(0).unwrap().pitch = -0.2;
            lab.peer.last = InputFrame {
                move_x: 1.,
                buttons: ferravex_fps::BUTTON_FIRE,
                ..Default::default()
            };
            lab.update_interactive(
                1. / 64.,
                InputFrame {
                    move_z: 1.,
                    buttons: ferravex_fps::BUTTON_FIRE,
                    ..Default::default()
                },
                false,
                1,
                1.,
            );
            assert_eq!(lab.session_state(), SessionState::Playing);
            assert_eq!(lab.game.tick, 1);
            assert_eq!(lab.peer.sequence, 1);
            let player = lab.game.player(0).unwrap();
            assert_eq!(player.ammo, 30, "menus cannot fire local commands");
            assert!((player.yaw - 0.8).abs() < 0.000001);
            assert_eq!(player.pitch, -0.2);
            if mode == 1 {
                assert_eq!(
                    lab.game.player(1).unwrap().ammo,
                    29,
                    "remote command still runs"
                );
            } else {
                let command = lab.peer.pending.front().unwrap();
                assert_eq!(command.buttons, 0);
                assert_eq!(command.move_z, 0.);
            }
            assert_eq!(lab.training_progress(), TrainingProgress::default());
            assert!(
                !lab.reset_interactive(),
                "cannot reset another player's match"
            );
            assert_eq!(lab.game.tick, 1);
            lab.peer.set_mode(0);
            lab.update_interactive(1., InputFrame::default(), false, 1, 1.);
            assert_eq!(lab.session_state(), SessionState::Paused);
            assert_eq!(lab.game.tick, 1);
        }
    }
    #[test]
    fn training_milestones_follow_accepted_actions_and_completed_reload() {
        let mut lab = Lab::new_interactive();
        for _ in 0..64 {
            lab.update_interactive(
                1. / 64.,
                InputFrame {
                    move_x: 1.,
                    ..Default::default()
                },
                true,
                1,
                1.,
            );
        }
        assert_eq!(lab.training_progress().distance, 5.);
        // Put a moving target in clear sight, away from courtyard cover.
        let origin = lab.game.player(0).unwrap().position;
        lab.game.player_mut(1).unwrap().position = origin + vec3(0., 0., -3.);
        let shot = InputFrame {
            buttons: ferravex_fps::BUTTON_FIRE,
            ..Default::default()
        };
        lab.update_interactive(1. / 64., shot, true, 1, 1.);
        assert_eq!(lab.training_progress().shots, 1);
        assert_eq!(lab.training_progress().hits, 1);
        lab.update_interactive(1. / 64., shot, true, 1, 1.);
        assert_eq!(
            lab.training_progress().shots,
            1,
            "cooldown fire is not accepted"
        );
        lab.update_interactive(
            1. / 64.,
            InputFrame {
                buttons: ferravex_fps::BUTTON_RELOAD,
                ..Default::default()
            },
            true,
            1,
            1.,
        );
        assert_eq!(lab.training_progress().reloads, 0);
        lab.update_interactive(10., InputFrame::default(), false, 1, 1.);
        assert_eq!(
            lab.training_progress().reloads,
            0,
            "pause does not finish reload"
        );
        for _ in 0..ferravex_fps::RELOAD_TICKS {
            lab.update_interactive(1. / 64., InputFrame::default(), true, 1, 1.);
        }
        assert_eq!(lab.training_progress().reloads, 1);
        assert_eq!(lab.training_mask(), 15);
        let serial = lab.objectives.last_event_serial();
        let views: Vec<_> = lab.training_objectives().collect();
        assert!(
            views
                .iter()
                .all(|v| v.state == ferravex_engine::play::objective::ObjectiveState::Complete)
        );
        assert_eq!(
            lab.objectives.last_event_serial(),
            serial,
            "HUD inspection is read-only"
        );
        assert_eq!(lab.game.player(0).unwrap().ammo, 30);
    }
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
