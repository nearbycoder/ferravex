//! A compact destruction range: all knockdowns come from rigid-body contacts.
//!
//! The host uses a shared session to gate fixed physics ticks. Starting or
//! rebuilding never fires a shot; only a fresh Fire edge can spend ammunition.
//!
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! let mut session = GameSession::new(120.0, 12, false);
//! assert!(session.start());
//! let frame = session.advance(1.0 / 60.0);
//! assert_eq!(frame.steps, 2);
//! assert!(session.finish(true));
//! assert_eq!(session.state(), SessionState::Won);
//! assert_eq!(session.advance(1.0).steps, 0);
//! session.restart(); // The host also rebuilds bodies, ammunition and feedback.
//! assert_eq!(session.tick(), 0);
//! ```
mod atmosphere;
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
mod hud;
mod juice;
mod quips;
mod score;

use ferravex_core::session::{GameSession, SessionState};
use ferravex_physics::d3::{
    ColliderBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle, Vector,
};
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    HintDevice, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::{
    demo::Demo,
    mesh::Mesh3d,
    preferences,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
};
use juice::{Juice, Material};
use quips::{Banter, Mood};
use score::{Callout, Events, RangeEvent, Tally};

#[cfg(test)]
const DT: f32 = 1.0 / 120.0;
const AMMO: u8 = 8;
const GOAL: usize = 6;
const SPEED: f32 = 28.0;
const RELOAD: f32 = 0.8;
const SETTLE: f32 = 6.0;
const PIVOT: Vec3 = Vec3::new(0.0, 1.3, 15.0);
const INK: Color = Color::from_rgba(34, 40, 39, 255);
const PAPER: Color = Color::from_rgba(246, 235, 211, 255);
const RUST: Color = Color::from_rgba(176, 86, 48, 255);
const SAND: Color = Color::from_rgba(194, 169, 124, 255);
const STEEL: Color = Color::from_rgba(65, 81, 78, 255);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Aiming,
    Won,
    Lost,
}

struct Block {
    handle: RigidBodyHandle,
    home: Vec3,
    target: bool,
    down: bool,
    previous_position: Vec3,
    previous_rotation: Quat,
}

struct Shot {
    handle: RigidBodyHandle,
    previous_position: Vec3,
    /// Velocity after the previous tick; a sharp change is a reported impact.
    velocity: Vec3,
    impacts: u8,
}

struct Range {
    physics: PhysicsWorld,
    blocks: Vec<Block>,
    shots: Vec<Shot>,
    yaw: f32,
    elevation: f32,
    ammo: u8,
    cooldown: f32,
    since_shot: f32,
    elapsed: f32,
    phase: Phase,
    preview: Vec<Vec3>,
    preview_hit: Option<Vec3>,
    preview_aim: (u32, u32),
    /// Demolition score, fed only by accepted ticks and shots.
    tally: Tally,
    /// Accepted events from the latest fire/step, drained by the session host.
    events: Events,
}

impl Range {
    fn new() -> Self {
        let mut physics = PhysicsWorld::new([0.0, -9.81, 0.0]);
        physics.add_box([0.0, -0.4, 4.0], [14.0, 0.4, 20.0], false);
        let mut blocks = Vec::new();
        for x in [-4.0, 0.0, 4.0] {
            let mut previous = None;
            for layer in 0..4 {
                let home = vec3(x, 0.6 + layer as f32 * 1.202, -4.0);
                let (handle, _) = physics.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(Vector::from_array(home.to_array()))
                        .linear_damping(0.12)
                        .angular_damping(0.18)
                        .ccd_enabled(true),
                    ColliderBuilder::cuboid(0.6, 0.6, 0.6)
                        .density(0.85)
                        .friction(0.55)
                        .restitution(0.025),
                );
                if let Some(a) = previous {
                    physics
                        .insert_joint(
                            a,
                            handle,
                            ferravex_physics::d3::JointBuilder::fixed()
                                .local_anchor1(Vec3::Y * 0.601)
                                .local_anchor2(-Vec3::Y * 0.601)
                                .break_limits(500., 80.),
                        )
                        .unwrap();
                }
                previous = Some(handle);
                blocks.push(Block {
                    handle,
                    home,
                    target: layer >= 2,
                    down: false,
                    previous_position: home,
                    previous_rotation: Quat::IDENTITY,
                });
            }
        }
        Self {
            physics,
            blocks,
            shots: Vec::with_capacity(AMMO as usize),
            yaw: 0.0,
            elevation: 0.13,
            ammo: AMMO,
            cooldown: 0.0,
            since_shot: 0.0,
            elapsed: 0.0,
            phase: Phase::Aiming,
            preview: Vec::with_capacity(45),
            preview_hit: None,
            preview_aim: (u32::MAX, u32::MAX),
            tally: Tally::default(),
            events: Events::new(),
        }
    }

    /// The nearest block whose cube is touched by a ball centred at `point`
    /// (the preview reports the ball's centre at first contact).
    fn block_at(&self, point: Vec3) -> Option<&Block> {
        self.blocks
            .iter()
            .filter_map(|block| {
                let body = &self.physics.bodies[block.handle];
                let center = Vec3::from_array(body.translation().to_array());
                let rotation = Quat::from_array(body.rotation().to_array());
                let local = (rotation.inverse() * (point - center)).abs();
                (local.max_element() < 1.0).then_some((block, local.length()))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(block, _)| block)
    }

    fn refresh_preview(&mut self, simulated: bool) {
        if self.cooldown > 0. {
            return;
        }
        let aim = (self.yaw.to_bits(), self.elevation.to_bits());
        let moving = simulated
            && (self.blocks.iter().any(|block| {
                let body = &self.physics.bodies[block.handle];
                Vec3::from_array(body.translation().to_array()) != block.previous_position
                    || Quat::from_array(body.rotation().to_array()) != block.previous_rotation
            }) || self.shots.iter().any(|shot| {
                Vec3::from_array(self.physics.bodies[shot.handle].translation().to_array())
                    != shot.previous_position
            }));
        if aim == self.preview_aim && !moving {
            return;
        }
        self.preview_aim = aim;
        self.preview.clear();
        self.preview_hit = None;
        let direction = self.direction();
        let start = PIVOT + direction * 1.6;
        self.preview.push(start);
        for i in 1..=44 {
            let t = i as f32 * 0.035;
            let next = start + direction * SPEED * t - Vec3::Y * (4.905 * t * t);
            let previous = *self.preview.last().unwrap();
            let delta = next - previous;
            if let Some(hit) = self.physics.spherecast(
                previous.to_array(),
                delta.to_array(),
                delta.length(),
                0.36,
                u32::MAX,
                None,
            ) {
                let point = Vec3::from_array(hit.point);
                self.preview.push(point);
                self.preview_hit = Some(point);
                break;
            }
            self.preview.push(next);
        }
    }

    fn direction(&self) -> Vec3 {
        vec3(
            self.yaw.sin() * self.elevation.cos(),
            self.elevation.sin(),
            -self.yaw.cos() * self.elevation.cos(),
        )
    }

    fn score(&self) -> usize {
        self.blocks.iter().filter(|b| b.target && b.down).count()
    }

    fn fire(&mut self) -> bool {
        if !self.can_fire() {
            return false;
        }
        let direction = self.direction();
        let muzzle = PIVOT + direction * 1.6;
        let (handle, _) = self.physics.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::from_array(muzzle.to_array()))
                .linvel(Vector::from_array((direction * SPEED).to_array()))
                .ccd_enabled(true),
            ColliderBuilder::ball(0.36)
                .density(18.0)
                .friction(0.6)
                .restitution(0.08),
        );
        self.shots.push(Shot {
            handle,
            previous_position: muzzle,
            velocity: direction * SPEED,
            impacts: 0,
        });
        self.ammo -= 1;
        self.cooldown = RELOAD;
        self.since_shot = 0.0;
        let cleared = (0..score::COLUMNS)
            .filter(|column| self.blocks[column * 4 + 2].down && self.blocks[column * 4 + 3].down)
            .fold(0, |mask, column| mask | 1 << column);
        self.tally.fired(cleared, &mut self.events);
        self.events.push(RangeEvent::Fired {
            muzzle,
            direction,
            last: self.ammo == 0,
        });
        true
    }

    fn can_fire(&self) -> bool {
        self.phase == Phase::Aiming && self.cooldown <= 0.0 && self.ammo > 0
    }

    fn step(&mut self, dt: f32) {
        // A result is final: late debris must not change a loss into a win.
        if self.phase != Phase::Aiming {
            return;
        }
        self.elapsed += dt;
        self.since_shot += dt;
        self.cooldown = (self.cooldown - dt).max(0.0);
        for block in &mut self.blocks {
            let body = &self.physics.bodies[block.handle];
            block.previous_position = Vec3::from_array(body.translation().to_array());
            block.previous_rotation = Quat::from_array(body.rotation().to_array());
        }
        for shot in &mut self.shots {
            shot.previous_position =
                Vec3::from_array(self.physics.bodies[shot.handle].translation().to_array());
        }
        self.physics.step(dt);
        for shot in &mut self.shots {
            let body = &self.physics.bodies[shot.handle];
            let position = Vec3::from_array(body.translation().to_array());
            let velocity = Vec3::from_array(body.linvel().to_array());
            let change = (velocity - shot.velocity).length();
            if change > 6.0 && shot.impacts < 3 {
                shot.impacts += 1;
                self.events.push(RangeEvent::Impact {
                    at: position,
                    strength: change,
                });
            }
            shot.velocity = velocity;
            if position.distance(score::BELL) < score::BELL_RADIUS {
                self.tally.bell(score::BELL, &mut self.events);
            }
        }
        // Every block can be knocked down for points; only targets count
        // toward the win. Blocks are stored tower by tower, bottom to top.
        let mut down: [bool; 12] = std::array::from_fn(|i| self.blocks[i].down);
        let mut fallen = [0_usize; 12];
        let mut count = 0;
        for (index, block) in self.blocks.iter_mut().enumerate() {
            if block.down {
                continue;
            }
            let body = &self.physics.bodies[block.handle];
            let p = Vec3::from_array(body.translation().to_array());
            let rotation = Quat::from_array(body.rotation().to_array());
            let up = (rotation * Vec3::Y).y;
            block.down = p.distance(block.home) > 1.3 || p.y < block.home.y - 0.75 || up < 0.45;
            if block.down {
                fallen[count] = index;
                count += 1;
            } else if index % 4 == 3 && (up < 0.985 || p.distance(block.home) > 0.25) {
                self.tally.wobble(index / 4, p, &mut self.events);
            }
        }
        for &index in &fallen[..count] {
            down[index] = true;
            let column = index / 4;
            let block = &self.blocks[index];
            let at = Vec3::from_array(self.physics.bodies[block.handle].translation().to_array());
            let tower = [0, 1, 2, 3].map(|layer| down[column * 4 + layer]);
            self.tally
                .knocked(column, block.target, at, tower, &mut self.events);
        }
        self.tally.tick(self.since_shot, &mut self.events);
        if self.score() == GOAL {
            self.phase = Phase::Won;
            self.tally
                .won(self.ammo, vec3(0.0, 2.4, -4.0), &mut self.events);
        } else if self.ammo == 0 && self.since_shot > SETTLE {
            self.phase = Phase::Lost;
        }
    }

    /// A physical low arc from the barrel muzzle to a selected point.
    fn aim_at(&mut self, point: Vec3) {
        let delta = point - PIVOT;
        self.yaw = delta.x.atan2(-delta.z);
        let distance = vec2(delta.x, delta.z).length();
        let mut low: f32 = 0.025;
        let mut high: f32 = 0.7;
        for _ in 0..24 {
            let angle = (low + high) * 0.5;
            let time = (distance - 1.6 * angle.cos()) / (SPEED * angle.cos());
            let height = PIVOT.y + distance * angle.tan() - 4.905 * time * time;
            if height < point.y {
                low = angle;
            } else {
                high = angle;
            }
        }
        self.elevation = (low + high) * 0.5;
    }

    /// Autoplay chooses the next standing tower and uses the same fire operation.
    fn autoplay(&mut self) -> bool {
        if self.phase != Phase::Aiming || self.elapsed < 0.7 || self.since_shot < 2.0 {
            return false;
        }
        if let Some(target) = self.blocks.iter().find(|b| b.target && !b.down) {
            self.aim_at(vec3(target.home.x, 1.8, target.home.z));
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
struct Feedback {
    shot: f32,
    destruction: f32,
    knocked_down: usize,
    rebuilt: f32,
    recoil: f32,
    motion_scale: f32,
}

impl Feedback {
    fn sync_preferences(&mut self) {
        let policy = preferences::feedback_policy();
        self.apply_motion(policy.reduced_motion(), policy.intensity());
        // This range has no flash, camera shake or particle channels. Static
        // confirmations and target colors remain visible under either policy.
    }

    fn apply_motion(&mut self, reduced: bool, intensity: f32) {
        self.motion_scale = if reduced { 0.0 } else { intensity };
        if self.motion_scale == 0.0 {
            // Forget the old pose impulse, including during paused/zero ticks.
            self.recoil = 0.0;
        }
    }

    fn accepted_shot(&mut self) {
        self.shot = 1.0;
        self.rebuilt = 0.0;
        self.recoil = if self.motion_scale > 0.0 { 0.3 } else { 0.0 };
    }

    fn barrel_offset(&self) -> f32 {
        self.recoil * self.motion_scale
    }

    fn step(&mut self, dt: f32) {
        self.shot = (self.shot - dt).max(0.0);
        self.destruction = (self.destruction - dt).max(0.0);
        self.rebuilt = (self.rebuilt - dt).max(0.0);
        self.recoil = (self.recoil - dt * 1.2).max(0.0);
    }
}

#[derive(Default)]
struct FrameFeedback {
    alpha: f32,
    fired: bool,
    destroyed: usize,
    outcome: Option<Phase>,
    /// Sandstone supports knocked down this frame.
    loosened: usize,
    impacts: usize,
    wobbled: bool,
    /// The most important callout this frame, for audio.
    callout: Option<Callout>,
}

#[derive(Default)]
struct TickInput {
    fire: bool,
    aim: Vec2,
}

/// Session host shared by interactive play and headless tests. Presentation
/// policy is read here, outside the range's fixed physics and gameplay rules.
struct Foundry {
    range: Range,
    session: GameSession,
    pending_fire: bool,
    feedback: Feedback,
    juice: Juice,
    /// Foreman Gus's line counters; kept across rebuilds so retries vary.
    banter: Banter,
    /// Debug capture only: replaces autoplay's next aim. Always `None` in play.
    scripted_aim: Option<(f32, f32)>,
}

impl Foundry {
    fn new(autoplay: bool) -> Self {
        let mut foundry = Self {
            range: Range::new(),
            session: GameSession::new(120.0, 12, autoplay),
            pending_fire: false,
            feedback: Feedback::default(),
            juice: Juice::new(),
            banter: Banter::default(),
            scripted_aim: None,
        };
        if foundry.session.is_playing() {
            // Autoplay sessions start themselves; Gus still says hello.
            foundry.juice.say(&mut foundry.banter, Mood::Start);
        }
        foundry
    }

    fn start(&mut self) -> bool {
        let started = self.session.start();
        if started {
            self.juice.say(&mut self.banter, Mood::Start);
        }
        started
    }

    fn rebuild(&mut self) {
        self.range = Range::new();
        self.session.restart();
        self.pending_fire = false;
        self.feedback = Feedback {
            rebuilt: 1.8,
            ..Feedback::default()
        };
        self.juice.reset();
        self.juice.say(&mut self.banter, Mood::Retry);
    }

    fn set_paused(&mut self, paused: bool) {
        let was_paused = self.session.is_paused();
        self.session.set_paused(paused);
        if paused {
            self.pending_fire = false;
            if !was_paused && self.session.is_paused() {
                self.banter.pause();
            }
        }
    }

    fn queue_fire(&mut self) {
        // Ignore reload/terminal presses instead of deferring an unwanted shot.
        if self.session.is_playing() && self.range.can_fire() {
            self.pending_fire = true;
        }
    }

    /// The finished (or current) run for the results card and rank title.
    fn summary(&self) -> quips::RunSummary {
        let tally = &self.range.tally;
        let won = self.range.phase == Phase::Won;
        quips::RunSummary {
            won,
            stars: score::stars(won, tally.shots),
            targets: self.range.score() as u32,
            misses: tally.misses,
            wonder: tally.wonders > 0,
            domino: tally.dominoes > 0,
            bell: tally.bell,
        }
    }

    #[cfg(test)]
    fn advance(&mut self, elapsed: f64, autoplay: bool) -> FrameFeedback {
        self.advance_input(elapsed, autoplay, TickInput::default)
    }

    fn advance_input(
        &mut self,
        elapsed: f64,
        autoplay: bool,
        mut input: impl FnMut() -> TickInput,
    ) -> FrameFeedback {
        // Presentation settings are sampled even when session.advance produces
        // no ticks (ready, paused, controls, results, or a high render rate).
        self.feedback.sync_preferences();
        let policy = preferences::feedback_policy();
        self.juice.apply_policy(
            policy.reduced_motion(),
            policy.reduced_flash(),
            policy.intensity(),
        );
        let frame = self.session.advance(elapsed);
        let mut events = FrameFeedback {
            alpha: frame.alpha,
            ..FrameFeedback::default()
        };
        for _ in 0..frame.steps {
            self.feedback.step(frame.dt);
            self.juice.step(frame.dt, self.range.tally.points);
            let input = input();
            if !autoplay {
                self.range.yaw =
                    (self.range.yaw + input.aim.x * frame.dt * 0.33).clamp(-0.55, 0.55);
                self.range.elevation =
                    (self.range.elevation + input.aim.y * frame.dt * 0.2).clamp(0.025, 0.65);
                if input.fire {
                    self.queue_fire();
                }
            }
            if autoplay && self.range.autoplay() {
                if let Some((yaw, elevation)) = self.scripted_aim.take() {
                    self.range.yaw = yaw;
                    self.range.elevation = elevation;
                }
                self.queue_fire();
            }
            if std::mem::take(&mut self.pending_fire) && self.range.fire() {
                self.feedback.accepted_shot();
                events.fired = true;
            }
            let before = self.range.score();
            self.range.step(frame.dt);
            let destroyed = self.range.score() - before;
            if destroyed > 0 {
                self.feedback.knocked_down = if self.feedback.destruction > 0.0 {
                    self.feedback.knocked_down + destroyed
                } else {
                    destroyed
                };
                self.feedback.destruction = 2.0;
                events.destroyed += destroyed;
            }
            // Juice and audio react only to events the range accepted.
            for event in self.range.events.as_slice() {
                match *event {
                    RangeEvent::Knocked { target: false, .. } => events.loosened += 1,
                    RangeEvent::Impact { .. } => events.impacts += 1,
                    RangeEvent::Wobble { .. } => events.wobbled = true,
                    RangeEvent::Callout { callout, .. }
                        if events
                            .callout
                            .is_none_or(|old| callout.priority() > old.priority()) =>
                    {
                        events.callout = Some(callout);
                    }
                    _ => {}
                }
                self.juice.event(*event, &mut self.banter);
            }
            self.range.events.clear();
            self.juice.sample_trail(self.range.shots.last().map(|shot| {
                let body = &self.range.physics.bodies[shot.handle];
                (
                    Vec3::from_array(body.translation().to_array()),
                    Vec3::from_array(body.linvel().to_array()).length(),
                )
            }));
            if self.range.ammo == 0
                && self.range.phase == Phase::Aiming
                && self.range.since_shot > score::MISS_AFTER
            {
                self.juice.settling(&mut self.banter);
            }
            if self.range.phase != Phase::Aiming {
                self.session.finish(self.range.phase == Phase::Won);
                self.pending_fire = false;
                events.outcome = Some(self.range.phase);
                break;
            }
        }
        if !self.session.is_playing() {
            // Show the current pose when frozen, not the prior tick's pose.
            events.alpha = 1.0;
        }
        self.range.refresh_preview(frame.steps > 0);
        events
    }
}

/// Foreman Gus stands beside the right tower, safely out of the line of fire.
const GUS: Vec3 = vec3(6.7, 0.0, -2.0);
const GUS_FACING: f32 = 0.5;
/// Gus is a big lad, drawn half again life size so he reads at a distance.
const GUS_SCALE: f32 = 1.5;
/// The bell hangs from the girder's centre on a short rope.
const BELL_PIVOT: Vec3 = vec3(0.0, 6.75, -9.0);
const HIVIS: Color = Color::from_rgba(240, 150, 44, 255);
const HARDHAT: Color = Color::from_rgba(250, 206, 64, 255);
const SKIN: Color = Color::from_rgba(226, 178, 138, 255);
const DENIM: Color = Color::from_rgba(58, 72, 98, 255);
const BRASS: Color = Color::from_rgba(214, 168, 72, 255);
const SOOT: Color = Color::from_rgba(76, 78, 72, 255);

struct Art {
    floor: Mesh3d,
    stone: Mesh3d,
    target: Mesh3d,
    fallen: Mesh3d,
    ball: Mesh3d,
    barrel: Mesh3d,
    muzzle: Mesh3d,
    wheel: Mesh3d,
    chassis: Mesh3d,
    line: Mesh3d,
    column: Mesh3d,
    girder: Mesh3d,
    chimney: Mesh3d,
    roof: Mesh3d,
    building: Mesh3d,
    rivet: Mesh3d,
    chunk_rust: Mesh3d,
    chunk_sand: Mesh3d,
    chunk_soot: Mesh3d,
    bell: Mesh3d,
    clapper: Mesh3d,
    rope: Mesh3d,
    gus_legs: Mesh3d,
    gus_torso: Mesh3d,
    gus_stripe: Mesh3d,
    gus_arm: Mesh3d,
    gus_head: Mesh3d,
    gus_hat: Mesh3d,
    gus_brim: Mesh3d,
    gus_tash: Mesh3d,
    gus_eye: Mesh3d,
    gus_board: Mesh3d,
    ground: Mesh3d,
    shed: Mesh3d,
    stack: Mesh3d,
    furnace: Mesh3d,
    furnace_arch: Mesh3d,
    channel: Mesh3d,
    channel_rail: Mesh3d,
}

/// Warm, sooty foundry palette for the world (targets keep their rust red).
const FLOOR: Color = Color::from_rgba(92, 78, 66, 255);
const BRICK: Color = Color::from_rgba(122, 82, 62, 255);
const CHARRED: Color = Color::from_rgba(70, 60, 54, 255);

impl Art {
    fn new() -> Self {
        Self {
            floor: Mesh3d::cuboid(vec3(28.0, 0.8, 40.0), FLOOR),
            stone: Mesh3d::cuboid(Vec3::splat(1.17), SAND),
            target: Mesh3d::cuboid(Vec3::splat(1.17), RUST),
            fallen: Mesh3d::cuboid(Vec3::splat(1.2), CHARRED),
            ball: Mesh3d::sphere(0.36, INK, 10, 6),
            barrel: Mesh3d::frustum(0.34, 0.27, 2.4, STEEL, 12),
            muzzle: Mesh3d::cylinder(0.2, 0.035, INK, 12),
            wheel: Mesh3d::cylinder(0.6, 0.3, INK, 10),
            chassis: Mesh3d::cuboid(vec3(1.5, 0.45, 1.6), RUST),
            line: Mesh3d::cuboid(
                vec3(0.075, 0.008, 26.0),
                Color::from_rgba(168, 128, 84, 255),
            ),
            column: Mesh3d::cuboid(vec3(0.32, 7.0, 0.32), STEEL),
            girder: Mesh3d::cuboid(vec3(18.0, 0.3, 0.34), RUST),
            chimney: Mesh3d::frustum(1.0, 0.7, 12.0, Color::from_rgba(112, 76, 60, 255), 8),
            roof: Mesh3d::cuboid(vec3(8.5, 0.3, 6.5), STEEL),
            building: Mesh3d::cuboid(vec3(8.0, 4.0, 6.0), BRICK),
            rivet: Mesh3d::cylinder(0.16, 0.015, PAPER, 8),
            chunk_rust: Mesh3d::cuboid(Vec3::ONE, RUST),
            chunk_sand: Mesh3d::cuboid(Vec3::ONE, SAND),
            chunk_soot: Mesh3d::cuboid(Vec3::ONE, SOOT),
            bell: Mesh3d::frustum(0.44, 0.2, 0.55, BRASS, 10),
            clapper: Mesh3d::sphere(0.09, INK, 6, 4),
            rope: Mesh3d::cuboid(vec3(0.05, 0.72, 0.05), INK),
            gus_legs: Mesh3d::cuboid(vec3(0.42, 0.62, 0.28), DENIM),
            gus_torso: Mesh3d::cuboid(vec3(0.62, 0.62, 0.38), HIVIS),
            gus_stripe: Mesh3d::cuboid(vec3(0.64, 0.07, 0.4), PAPER),
            gus_arm: Mesh3d::cuboid(vec3(0.14, 0.5, 0.14), HIVIS),
            gus_head: Mesh3d::sphere(0.21, SKIN, 10, 6),
            gus_hat: Mesh3d::frustum(0.24, 0.16, 0.16, HARDHAT, 10),
            gus_brim: Mesh3d::cylinder(0.31, 0.03, HARDHAT, 10),
            gus_tash: Mesh3d::cuboid(vec3(0.22, 0.05, 0.05), Color::from_rgba(92, 62, 44, 255)),
            gus_eye: Mesh3d::cuboid(Vec3::splat(0.045), INK),
            gus_board: Mesh3d::cuboid(vec3(0.3, 0.38, 0.04), Color::from_rgba(150, 112, 70, 255)),
            ground: Mesh3d::cuboid(vec3(260.0, 1.0, 220.0), Color::from_rgba(52, 38, 34, 255)),
            shed: Mesh3d::cuboid(vec3(10.0, 6.0, 8.0), Color::from_rgba(70, 50, 44, 255)),
            stack: Mesh3d::frustum(1.6, 1.0, 22.0, Color::from_rgba(66, 46, 40, 255), 8),
            furnace: Mesh3d::cuboid(vec3(2.4, 1.6, 0.12), atmosphere::MOLTEN),
            furnace_arch: Mesh3d::cuboid(vec3(3.2, 2.3, 0.3), Color::from_rgba(58, 44, 40, 255)),
            channel: Mesh3d::cuboid(vec3(0.7, 0.06, 28.0), Color::from_rgba(196, 74, 26, 255)),
            channel_rail: Mesh3d::cuboid(vec3(0.22, 0.2, 28.0), Color::from_rgba(54, 44, 40, 255)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        game: &Range,
        alpha: f32,
        recoil: f32,
        juice: &Juice,
        camera: &RangeCamera,
        material: &LitMaterial,
        ambient_time: f32,
    ) {
        set_camera(camera);
        material.begin(LitSettings {
            // Low evening sun from the furnace side for long warm shading.
            light_direction: vec3(-0.55, 0.7, 0.45),
            camera_position: camera.eye,
            fog_color: atmosphere::HAZE,
            fog_start: 42.0,
            fog_end: 120.0,
            ambient: 0.5,
        });
        // The yard below the raised range, and the foundry town in the haze.
        self.ground.draw(vec3(0.0, -4.5, -20.0), Quat::IDENTITY);
        for (x, z, turn) in [
            (-46.0, -48.0, 0.2),
            (-28.0, -62.0, -0.1),
            (-6.0, -58.0, 0.05),
            (18.0, -64.0, 0.3),
            (40.0, -50.0, -0.25),
            (-60.0, -20.0, 0.4),
        ] {
            self.shed
                .draw(vec3(x, -1.0, z), Quat::from_rotation_y(turn));
        }
        for (x, z) in [(-36.0, -58.0), (-14.0, -66.0), (8.0, -70.0), (30.0, -60.0)] {
            self.stack.draw(vec3(x, 7.0, z), Quat::IDENTITY);
        }
        self.floor.draw(vec3(0.0, -0.4, 4.0), Quat::IDENTITY);
        // The furnace mouth and the molten channel it feeds.
        self.furnace_arch
            .draw(atmosphere::FURNACE - Vec3::Z * 0.1, Quat::IDENTITY);
        self.furnace.draw(atmosphere::FURNACE, Quat::IDENTITY);
        let channel_z = (atmosphere::CHANNEL_Z.0 + atmosphere::CHANNEL_Z.1) * 0.5;
        self.channel
            .draw(vec3(atmosphere::CHANNEL_X, 0.02, channel_z), Quat::IDENTITY);
        for side in [-0.5, 0.5] {
            self.channel_rail.draw(
                vec3(atmosphere::CHANNEL_X + side, 0.1, channel_z),
                Quat::IDENTITY,
            );
        }
        for x in [-8.0, -6.0, 6.0, 8.0] {
            self.line.draw(vec3(x, 0.012, 4.0), Quat::IDENTITY);
        }
        for x in [-9.0, 9.0] {
            self.column.draw(vec3(x, 3.5, -9.0), Quat::IDENTITY);
        }
        self.girder.draw(vec3(0.0, 6.9, -9.0), Quat::IDENTITY);
        self.building.draw(vec3(-15.0, 2.0, -15.0), Quat::IDENTITY);
        self.roof
            .draw(vec3(-15.0, 4.15, -15.0), Quat::from_rotation_z(-0.1));
        self.chimney.draw(vec3(-12.0, 6.0, -17.0), Quat::IDENTITY);
        self.chimney.draw(vec3(-16.0, 5.0, -18.0), Quat::IDENTITY);
        // The secret lunch bell swings when rung (motion only).
        let swing = Quat::from_rotation_x(juice.bell_angle());
        self.rope
            .draw(BELL_PIVOT + swing * vec3(0.0, -0.36, 0.0), swing);
        self.bell
            .draw(BELL_PIVOT + swing * (score::BELL - BELL_PIVOT), swing);
        self.clapper
            .draw(BELL_PIVOT + swing * vec3(0.0, -1.3, 0.0), swing);
        for block in &game.blocks {
            let body = &game.physics.bodies[block.handle];
            let p = block
                .previous_position
                .lerp(Vec3::from_array(body.translation().to_array()), alpha);
            let q = block
                .previous_rotation
                .slerp(Quat::from_array(body.rotation().to_array()), alpha);
            let mesh = match (block.target, block.down) {
                (true, true) => &self.fallen,
                (true, false) => &self.target,
                (false, _) => &self.stone,
            };
            mesh.draw(p, q);
            if block.target && !block.down {
                self.rivet.draw(
                    p + q * vec3(0.0, 0.0, 0.594),
                    q * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                );
            }
        }
        for shot in &game.shots {
            let p = shot.previous_position.lerp(
                Vec3::from_array(game.physics.bodies[shot.handle].translation().to_array()),
                alpha,
            );
            self.ball.draw(p, Quat::IDENTITY);
        }
        for chunk in juice.chunks.iter().flatten() {
            let mesh = match chunk.material {
                Material::Rust => &self.chunk_rust,
                Material::Sand => &self.chunk_sand,
                Material::Soot => &self.chunk_soot,
            };
            mesh.draw_transformed(Mat4::from_scale_rotation_translation(
                Vec3::splat(chunk.scale()),
                chunk.rotation,
                chunk.position,
            ));
        }
        self.draw_gus(juice, ambient_time);
        let direction = game.direction();
        let q = Quat::from_rotation_arc(Vec3::Y, direction);
        self.chassis
            .draw_transformed(Mat4::from_scale_rotation_translation(
                juice.carriage_scale(),
                Quat::from_rotation_y(-game.yaw),
                vec3(0.0, 0.7, 15.0),
            ));
        for x in [-0.95, 0.95] {
            self.wheel.draw(
                vec3(x, 0.6, 15.0),
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            );
        }
        self.barrel.draw(PIVOT + direction * (0.3 - recoil), q);
        self.muzzle.draw(PIVOT + direction * (1.515 - recoil), q);
        material.end();
        // Screen-space atmosphere follows; the HUD boundary is explicit.
        set_default_camera();
    }

    /// Foreman Gus: idles, frets while a tower wobbles, and hops at a collapse.
    fn draw_gus(&self, juice: &Juice, ambient_time: f32) {
        let motion = juice.motion().min(1.0);
        let bob = (ambient_time * 3.1).sin() * 0.025 * motion;
        let root = Mat4::from_scale_rotation_translation(
            Vec3::splat(GUS_SCALE),
            Quat::from_rotation_y(GUS_FACING) * Quat::from_rotation_z(juice.fret()),
            GUS + Vec3::Y * (juice.hop_height() + bob),
        );
        let part = |mesh: &Mesh3d, at: Vec3, rotation: Quat| {
            mesh.draw_transformed(root * Mat4::from_rotation_translation(rotation, at));
        };
        part(&self.gus_legs, vec3(0.0, 0.31, 0.0), Quat::IDENTITY);
        part(&self.gus_torso, vec3(0.0, 0.93, 0.0), Quat::IDENTITY);
        part(&self.gus_stripe, vec3(0.0, 0.86, 0.0), Quat::IDENTITY);
        part(&self.gus_head, vec3(0.0, 1.46, 0.0), Quat::IDENTITY);
        part(&self.gus_tash, vec3(0.0, 1.4, 0.2), Quat::IDENTITY);
        for x in [-0.075, 0.075] {
            part(&self.gus_eye, vec3(x, 1.5, 0.19), Quat::IDENTITY);
        }
        part(&self.gus_brim, vec3(0.0, 1.57, 0.03), Quat::IDENTITY);
        part(&self.gus_hat, vec3(0.0, 1.66, 0.0), Quat::IDENTITY);
        part(
            &self.gus_board,
            vec3(0.12, 0.95, 0.23),
            Quat::from_rotation_x(-0.35),
        );
        // Arms rise in despair on a collapse and flap while he frets.
        let raise = (juice.hop * 2.6).min(1.0).max(juice.alarm * 0.7) * motion;
        for side in [-1.0_f32, 1.0] {
            let flap = (juice.time * 16.0 + side).sin() * 0.25 * juice.alarm * motion;
            let angle = side * (0.12 + 2.5 * raise + flap);
            let shoulder = vec3(side * 0.39, 1.2, 0.0);
            let rotation = Quat::from_rotation_z(angle);
            part(
                &self.gus_arm,
                shoulder + rotation * vec3(0.0, -0.25, 0.0),
                rotation,
            );
        }
    }
}

/// Fits the authored range between the HUD panels without moving the eye,
/// changing depth/fog, or coupling the view to moving debris or player aim.
struct RangeCamera {
    matrix: Mat4,
    eye: Vec3,
}

impl RangeCamera {
    fn new(w: f32, h: f32) -> Self {
        let base = Camera3D {
            position: vec3(23.0, 18.0, 28.0),
            target: vec3(0.0, 1.5, 3.5),
            up: Vec3::Y,
            fovy: 43.0_f32.to_radians(),
            z_near: 0.1,
            z_far: 130.0,
            ..Default::default()
        };
        let matrix = base.matrix(w, h);
        let scene = HudLayout::new(w, h).scene;
        let mut low = vec2(f32::INFINITY, f32::INFINITY);
        let mut high = vec2(f32::NEG_INFINITY, f32::NEG_INFINITY);
        // Standing towers, cannon including every legal muzzle direction, and
        // the low arcs that reach the supports. The decoration may extend under
        // the HUD; flying balls/debris never cause camera motion.
        for (min, max) in [
            (vec3(-4.6, 0.0, -4.6), vec3(4.6, 4.81, -3.4)),
            (vec3(-1.4, 0.0, 13.0), vec3(1.4, 3.0, 16.4)),
        ] {
            for x in [min.x, max.x] {
                for y in [min.y, max.y] {
                    for z in [min.z, max.z] {
                        let p = matrix.project_point3(vec3(x, y, z));
                        let p = vec2((p.x + 1.0) * w * 0.5, (1.0 - p.y) * h * 0.5);
                        low = low.min(p);
                        high = high.max(p);
                    }
                }
            }
        }
        let scale = (scene.w / (high.x - low.x))
            .min(scene.h / (high.y - low.y))
            .min(1.0);
        // Smallest translation that contains the fitted bounds. In particular,
        // an already fitting desktop view retains its original composition.
        let shift_axis = |low: f32, high: f32, start: f32, size: f32| {
            if low * scale < start {
                start - low * scale
            } else if high * scale > start + size {
                start + size - high * scale
            } else {
                0.0
            }
        };
        let shift = vec2(
            shift_axis(low.x, high.x, scene.x, scene.w),
            shift_axis(low.y, high.y, scene.y, scene.h),
        );
        let offset = vec3(
            scale - 1.0 + 2.0 * shift.x / w,
            1.0 - scale - 2.0 * shift.y / h,
            0.0,
        );
        Self {
            matrix: Mat4::from_translation(offset)
                * Mat4::from_scale(vec3(scale, scale, 1.0))
                * matrix,
            eye: base.position,
        }
    }

    /// A screen-space presentation offset in pixels (camera kick and shake).
    /// The eye, depth and fog are unchanged.
    fn shifted(mut self, w: f32, h: f32, pixels: Vec2) -> Self {
        if pixels != Vec2::ZERO && w > 0.0 && h > 0.0 {
            let offset = vec3(2.0 * pixels.x / w, -2.0 * pixels.y / h, 0.0);
            self.matrix = Mat4::from_translation(offset) * self.matrix;
        }
        self
    }

    /// Screen position of a world point, or `None` behind the eye.
    fn project(&self, w: f32, h: f32, point: Vec3) -> Option<Vec2> {
        let clip = self.matrix * point.extend(1.0);
        (clip.w > 0.01).then(|| {
            vec2(
                (clip.x / clip.w + 1.0) * w * 0.5,
                (1.0 - clip.y / clip.w) * h * 0.5,
            )
        })
    }
}

impl Camera for RangeCamera {
    fn matrix(&self, _: f32, _: f32) -> Mat4 {
        self.matrix
    }

    fn depth(&self) -> bool {
        true
    }

    fn position(&self) -> Option<Vec3> {
        Some(self.eye)
    }
}

fn clicked(rect: Rect) -> bool {
    rect.contains(Vec2::from(mouse_position())) && is_mouse_button_pressed(MouseButton::Left)
}

fn action_hint<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    if input.current_device() == HintDevice::Touch {
        // The preset's Touch 2/confirm channels are not installed here: this
        // range uses its visible pointer buttons and a look-only touch region.
        return "on-screen button";
    }
    // Never silently advertise a default binding after a remap or format error.
    input.format_hint(action, buffer).unwrap_or("See Controls")
}

struct HudLayout {
    scene: Rect,
    panel: Rect,
    fire: Rect,
    rebuild: Rect,
    pause: Rect,
}

impl HudLayout {
    fn new(w: f32, h: f32) -> Self {
        let height = 116.0;
        let panel = Rect::new(16.0, h - height - 16.0, w - 32.0, height);
        let width = (panel.w - 48.0) / 3.0;
        let y = panel.y + panel.h - 54.0;
        Self {
            scene: Rect::new(16.0, 56.0, w - 32.0, (panel.y - 64.0).max(1.0)),
            panel,
            fire: Rect::new(32.0, y, width, 44.0),
            rebuild: Rect::new(40.0 + width, y, width, 44.0),
            pause: Rect::new(48.0 + width * 2.0, y, width, 44.0),
        }
    }
}

/// Personal records are stored per layout/rules version.
const RECORD_CONTEXT: u64 = 1;

/// Read the stored best points and best stars, if any.
fn stored_best(records: Option<&ferravex_render::storage::Records>) -> hud::Best {
    let Some(records) = records else {
        return hud::Best::default();
    };
    hud::Best {
        points: records.get("points", RECORD_CONTEXT).map(|r| r.score),
        stars: records.get("spared", RECORD_CONTEXT).map_or(0, |r| {
            score::stars(true, AMMO.saturating_sub(r.score as u8))
        }),
    }
}

/// Capture the comparison once at the result, then record the attempt.
fn finish_run(
    records: Option<&mut ferravex_render::storage::Records>,
    foundry: &Foundry,
    fake_best: Option<u64>,
) -> hud::Results {
    let tally = &foundry.range.tally;
    let won = foundry.range.phase == Phase::Won;
    let points = u64::from(tally.points);
    let mut previous = stored_best(records.as_deref());
    if fake_best.is_some() {
        previous.points = fake_best;
    }
    if let Some(records) = records {
        // Storage failures keep the in-memory session best; play continues.
        let _ = records.attempt("points", RECORD_CONTEXT, points, None);
        if won {
            let _ = records.attempt("spared", RECORD_CONTEXT, u64::from(tally.spare), None);
        }
    }
    let stars = score::stars(won, tally.shots);
    hud::Results {
        previous,
        new_best: points > 0 && previous.points.is_none_or(|best| points > best),
        new_stars: stars > previous.stars,
    }
}

fn window_conf() -> Conf {
    #[allow(unused_mut)]
    let mut conf = ferravex_render::window_conf("Siege Foundry / Ferravex");
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut conf);
    conf
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut foundry = Foundry::new(demo.autoplay);
    let art = Art::new();
    let material = LitMaterial::new().expect("Ferravex lit material");
    let mut sound = GameAudio::new();
    let mut player_input = PlayerInput::preset("siege-foundry", Preset::Turret);
    player_input.set_mouse_look_button(Some(1)).unwrap();
    let mut records = ferravex_render::storage::Records::new("siege-foundry").ok();
    let mut results = hud::Results::default();
    let mut coach = hud::Coach::default();
    let mut hints = hud::Hints::default();
    let mut hud_anim = hud::HudAnim::default();
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
    let mut look_state = None;
    let mut air = atmosphere::Air {
        time: 0.0,
        motion: 1.0,
        flicker: 1.0,
    };
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let fake_best = capture.fake_best();
    #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
    let fake_best = None;
    loop {
        let (w, h) = (screen_width(), screen_height());
        let layout = HudLayout::new(w, h);
        player_input
            .set_touch_layout(&[Control {
                region: Region::Rect(Rect::new(0.0, 90.0, w, (layout.panel.y - 90.0).max(1.0))),
                output: Output::Look { x: 0, y: 1 },
            }])
            .unwrap();
        // Resolve pointer pause before sampling, so keyboard and pointer resume
        // both re-enable the gameplay context on this frame.
        let pausable = matches!(
            foundry.session.state(),
            SessionState::Playing | SessionState::Paused
        );
        if !demo.autoplay && !player_input.controls_open() && pausable && clicked(layout.pause) {
            player_input.set_paused(!foundry.session.is_paused());
        }
        player_input.sample_play();
        hints.refresh(&player_input);
        let dt = demo.dt();
        #[allow(unused_mut)]
        let mut active = demo.autoplay || player_input.gameplay_active() && !player_input.paused();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        {
            if capture.hold(foundry.session.is_playing()) {
                player_input.set_paused(true);
                active = false;
            }
        }
        let controls_open = player_input.controls_open();
        let confirm = player_input.pressed("confirm");
        let rebuild = player_input.pressed("restart") || !controls_open && clicked(layout.rebuild);
        // Drain relative look while inactive, including the rebuild frame.
        let look = player_input.look(dt);
        let state = foundry.session.state();
        let pointer_fire = !controls_open && clicked(layout.fire);
        // Fire doubles as a menu action only outside play. During play, leave
        // its edge in PlayerInput until an actual simulation tick consumes it.
        let menu_fire = if state != SessionState::Playing || rebuild {
            player_input.pressed("fire")
        } else {
            false
        };
        let restarted = rebuild
            || matches!(state, SessionState::Won | SessionState::Lost)
                && (confirm || menu_fire || pointer_fire);
        let started = state == SessionState::Ready && (confirm || menu_fire || pointer_fire);
        if restarted {
            foundry.rebuild();
            hud_anim.open_doors();
            player_input.set_paused(false);
            sound.reset();
            results = hud::Results::default();
        } else if started {
            foundry.start();
            player_input.set_paused(false);
        } else {
            foundry.set_paused(!active);
        }
        let aim_before = (foundry.range.yaw, foundry.range.elevation);
        if foundry.session.is_playing() && active && !restarted && !started {
            let game = &mut foundry.range;
            let pointer = Vec2::from(mouse_position());
            if pointer.y > 90.0 && pointer.y < layout.panel.y || player_input.touch_active() {
                game.yaw += look.x * 0.7;
                game.elevation -= look.y * 0.45;
            }
            game.yaw = game.yaw.clamp(-0.55, 0.55);
            game.elevation = game.elevation.clamp(0.025, 0.65);
            if pointer_fire {
                foundry.queue_fire();
            }
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if demo.autoplay
            && foundry.scripted_aim.is_none()
            && let Some(aim) = capture.aim(foundry.range.tally.shots)
        {
            foundry.scripted_aim = Some(match aim {
                capture::Aim::Bell => BELL_AIM,
                capture::Aim::Miss => (0.5, 0.13),
            });
        }
        let events = foundry.advance_input(
            if restarted || started {
                0.0
            } else {
                f64::from(dt)
            },
            demo.autoplay,
            || TickInput {
                fire: player_input.take_tick(["fire"])[0].pressed,
                aim: player_input.movement(),
            },
        );
        let aimed = (foundry.range.yaw, foundry.range.elevation) != aim_before;
        coach.aimed |= aimed && !demo.autoplay || events.fired && demo.autoplay;
        coach.fired |= events.fired;
        coach.knocked |= events.destroyed > 0 || events.loosened > 0;
        if events.fired {
            sound.cue(Cue::Fire, 0.8);
        }
        if events.impacts > 0 {
            sound.cue(Cue::Impact(Surface::Stone), 0.45);
        }
        if events.loosened > 0 {
            sound.cue(Cue::Impact(Surface::Wood), 0.5);
        }
        if events.destroyed > 0 {
            sound.cue(Cue::Impact(Surface::Metal), 0.6);
        }
        if events.wobbled {
            sound.cue(Cue::Footstep(Surface::Wood), 0.4);
        }
        match events.callout {
            Some(Callout::Bell) => {
                sound.cue(Cue::Pickup, 0.8);
            }
            Some(Callout::Domino | Callout::OneShotWonder | Callout::Overkill) => {
                sound.cue(Cue::Pickup, 0.55);
            }
            Some(Callout::Chain(_) | Callout::Timber | Callout::Delayed) => {
                sound.cue(Cue::Reload, 0.45);
            }
            Some(Callout::Spare(_)) | None => {}
        }
        if let Some(outcome) = events.outcome {
            sound.cue(
                if outcome == Phase::Won {
                    Cue::Success
                } else {
                    Cue::Failure
                },
                0.65,
            );
            results = finish_run(records.as_mut(), &foundry, fake_best);
        }
        if matches!(
            foundry.session.state(),
            SessionState::Won | SessionState::Lost
        ) {
            foundry.juice.result_time(dt);
        }
        let state = foundry.session.state();
        if look_state != Some(state) {
            // Sticky post settings change only when the screen changes.
            set_post_fx(atmosphere::look(state));
            look_state = Some(state);
        }
        let policy = preferences::feedback_policy();
        let air_motion = if policy.reduced_motion() {
            0.0
        } else {
            policy.intensity().min(1.0)
        };
        air.time += dt * air_motion;
        air.motion = air_motion;
        air.flicker = if policy.reduced_flash() { 0.0 } else { 1.0 };
        clear_background(atmosphere::SKY_TOP);
        atmosphere::sky(w, h);
        let camera = RangeCamera::new(w, h).shifted(w, h, foundry.juice.camera_offset());
        art.draw(
            &foundry.range,
            events.alpha,
            foundry.feedback.barrel_offset(),
            &foundry.juice,
            &camera,
            &material,
            air.time,
        );
        atmosphere::world(&foundry, &camera, w, h, air);
        post_fx_hud_begin();
        let best = stored_best(records.as_ref());
        hud_anim.update(
            dt.clamp(0., 0.1),
            state,
            &layout,
            hud::enabled_buttons(&foundry, state, controls_open),
        );
        hud::draw(&hud::View {
            foundry: &foundry,
            layout: &layout,
            camera: &camera,
            w,
            h,
            controls_open,
            hints: &hints,
            results: &results,
            best,
            coach: &coach,
            anim: &hud_anim,
        });
        // The range supplies its own pause panel; retain the shared editor.
        if !player_input.paused() || controls_open {
            player_input.draw_settings();
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

/// Yaw and elevation that send a ball over the centre tower and through the
/// secret lunch bell. Used by debug captures; kept honest by a regression test
/// that fires it through real physics.
#[cfg_attr(
    not(any(test, all(debug_assertions, not(target_arch = "wasm32")))),
    allow(dead_code)
)]
const BELL_AIM: (f32, f32) = (0.0, 0.335);

#[cfg(test)]
mod journey200_tests;

#[cfg(test)]
mod journey200_194;

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_render::ui;

    #[test]
    fn tick_input_is_not_read_while_ready_paused_or_on_zero_tick_frames() {
        let mut foundry = Foundry::new(false);
        let mut calls = 0;
        let mut fire = true;
        let mut input = || {
            calls += 1;
            TickInput {
                fire: std::mem::take(&mut fire),
                aim: Vec2::ZERO,
            }
        };
        foundry.advance_input(1.0, false, &mut input);
        foundry.session.start();
        foundry.advance_input(0.001, false, &mut input);
        foundry.set_paused(true);
        foundry.advance_input(1.0, false, &mut input);
        foundry.set_paused(false);
        let events = foundry.advance_input(0.05, false, &mut input);
        assert!(events.fired);
        assert_eq!(calls, 6);
        assert!(!fire);
        assert_eq!(foundry.range.shots.len(), 1);
        assert_eq!(foundry.range.ammo, AMMO - 1);
    }

    #[test]
    fn player_input_taps_survive_zero_ticks_and_reload_rejection_is_not_buffered() {
        use ferravex_render::input::{self, ActionKind, Binding, InputCode, PadState};
        let mut player = PlayerInput::transient("journey200-139-ticks");
        player
            .action(
                "fire",
                "Fire",
                ActionKind::Button,
                vec![Binding::digital(InputCode::PadButton(7))],
            )
            .unwrap();
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.5;
        let mut frame = |dt, button| {
            pad.buttons[7] = button;
            assert!(input::gamepad_sample(0, pad));
            input::host_frame(dt as f32, 320, 448, || {
                player.sample_play();
                foundry
                    .advance_input(dt, false, || TickInput {
                        fire: player.take_tick(["fire"])[0].pressed,
                        aim: Vec2::ZERO,
                    })
                    .fired
            })
        };
        // Connect released first, then press/release entirely before a tick.
        assert!(!frame(0.0, 0.0));
        assert!(!frame(0.001, 1.0));
        assert!(!frame(0.001, 0.0));
        assert!(frame(0.05, 0.0));
        assert!(!frame(0.001, 1.0));
        assert!(!frame(0.1, 0.0)); // Rejected at the tick while still loading.
        for _ in 0..10 {
            assert!(!frame(0.1, 0.0));
        }
        assert!(frame(0.01, 1.0)); // Only a fresh ready press fires again.
        assert_eq!(foundry.range.ammo, AMMO - 2);
        assert_eq!(foundry.range.shots.len(), 2);
        assert!(input::gamepad_sample(0, PadState::default()));
    }

    #[test]
    fn held_aim_uses_equal_fixed_ticks_at_30_60_and_144_hz() {
        let mut reference = None;
        for fps in [30, 60, 144] {
            let mut foundry = Foundry::new(true);
            for _ in 0..fps {
                foundry.advance_input(1.0 / f64::from(fps), false, || TickInput {
                    fire: false,
                    aim: vec2(0.5, 0.25),
                });
            }
            assert_eq!(foundry.session.tick(), 120);
            let aim = (foundry.range.yaw, foundry.range.elevation);
            assert!(aim.0 > 0.16 && aim.0 < 0.17);
            assert!(aim.1 > 0.17 && aim.1 < 0.19);
            if let Some(reference) = reference {
                assert_eq!(aim, reference);
            }
            reference = Some(aim);
        }
    }

    #[test]
    fn motion_policy_cancels_old_recoil_without_erasing_shot_confirmation() {
        let mut feedback = Feedback::default();
        feedback.apply_motion(false, 1.0);
        feedback.accepted_shot();
        assert_eq!(feedback.barrel_offset(), 0.3);
        feedback.apply_motion(false, 0.5);
        assert_eq!(feedback.barrel_offset(), 0.15);
        // No time need pass: changing policy while paused must retire the pose.
        feedback.apply_motion(true, 1.0);
        assert_eq!(feedback.barrel_offset(), 0.0);
        assert_eq!(feedback.shot, 1.0);
        feedback.apply_motion(false, 1.0);
        assert_eq!(feedback.barrel_offset(), 0.0);
        feedback.apply_motion(true, 1.0);
        feedback.accepted_shot();
        feedback.apply_motion(false, 1.0);
        assert_eq!(
            feedback.barrel_offset(),
            0.0,
            "disabled shots cannot revive"
        );
        feedback.accepted_shot();
        assert!(feedback.barrel_offset() > 0.0);
        feedback.apply_motion(false, 0.0);
        feedback.apply_motion(false, 1.0);
        assert_eq!(feedback.barrel_offset(), 0.0);
    }

    #[test]
    fn ready_paused_and_terminal_frames_sample_shared_presentation_policy() {
        preferences::set_feedback_policy(Default::default());
        let mut foundry = Foundry::new(false);
        for state in [
            SessionState::Ready,
            SessionState::Playing,
            SessionState::Paused,
            SessionState::Won,
        ] {
            match state {
                SessionState::Playing => {
                    foundry.session.start();
                }
                SessionState::Paused => foundry.set_paused(true),
                SessionState::Won => {
                    foundry.session.finish(true);
                }
                _ => {}
            }
            foundry.feedback.motion_scale = 0.25;
            foundry.advance(0.0, false);
            assert_eq!(foundry.feedback.motion_scale, 1.0, "{state:?}");
            assert_eq!(foundry.session.tick(), 0);
            assert_eq!(foundry.range.ammo, AMMO);
        }
    }

    #[test]
    fn hud_hints_follow_remaps_unbound_actions_and_bounded_buffer_errors() {
        use ferravex_render::input::{ActionKind, Binding, InputCode};
        let mut player = PlayerInput::transient("journey200-139-hints");
        player
            .action(
                "fire",
                "Fire",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let mut buffer = [0; 512];
        assert_eq!(action_hint(&player, "fire", &mut buffer), "Space");
        player
            .rebind("fire", vec![Binding::digital(InputCode::Key(b'f' as u16))])
            .unwrap();
        assert_eq!(action_hint(&player, "fire", &mut buffer), "F");
        player.rebind("fire", vec![]).unwrap();
        assert_eq!(action_hint(&player, "fire", &mut buffer), "Unbound");
        assert_eq!(action_hint(&player, "fire", &mut []), "See Controls");
        assert_eq!(action_hint(&player, "missing", &mut buffer), "See Controls");
    }

    #[test]
    fn narrow_hud_keeps_controls_and_fixed_size_text_inside_panels() {
        let metrics = get_default_font().try_metrics(16, 1.0).unwrap();
        for width in [320.0, 390.0, 700.0, 1280.0] {
            let layout = HudLayout::new(width, 448.0);
            let mut last_right = layout.panel.x;
            for rect in [layout.fire, layout.rebuild, layout.pause] {
                assert!(rect.x >= last_right);
                assert!(rect.x + rect.w <= layout.panel.x + layout.panel.w);
                assert!(rect.y >= layout.panel.y);
                assert!(rect.y + rect.h <= layout.panel.y + layout.panel.h);
                assert!(rect.h >= 44.0);
                last_right = rect.x + rect.w;
            }
            assert!(
                layout.panel.y > 216.0,
                "result panel must not cover buttons"
            );
            for text in [
                "YAW -32 / ELEV 37.2",
                "POWER 28 m/s - fixed",
                "REBUILT: 8 SHOTS READY",
                "SETTLING 6.0s",
            ] {
                let measured = metrics.measure_line(text).unwrap();
                assert_eq!(measured.unsupported_glyphs, 0);
                assert!(measured.width <= layout.panel.w - 24.0, "{text}: {width}");
            }
            let long_hint = "FIRE: Left Shift / Right Shift / Space / Enter";
            let fitted =
                ui::fit_label(long_hint, layout.panel.w - 24.0, metrics.glyph_advance()).unwrap();
            assert!(fitted.width <= layout.panel.w - 24.0);
            assert_eq!(
                metrics.measure_line(fitted.prefix).unwrap().width
                    + metrics.measure_line(fitted.suffix).unwrap().width,
                fitted.width
            );
        }
    }

    #[test]
    fn trajectory_preview_stops_at_real_tower_geometry_and_reuses_storage() {
        let mut game = Range::new();
        game.aim_at(vec3(0., 1.8, -4.));
        game.refresh_preview(false);
        let hit = game
            .preview_hit
            .expect("arc should hit the front of the tower");
        assert!(hit.z > -3.2 && hit.z < -2.8, "{hit:?}");
        assert_eq!(game.preview.last(), Some(&hit));
        let capacity = game.preview.capacity();
        let old = game.preview.clone();
        game.refresh_preview(false);
        assert_eq!(game.preview, old);
        game.yaw += 0.4;
        game.refresh_preview(false);
        assert_ne!(game.preview, old);
        assert_eq!(game.preview.capacity(), capacity);
    }

    fn tick(game: &mut Range, seconds: f32) {
        for _ in 0..(seconds / DT).round() as usize {
            game.step(DT);
        }
    }

    #[test]
    fn stack_stays_upright_without_projectiles() {
        let mut game = Range::new();
        tick(&mut game, 8.0);
        assert_eq!(game.score(), 0);
        assert_eq!(game.phase, Phase::Aiming);
    }

    #[test]
    fn ammunition_cooldown_ccd_and_loss_are_real() {
        let mut game = Range::new();
        game.yaw = 0.5;
        assert!(game.fire());
        assert!(!game.fire());
        assert!(game.physics.bodies[game.shots[0].handle].is_ccd_enabled());
        while game.ammo > 0 {
            tick(&mut game, 0.9);
            assert!(game.fire());
        }
        tick(&mut game, 6.2);
        assert_eq!(game.phase, Phase::Lost);
        assert_eq!(game.shots.len(), AMMO as usize);
        assert!(!game.fire());
    }

    #[test]
    fn autoplay_wins_through_ballistic_contacts() {
        for fps in [30, 144] {
            let mut foundry = Foundry::new(true);
            let mut fired = 0;
            let mut destroyed = 0;
            let mut outcomes = 0;
            for _ in 0..30 * fps {
                let events = foundry.advance(1.0 / f64::from(fps), true);
                fired += usize::from(events.fired);
                destroyed += events.destroyed;
                outcomes += usize::from(events.outcome.is_some());
                if foundry.session.state() == SessionState::Won {
                    break;
                }
            }
            let game = &foundry.range;
            assert_eq!(game.score(), GOAL, "{fps} FPS, ammo {}", game.ammo);
            assert_eq!(game.phase, Phase::Won);
            assert_eq!(foundry.session.state(), SessionState::Won);
            assert!(game.ammo > 0);
            assert!(fired > 0);
            assert_eq!(fired, game.shots.len());
            assert_eq!(destroyed, GOAL);
            assert_eq!(outcomes, 1);
        }
    }

    #[test]
    fn shot_input_survives_zero_step_frame_and_is_consumed_once_during_catchup() {
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.5;
        foundry.queue_fire();
        assert!(!foundry.advance(0.001, false).fired);
        assert!(foundry.pending_fire);
        assert_eq!(foundry.range.ammo, AMMO);
        assert!(foundry.advance(0.05, false).fired);
        assert!(!foundry.pending_fire);
        assert_eq!(foundry.range.ammo, AMMO - 1);
        for _ in 0..20 {
            assert!(!foundry.advance(0.1, false).fired);
        }
        assert_eq!(foundry.range.shots.len(), 1);
    }

    #[test]
    fn ready_session_cannot_spend_ammo_or_accumulate_physics_time() {
        let mut foundry = Foundry::new(false);
        foundry.queue_fire();
        let events = foundry.advance(50.0, false);
        assert!(!events.fired);
        assert_eq!(foundry.range.elapsed, 0.0);
        assert_eq!(foundry.range.ammo, AMMO);
        assert_eq!(foundry.session.tick(), 0);
        assert!(foundry.session.start());
        assert!(!foundry.advance(1.0 / 120.0, false).fired);
        assert_eq!(foundry.session.tick(), 1);
        foundry.queue_fire();
        assert!(foundry.advance(1.0 / 120.0, false).fired);
    }

    #[test]
    fn rejected_reload_edge_does_not_fire_later_or_refresh_feedback() {
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.5;
        foundry.queue_fire();
        assert!(foundry.advance(1.0 / 120.0, false).fired);
        let shot_feedback = foundry.feedback.shot;
        foundry.queue_fire();
        assert!(!foundry.pending_fire);
        assert_eq!(foundry.feedback.shot, shot_feedback);
        for _ in 0..12 {
            assert!(!foundry.advance(0.1, false).fired);
        }
        assert_eq!(foundry.range.ammo, AMMO - 1);
        assert_eq!(foundry.feedback.shot, 0.0);
        foundry.queue_fire();
        assert!(foundry.advance(1.0 / 120.0, false).fired);
    }

    #[test]
    fn pause_clears_buffered_input_and_freezes_reload_feedback_and_physics() {
        let mut foundry = Foundry::new(true);
        foundry.queue_fire();
        foundry.advance(0.001, false);
        foundry.set_paused(true);
        foundry.queue_fire();
        assert!(!foundry.pending_fire);
        foundry.advance(10.0, false);
        assert_eq!(foundry.range.elapsed, 0.0);
        foundry.set_paused(false);
        assert!(!foundry.advance(1.0 / 120.0, false).fired);
        assert_eq!(foundry.session.tick(), 1);
        foundry.queue_fire();
        foundry.advance(1.0 / 120.0, false);
        let before = (
            foundry.range.elapsed,
            foundry.range.cooldown,
            foundry.feedback.shot,
        );
        let handle = foundry.range.shots[0].handle;
        let position = foundry.range.physics.bodies[handle].translation();
        let tick = foundry.session.tick();
        foundry.set_paused(true);
        let events = foundry.advance(60.0, true);
        assert_eq!(events.alpha, 1.0);
        assert!(!events.fired);
        assert_eq!(
            before,
            (
                foundry.range.elapsed,
                foundry.range.cooldown,
                foundry.feedback.shot
            )
        );
        assert_eq!(position, foundry.range.physics.bodies[handle].translation());
        assert_eq!(tick, foundry.session.tick());
    }

    #[test]
    fn rebuild_restores_resources_and_drops_input_clock_and_feedback_debt() {
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.4;
        foundry.range.elevation = 0.6;
        foundry.queue_fire();
        foundry.advance(0.05, false);
        assert_eq!(foundry.range.physics.bodies.len(), 14);
        for _ in 0..9 {
            foundry.advance(0.1, false);
        }
        foundry.range.blocks[3].down = true;
        foundry.feedback.destruction = 1.0;
        foundry.feedback.knocked_down = 1;
        foundry.queue_fire();
        assert!(foundry.pending_fire);
        foundry.rebuild();
        let game = &foundry.range;
        assert_eq!(foundry.session.state(), SessionState::Playing);
        assert_eq!(foundry.session.tick(), 0);
        assert_eq!(game.ammo, AMMO);
        assert_eq!(game.score(), 0);
        assert_eq!(game.phase, Phase::Aiming);
        assert!(game.shots.is_empty());
        assert_eq!(game.physics.bodies.len(), 13);
        assert_eq!(game.physics.colliders.len(), 13);
        assert_eq!(game.yaw, 0.0);
        assert_eq!(game.elevation, 0.13);
        assert_eq!(
            (game.elapsed, game.cooldown, game.since_shot),
            (0.0, 0.0, 0.0)
        );
        assert!(game.preview.is_empty());
        assert!(game.preview_hit.is_none());
        assert!(!foundry.pending_fire);
        assert_eq!(
            (foundry.feedback.shot, foundry.feedback.destruction),
            (0.0, 0.0)
        );
        assert_eq!(foundry.feedback.knocked_down, 0);
        assert!(foundry.feedback.rebuilt > 0.0);
        assert!(!foundry.advance(0.001, false).fired);
        assert_eq!(foundry.session.tick(), 0);
        assert_eq!(foundry.range.elapsed, 0.0);
        foundry.set_paused(true);
        foundry.rebuild();
        assert_eq!(foundry.session.state(), SessionState::Playing);
        assert_eq!(foundry.range.ammo, AMMO);
    }

    #[test]
    fn last_shot_gets_full_settling_time_before_loss() {
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.5;
        while foundry.range.ammo > 0 {
            foundry.queue_fire();
            foundry.advance(1.0 / 120.0, false);
            if foundry.range.ammo > 0 {
                for _ in 0..9 {
                    foundry.advance(0.1, false);
                }
            }
        }
        for _ in 0..59 {
            foundry.advance(0.1, false);
        }
        assert_eq!(foundry.session.state(), SessionState::Playing);
        assert_eq!(foundry.range.phase, Phase::Aiming);
        let events = foundry.advance(0.1, false);
        assert_eq!(events.outcome, Some(Phase::Lost));
        assert_eq!(foundry.session.state(), SessionState::Lost);
        assert!(!foundry.range.fire());
    }

    #[test]
    fn terminal_result_is_emitted_once_and_stops_remaining_catchup_steps() {
        for won in [true, false] {
            let mut foundry = Foundry::new(true);
            if won {
                for block in &mut foundry.range.blocks {
                    block.down = block.target;
                }
            } else {
                foundry.range.ammo = 0;
                foundry.range.since_shot = SETTLE;
            }
            let phase = if won { Phase::Won } else { Phase::Lost };
            let events = foundry.advance(0.1, false);
            assert_eq!(events.outcome, Some(phase));
            assert_eq!(events.alpha, 1.0);
            assert_eq!(foundry.range.elapsed, DT);
            let handle = foundry.range.blocks[3].handle;
            let position = foundry.range.physics.bodies[handle].translation();
            for block in &mut foundry.range.blocks {
                block.down = block.target;
            }
            foundry.queue_fire();
            assert!(!foundry.advance(1.0, true).fired);
            assert!(foundry.advance(1.0, true).outcome.is_none());
            foundry.range.step(DT);
            assert_eq!(
                foundry.range.phase, phase,
                "a late knockdown cannot reverse a verdict"
            );
            assert_eq!(foundry.range.elapsed, DT);
            assert_eq!(position, foundry.range.physics.bodies[handle].translation());
            foundry.rebuild();
            assert_eq!(foundry.session.state(), SessionState::Playing);
            assert_eq!(foundry.range.ammo, AMMO);
            assert_eq!(foundry.range.score(), 0);
        }
    }

    fn started(autoplay: bool) -> Foundry {
        let mut foundry = Foundry::new(autoplay);
        // Autoplay sessions start themselves; interactive ones need Start.
        assert_eq!(foundry.start(), !autoplay);
        assert!(foundry.session.is_playing());
        foundry
    }

    #[test]
    fn bell_aim_rings_the_secret_bell_once_without_touching_physics() {
        let mut foundry = started(false);
        let bodies = foundry.range.physics.bodies.len();
        (foundry.range.yaw, foundry.range.elevation) = BELL_AIM;
        foundry.queue_fire();
        let mut callouts = Vec::new();
        for _ in 0..360 {
            callouts.extend(foundry.advance(1.0 / 120.0, false).callout);
        }
        let tally = &foundry.range.tally;
        assert!(tally.bell);
        assert_eq!(callouts, [Callout::Bell]);
        assert_eq!(tally.points, score::BELL_BONUS, "clean over the tower");
        assert_eq!(foundry.range.score(), 0);
        assert_eq!(
            foundry.range.physics.bodies.len(),
            bodies + 1,
            "sensor only"
        );
        assert_eq!(tally.misses, 0, "ringing the bell is not a miss");
        assert_eq!(foundry.juice.bubble.unwrap().mood, Mood::Bell);
        assert!(quips::rank(foundry.summary()).title.contains("BELL"));
    }

    #[test]
    fn autoplay_score_events_and_stars_match_across_frame_rates() {
        let mut reference = None;
        for fps in [30, 60, 144] {
            let mut foundry = started(true);
            let mut timbers = 0;
            for _ in 0..20 * fps {
                let events = foundry.advance(1.0 / f64::from(fps), true);
                timbers += usize::from(events.callout == Some(Callout::Timber));
                assert!(foundry.range.events.as_slice().is_empty(), "drained");
            }
            assert_eq!(foundry.session.state(), SessionState::Won);
            let tally = foundry.range.tally.clone();
            assert_eq!(usize::from(tally.shots), foundry.range.shots.len());
            assert_eq!(tally.targets, GOAL as u32);
            assert_eq!(tally.spare, foundry.range.ammo);
            assert!(timbers >= 1, "{fps}: a tower fell with a TIMBER call");
            let summary = foundry.summary();
            assert!(summary.won && summary.stars >= 2, "{summary:?}");
            // Targets pay at least 100 each, and spare shells pay a bonus.
            assert!(
                tally.points
                    >= 6 * score::TARGET_POINTS + u32::from(tally.spare) * score::SPARE_BONUS
            );
            if let Some(reference) = &reference {
                assert_eq!(&tally, reference, "{fps} Hz score diverged");
            }
            reference = Some(tally);
        }
    }

    #[test]
    fn a_wide_shot_is_a_miss_that_gus_comments_on_and_pause_freezes_juice() {
        let mut foundry = started(false);
        foundry.range.yaw = 0.5;
        foundry.queue_fire();
        assert!(foundry.advance(1.0 / 120.0, false).fired);
        for _ in 0..40 {
            foundry.advance(0.1, false);
        }
        assert_eq!(foundry.range.tally.misses, 1);
        assert_eq!(foundry.range.tally.points, 0);
        let bubble = foundry.juice.bubble.expect("Gus reacts to the miss");
        assert_eq!(bubble.mood, Mood::Miss);
        foundry.set_paused(true);
        let age = bubble.age;
        let time = foundry.juice.time;
        foundry.advance(30.0, false);
        assert_eq!(foundry.juice.bubble.unwrap().age, age);
        assert_eq!(foundry.juice.time, time);
        // Rebuild resets the run's score but keeps Gus's line counters moving.
        foundry.rebuild();
        assert_eq!(foundry.range.tally, Tally::default());
        assert_eq!(foundry.juice.bubble.unwrap().mood, Mood::Retry);
    }

    #[test]
    fn camera_kick_is_presentation_only_and_keeps_eye_and_depth() {
        let base = RangeCamera::new(1280.0, 800.0);
        let shifted = RangeCamera::new(1280.0, 800.0).shifted(1280.0, 800.0, vec2(6.0, -4.0));
        assert_eq!(shifted.eye, base.eye);
        let a = base.project(1280.0, 800.0, PIVOT).unwrap();
        let b = shifted.project(1280.0, 800.0, PIVOT).unwrap();
        assert!((b - a - vec2(6.0, -4.0)).length() < 0.01, "{a:?} -> {b:?}");
        let clip = |camera: &RangeCamera| camera.matrix * PIVOT.extend(1.0);
        assert_eq!(clip(&base).z, clip(&shifted).z);
        assert_eq!(clip(&base).w, clip(&shifted).w);
        let same = RangeCamera::new(1280.0, 800.0).shifted(1280.0, 800.0, Vec2::ZERO);
        assert_eq!(same.matrix, base.matrix);
    }

    #[test]
    fn reticle_identifies_targets_and_supports_under_the_preview() {
        let mut game = Range::new();
        let support = game.blocks[5].home + vec3(0.0, 0.0, 0.96);
        assert!(!game.block_at(support).unwrap().target);
        let target = game.blocks[6].home + vec3(0.0, 0.1, 0.96);
        assert!(game.block_at(target).unwrap().target);
        assert!(game.block_at(vec3(0.0, 0.0, 10.0)).is_none());
        game.aim_at(vec3(0.0, 1.8, -4.0));
        game.refresh_preview(false);
        let hit = game.preview_hit.unwrap();
        assert!(
            !game.block_at(hit).unwrap().target,
            "autoplay aims at supports"
        );
    }
}
