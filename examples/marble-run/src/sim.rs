use ferravex_physics::d3::{
    ColliderBuilder, PhysicsWorld, QueryOptions, RigidBodyBuilder, RigidBodyHandle, Vector,
};
use ferravex_physics::{ColliderHandle, ContactEvent};
use ferravex_render::prelude::{Quat, Vec2, Vec3};
use ferravex_render::{vec2, vec3};

use crate::level::Level;
use std::sync::Arc;
#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub roll: Vec2,
    pub brake: bool,
}

/// Airborne seconds that count as a jump worth celebrating on landing.
pub const AIR_MIN: f32 = 0.3;
/// Seconds teetering past an edge (touching, centre unsupported) before a save
/// back onto solid ground counts as a clutch.
pub const CLUTCH_MIN: f32 = 0.08;

/// Airtime and edge-teetering bookkeeping, separated from physics queries so
/// the landing and clutch rules are directly testable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Footing {
    pub airtime: f32,
    pub edge_time: f32,
}

impl Footing {
    /// Feed one tick: `touching` any solid, `supported` directly under the
    /// centre. Returns (landing airtime, clutch save).
    pub fn update(&mut self, touching: bool, supported: bool, dt: f32) -> (Option<f32>, bool) {
        if !touching {
            self.airtime += dt;
            self.edge_time = 0.;
            return (None, false);
        }
        let landed = (self.airtime >= AIR_MIN).then_some(self.airtime);
        self.airtime = 0.;
        if supported {
            let clutch = self.edge_time >= CLUTCH_MIN;
            self.edge_time = 0.;
            (landed, clutch)
        } else {
            self.edge_time += dt;
            (landed, false)
        }
    }
}

/// Accepted moments from the latest physics tick. Cleared at the start of every
/// [`Run::step`]; presentation reads them after the tick and never writes back.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Events {
    /// Newly collected crystal index (the last one if several in one tick).
    pub gem: Option<usize>,
    /// The marble left the course and was restored to its checkpoint.
    pub fell: bool,
    /// A rotating sweeper started touching the marble: (marble position, speed change).
    pub bonk: Option<(Vec3, f32)>,
    /// Landed after at least [`AIR_MIN`] seconds airborne.
    pub landed: Option<f32>,
    /// Rolled back onto solid footing after teetering past an edge.
    pub clutch: bool,
    /// The course's hidden moonstone was collected.
    pub secret: bool,
}

/// Bounded per-run style counters for the results card and rank title.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub bonks: u32,
    pub clutches: u32,
    pub best_air: f32,
    pub top_speed: f32,
}

pub struct Run {
    pub level: Arc<Level>,
    pub physics: PhysicsWorld,
    pub marble: RigidBodyHandle,
    pub previous_position: Vec3,
    pub previous_rotation: Quat,
    pub collected: Vec<bool>,
    pub checkpoint: usize,
    /// First accepted visit only; skipped checkpoints have no invented split.
    pub splits: [Option<u64>; crate::level::MAX_CHECKPOINTS],
    pub elapsed: f32,
    pub falls: u32,
    pub finished: bool,
    pub notice: f32,
    pub recovered: bool,
    pub sweepers: Vec<RigidBodyHandle>,
    route_index: usize,
    marble_collider: ColliderHandle,
    sweeper_colliders: Vec<ColliderHandle>,
    pub events: Events,
    pub stats: Stats,
    /// Off-route bonus pickup derived from the authored geometry; never required.
    pub secret: Option<Vec3>,
    pub secret_found: bool,
    /// Simulation time of the latest sweeper bonk, used to explain a fall.
    pub bonked_at: Option<f32>,
    footing: Footing,
}
impl Run {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_level(Arc::new(Level::parse(crate::level::SOURCE).unwrap()))
    }
    pub fn with_level(level: Arc<Level>) -> Self {
        let mut physics = PhysicsWorld::new([0., level.tuning.gravity, 0.]);
        for p in &level.platforms {
            physics.raw.insert(
                RigidBodyBuilder::fixed().translation(Vector::from_array(p.center.to_array())),
                ColliderBuilder::cuboid(p.size.x * 0.5, p.size.y * 0.5, p.size.z * 0.5)
                    .rotation(p.rotation)
                    .material(ferravex_physics::PhysicsMaterial {
                        static_friction: p.friction * 1.1,
                        sliding_friction: p.friction,
                        rolling_resistance: 0.002,
                        ..Default::default()
                    }),
            );
        }
        let (sweepers, sweeper_colliders) = level
            .sweepers
            .iter()
            .map(|p| {
                physics.raw.insert(
                    RigidBodyBuilder::kinematic_position_based()
                        .translation(Vector::from_array(p.position.to_array())),
                    ColliderBuilder::cuboid(p.size.x * 0.5, p.size.y * 0.5, p.size.z * 0.5)
                        .rotation(p.rotation(0.))
                        .friction(0.7)
                        .restitution(0.12),
                )
            })
            .unzip();
        for p in &level.obstacles {
            physics.raw.insert(
                RigidBodyBuilder::fixed().translation(Vector::from_array(p.position.to_array())),
                ColliderBuilder::cylinder(p.height * 0.5, p.radius)
                    .rotation(p.rotation)
                    .friction(0.6)
                    .restitution(0.15),
            );
        }
        let (marble, marble_collider) = physics.raw.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::from_array(level.spawn.to_array()))
                .linear_damping(level.tuning.linear_damping)
                .angular_damping(level.tuning.angular_damping)
                .ccd_enabled(true),
            ColliderBuilder::ball(level.tuning.radius)
                .density(level.tuning.density)
                .friction(level.tuning.friction)
                .restitution(level.tuning.restitution),
        );
        Self {
            physics,
            marble,
            previous_position: level.spawn,
            previous_rotation: Quat::IDENTITY,
            collected: vec![false; level.gems.len()],
            checkpoint: 0,
            splits: [None; crate::level::MAX_CHECKPOINTS],
            elapsed: 0.,
            falls: 0,
            finished: false,
            notice: 0.,
            recovered: false,
            sweepers,
            route_index: level.spawn_route,
            marble_collider,
            sweeper_colliders,
            events: Events::default(),
            stats: Stats::default(),
            secret: secret_spot(&level),
            secret_found: false,
            bonked_at: None,
            footing: Footing::default(),
            level,
        }
    }
    pub fn position(&self) -> Vec3 {
        Vec3::from_array(self.physics.bodies[self.marble].translation().to_array())
    }
    pub fn rotation(&self) -> Quat {
        Quat::from_array(self.physics.bodies[self.marble].rotation().to_array())
    }
    pub fn gems(&self) -> usize {
        self.collected.iter().filter(|&&v| v).count()
    }
    pub fn recover(&mut self) {
        let p = if self.checkpoint == 0 {
            self.level.spawn
        } else {
            self.level.checkpoints[self.checkpoint - 1].position
        };
        let body = &mut self.physics.bodies[self.marble];
        body.set_translation(Vector::from_array(p.to_array()), true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
        self.previous_position = p;
        self.previous_rotation = self.rotation();
        self.falls += 1;
        self.recovered = true;
        self.notice = 2.;
        self.footing = Footing::default();
        self.route_index = if self.checkpoint == 0 {
            self.level.spawn_route
        } else {
            self.level.checkpoints[self.checkpoint - 1].route_index
        };
    }
    pub fn step(&mut self, input: Controls, dt: f32) {
        if self.finished {
            return;
        }
        self.events = Events::default();
        self.elapsed += dt;
        for (sweeper, &handle) in self.level.sweepers.iter().zip(&self.sweepers) {
            // Rotation belongs to the body so the solver transfers its velocity.
            // The collider's authored phase is local; advance only the time term here.
            self.physics.bodies[handle]
                .set_next_kinematic_rotation(Quat::from_rotation_y(sweeper.speed * self.elapsed));
        }
        self.notice = (self.notice - dt).max(0.);
        self.previous_position = self.position();
        self.previous_rotation = self.rotation();
        let body = &mut self.physics.bodies[self.marble];
        let velocity = body.linvel();
        let horizontal = Vec2::new(velocity.x, velocity.z);
        let desired = input.roll.clamp_length_max(1.)
            * if input.brake {
                1.0
            } else {
                self.level.tuning.speed
            };
        let delta = if input.brake {
            // Deliberate braking is strong enough to catch a downhill hairpin.
            ((desired - horizontal) * 12.).clamp_length_max(24.)
        } else if input.roll.length_squared() > 0.0001 {
            (desired - horizontal).clamp_length_max(1.8) * 4.
        } else {
            // Releasing the stick coasts; surface friction/damping decide the roll.
            Vec2::ZERO
        };
        let mass = body.mass();
        body.apply_impulse(Vector::new(delta.x, 0., delta.y) * (mass * dt), true);
        // Torque makes the ball truly roll. Surface friction and rigid-body contacts
        // decide its angular motion; render rotation comes from the rigid body.
        body.apply_torque_impulse(
            Vector::new(delta.y, 0., -delta.x) * (mass * self.level.tuning.radius * 0.35 * dt),
            true,
        );
        if input.brake {
            body.apply_torque_impulse(
                -body.angvel()
                    * (mass * self.level.tuning.radius * self.level.tuning.radius * dt * 6.),
                true,
            );
        }
        let before = Vec3::from_array(velocity.to_array());
        self.physics.step(dt);
        let p = self.position();
        if p.y < self.level.tuning.fall_y {
            self.recover();
            self.events.fell = true;
            return;
        }
        self.observe_moments(p, before, dt);
        for (i, gem) in self.level.gems.iter().enumerate() {
            if !self.collected[i] && p.distance(*gem) < self.level.tuning.gem_radius {
                self.collected[i] = true;
                self.events.gem = Some(i);
            }
        }
        if !self.secret_found
            && self
                .secret
                .is_some_and(|s| p.distance(s) < self.level.tuning.gem_radius)
        {
            self.secret_found = true;
            self.events.secret = true;
        }
        for (i, cp) in self.level.checkpoints.iter().enumerate() {
            if i >= self.checkpoint && p.distance(cp.position) < self.level.tuning.checkpoint_radius
            {
                self.checkpoint = i + 1;
                self.splits[i] = ferravex_engine::persistence::run_micros(self.elapsed);
                self.recovered = false;
                self.notice = 2.;
            }
        }
        if self.gems() == self.level.gems.len()
            && p.distance(self.level.finish) < self.level.tuning.finish_radius
        {
            self.finished = true;
        }
    }
    /// Read-only contact/ray queries after the physics step; never changes motion.
    fn observe_moments(&mut self, p: Vec3, before: Vec3, dt: f32) {
        let marble = self.marble_collider;
        let velocity = Vec3::from_array(self.physics.bodies[self.marble].linvel().to_array());
        let speed = vec2(velocity.x, velocity.z).length();
        self.stats.top_speed = self.stats.top_speed.max(speed);
        for event in self.physics.contact_events() {
            let ContactEvent::Started(a, b) = *event else {
                continue;
            };
            let other = if a == marble {
                b
            } else if b == marble {
                a
            } else {
                continue;
            };
            let change = (velocity - before).length();
            if self.sweeper_colliders.contains(&other) && change > 0.6 {
                self.events.bonk = Some((p, change));
                self.stats.bonks += 1;
                self.bonked_at = Some(self.elapsed);
            }
        }
        let touching = self
            .physics
            .contacts()
            .any(|c| !c.sensor && (c.collider1 == marble || c.collider2 == marble));
        let supported = touching
            && self
                .physics
                .raycast_with_options(
                    p.to_array(),
                    [0., -1., 0.],
                    QueryOptions::new(self.level.tuning.radius + 0.3).ignore_body(self.marble),
                )
                .is_some();
        let (landed, clutch) = self.footing.update(touching, supported, dt);
        if let Some(air) = landed {
            self.events.landed = Some(air);
            self.stats.best_air = self.stats.best_air.max(air);
        }
        if clutch {
            self.events.clutch = true;
            self.stats.clutches += 1;
        }
    }
    pub fn autopilot(&mut self) -> Controls {
        let p = self.position();
        let flat = Vec2::new(p.x, p.z);
        if flat.distance(self.level.route[self.route_index]) < 0.5
            && self.route_index + 1 < self.level.route.len()
        {
            self.route_index += 1;
        }
        let delta = self.level.route[self.route_index] - flat;
        let v = self.physics.bodies[self.marble].linvel();
        let desired_velocity = delta.clamp_length_max(1.8) * 1.6;
        Controls {
            roll: ((desired_velocity - Vec2::new(v.x, v.z) * 0.2) / self.level.tuning.speed)
                .clamp_length_max(1.),
            brake: false,
        }
    }
}

fn flat(a: Vec3, b: Vec3) -> f32 {
    vec2(a.x - b.x, a.z - b.z).length()
}

fn route_distance(route: &[Vec2], p: Vec2) -> f32 {
    if route.len() == 1 {
        return route[0].distance(p);
    }
    route
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0., 1.);
            (a + ab * t).distance(p)
        })
        .fold(f32::INFINITY, f32::min)
}

/// Minimum distance from the racing line, so the moonstone is a real detour.
const SECRET_DETOUR: f32 = 2.;

/// The hidden moonstone sits on the flat platform corner farthest from the
/// autoplay route, clear of hazards and other markers. Pure geometry, so it
/// follows live content edits without new authored fields or record changes.
pub fn secret_spot(level: &Level) -> Option<Vec3> {
    let radius = level.tuning.radius;
    let markers: Vec<Vec3> = level
        .gems
        .iter()
        .copied()
        .chain(level.checkpoints.iter().map(|c| c.position))
        .chain([level.finish, level.spawn])
        .collect();
    let mut best: Option<(f32, Vec3)> = None;
    for p in &level.platforms {
        if p.size.x < 1.8 || p.size.z < 1.8 || (p.rotation * Vec3::Y).y < 0.97 {
            continue;
        }
        for (sx, sz) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            let spot = p.at(vec3(
                sx * (p.size.x * 0.5 - 0.75),
                p.size.y * 0.5 + radius + 0.1,
                sz * (p.size.z * 0.5 - 0.75),
            ));
            let blocked = level
                .obstacles
                .iter()
                .any(|o| flat(o.position, spot) < o.radius + radius + 0.5)
                || level
                    .sweepers
                    .iter()
                    .any(|s| flat(s.position, spot) < s.size.x.max(s.size.z) * 0.5 + 1.)
                || markers.iter().any(|m| flat(*m, spot) < 3.5);
            if blocked {
                continue;
            }
            // The racing line starts at the spawn, before the first waypoint.
            let start = vec2(level.spawn.x, level.spawn.z);
            let distance = route_distance(&level.route, vec2(spot.x, spot.z)).min(route_distance(
                &[start, level.route[0]],
                vec2(spot.x, spot.z),
            ));
            if distance >= SECRET_DETOUR && best.is_none_or(|(d, _)| distance > d) {
                best = Some((distance, spot));
            }
        }
    }
    best.map(|(_, spot)| spot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releasing_coasts_and_deliberate_braking_stops_the_marble() {
        let mut coast = Run::new();
        let mut brake = Run::new();
        for _ in 0..90 {
            coast.step(Controls::default(), 1. / 120.);
            brake.step(Controls::default(), 1. / 120.);
        }
        for game in [&mut coast, &mut brake] {
            game.physics.bodies[game.marble].set_linvel(Vector::new(3., 0., 0.), true);
        }
        for _ in 0..25 {
            coast.step(Controls::default(), 1. / 120.);
            brake.step(
                Controls {
                    brake: true,
                    ..Default::default()
                },
                1. / 120.,
            );
        }
        let coast_speed = coast.physics.bodies[coast.marble].linvel().x.abs();
        let brake_speed = brake.physics.bodies[brake.marble].linvel().x.abs();
        assert!(
            brake_speed < coast_speed * 0.6,
            "coast={coast_speed}, brake={brake_speed}"
        );
        assert_eq!((coast.falls, brake.falls), (0, 0));
    }

    #[test]
    fn every_authored_course_is_finishable_using_only_ordinary_inputs() {
        for course in &crate::courses::COURSES {
            let mut game = Run::with_level(Arc::new(Level::parse(course.source).unwrap()));
            for _ in 0..120 * 180 {
                let input = game.autopilot();
                game.step(input, 1. / 120.);
                assert!(
                    game.position().is_finite(),
                    "{} produced invalid physics",
                    course.id
                );
                if game.finished {
                    break;
                }
            }
            assert!(
                game.finished,
                "{}: position={:?}, route={}, gems={}, falls={}",
                course.id,
                game.position(),
                game.route_index,
                game.gems(),
                game.falls
            );
            assert!(
                !game.secret_found,
                "{} autoplay must not stumble on the moonstone",
                course.id
            );
            eprintln!(
                "{} finished {:.1}s / {} falls / {:?} / secret {:?}",
                course.id, game.elapsed, game.falls, game.stats, game.secret
            );
        }
    }
    #[test]
    fn rotating_sweeper_transfers_motion_through_real_contacts() {
        let mut game = Run::with_level(Arc::new(
            Level::parse(crate::courses::COURSES[2].source).unwrap(),
        ));
        let pivot = game.level.sweepers[0].position;
        let start = pivot + Vec3::new(-1.5, 0., 0.9);
        game.physics.bodies[game.marble]
            .set_translation(Vector::from_array(start.to_array()), true);
        let mut displaced = false;
        for _ in 0..240 {
            game.step(
                Controls {
                    brake: true,
                    ..Default::default()
                },
                1. / 120.,
            );
            let p = game.position();
            displaced |= Vec2::new(p.x - start.x, p.z - start.z).length() > 0.35;
        }
        assert!(
            displaced,
            "the rotating bar must physically push the marble"
        );
    }
    #[test]
    fn every_checkpoint_is_a_safe_recovery_surface() {
        for course in &crate::courses::COURSES {
            let level = Arc::new(Level::parse(course.source).unwrap());
            for checkpoint in 1..=level.checkpoints.len() {
                let mut game = Run::with_level(level.clone());
                game.checkpoint = checkpoint;
                game.collected[0] = true;
                game.recover();
                let start = game.position();
                for _ in 0..240 {
                    game.step(Controls::default(), 1. / 120.);
                }
                assert_eq!(
                    game.falls, 1,
                    "{} checkpoint {} falls again without input",
                    course.id, checkpoint
                );
                assert!(
                    game.position().distance(start) < 0.3,
                    "recovery must settle safely"
                );
                assert!(game.collected[0]);
            }
        }
    }
    #[test]
    fn gravity_and_contacts_keep_marble_on_platform() {
        let mut game = Run::new();
        for _ in 0..240 {
            game.step(Controls::default(), 1. / 120.);
        }
        assert!((game.position().y - (0.3 + game.level.tuning.radius)).abs() < 0.03);
        assert_eq!(game.falls, 0);
    }
    #[test]
    fn fall_recovers_at_checkpoint_without_losing_gems() {
        let mut game = Run::new();
        game.physics.bodies[game.marble].set_translation(
            Vector::from_array(game.level.checkpoints[0].position.to_array()),
            true,
        );
        game.step(Controls::default(), 1. / 120.);
        assert_eq!(game.checkpoint, 1);
        game.collected[0] = true;
        game.physics.bodies[game.marble].set_translation(Vector::new(30., -9., 0.), true);
        game.step(Controls::default(), 1. / 120.);
        assert_eq!(game.falls, 1);
        assert!(game.position().distance(game.level.checkpoints[0].position) < 0.01);
        assert!(game.collected[0]);
    }
    #[test]
    fn ordinary_rolling_inputs_collect_all_gems_and_finish() {
        let mut game = Run::new();
        for _ in 0..120 * 90 {
            let input = game.autopilot();
            game.step(input, 1. / 120.);
            if game.finished {
                break;
            }
        }
        assert!(
            game.finished,
            "position={:?}, gems={}, route={}, falls={}",
            game.position(),
            game.gems(),
            game.route_index,
            game.falls
        );
        assert_eq!(game.gems(), 6);
        assert_eq!(game.checkpoint, 2);
        assert_eq!(game.falls, 0);
    }
    #[test]
    fn finish_requires_every_pickup() {
        let mut game = Run::new();
        game.physics.bodies[game.marble]
            .set_translation(Vector::from_array(game.level.finish.to_array()), true);
        game.step(Controls::default(), 1. / 120.);
        assert!(!game.finished);
    }
    #[test]
    fn brake_reduces_speed_and_rotation_is_physical() {
        let mut game = Run::new();
        for _ in 0..100 {
            game.step(
                Controls {
                    roll: vec2(0., -1.),
                    brake: false,
                },
                1. / 120.,
            );
        }
        let before = game.physics.bodies[game.marble].linvel().length();
        assert!(game.physics.bodies[game.marble].angvel().length() > 1.);
        for _ in 0..60 {
            game.step(
                Controls {
                    brake: true,
                    ..Controls::default()
                },
                1. / 120.,
            );
        }
        assert!(game.physics.bodies[game.marble].linvel().length() < before * 0.2);
    }

    #[test]
    fn accepted_checkpoint_splits_survive_recovery_and_skips_stay_unset() {
        let mut run = Run::new();
        run.physics.bodies[run.marble].set_translation(
            Vector::from_array(run.level.checkpoints[0].position.to_array()),
            true,
        );
        run.step(Controls::default(), 1. / 120.);
        assert_eq!(
            run.splits[0],
            ferravex_engine::persistence::run_micros(run.elapsed)
        );
        let splits = run.splits;
        for _ in 0..30 {
            run.step(Controls::default(), 1. / 120.);
        }
        assert_eq!(
            run.splits, splits,
            "standing on a gate must not replace its split"
        );
        run.recover();
        assert_eq!(run.splits, splits);
        let mut retry = Run::with_level(run.level.clone());
        assert!(retry.splits.iter().all(Option::is_none));
        retry.physics.bodies[retry.marble].set_translation(
            Vector::from_array(retry.level.checkpoints[1].position.to_array()),
            true,
        );
        retry.step(Controls::default(), 1. / 120.);
        assert_eq!(
            retry.checkpoint, 2,
            "preserve the existing non-sequential trigger rule"
        );
        assert_eq!(retry.splits[0], None);
        assert!(retry.splits[1].is_some());
    }

    #[test]
    fn footing_reports_landings_and_clutch_saves_only_after_their_thresholds() {
        let dt = 1. / 120.;
        let mut f = Footing::default();
        // A small hop is not a landing worth celebrating.
        for _ in 0..12 {
            assert_eq!(f.update(false, false, dt), (None, false));
        }
        assert_eq!(f.update(true, true, dt), (None, false));
        for _ in 0..48 {
            f.update(false, false, dt);
        }
        let (landed, clutch) = f.update(true, true, dt);
        assert!(landed.is_some_and(|t| (t - 0.4).abs() < 1e-4) && !clutch);
        // Brief edge contact is ordinary rolling; sustained teetering is a save.
        f.update(true, false, dt);
        assert_eq!(f.update(true, true, dt), (None, false));
        for _ in 0..12 {
            f.update(true, false, dt);
        }
        assert_eq!(f.update(true, true, dt), (None, true));
        // Teetering that ends in the air (a fall) never counts.
        for _ in 0..12 {
            f.update(true, false, dt);
        }
        f.update(false, false, dt);
        assert_eq!(f.update(true, true, dt), (None, false));
    }

    #[test]
    fn real_drops_report_one_landing_and_events_clear_each_tick() {
        let mut run = Run::new();
        for _ in 0..120 {
            run.step(Controls::default(), 1. / 120.);
        }
        assert_eq!(run.events, Events::default(), "resting yields no moments");
        let p = run.position() + Vec3::Y * 2.5;
        run.physics.bodies[run.marble].set_translation(Vector::from_array(p.to_array()), true);
        let mut landings = 0;
        for _ in 0..240 {
            run.step(Controls::default(), 1. / 120.);
            if let Some(air) = run.events.landed {
                landings += 1;
                assert!(air >= AIR_MIN);
            }
        }
        assert_eq!(landings, 1);
        assert!(run.stats.best_air >= AIR_MIN);
        assert_eq!(run.falls, 0);
    }

    #[test]
    fn sweeper_contact_reports_bonks_and_falls_are_flagged_as_void() {
        let mut game = Run::with_level(Arc::new(
            Level::parse(crate::courses::COURSES[2].source).unwrap(),
        ));
        let pivot = game.level.sweepers[0].position;
        let start = pivot + Vec3::new(-1.5, 0., 0.9);
        game.physics.bodies[game.marble]
            .set_translation(Vector::from_array(start.to_array()), true);
        let mut bonks = 0;
        for _ in 0..240 {
            game.step(
                Controls {
                    brake: true,
                    ..Default::default()
                },
                1. / 120.,
            );
            bonks += u32::from(game.events.bonk.is_some());
        }
        assert!(bonks > 0 && bonks == game.stats.bonks);
        assert!(game.bonked_at.is_some());
        game.physics.bodies[game.marble].set_translation(Vector::new(0., -30., 0.), true);
        game.step(Controls::default(), 1. / 120.);
        assert!(game.events.fell);
        game.step(Controls::default(), 1. / 120.);
        assert!(!game.events.fell, "events describe only the latest tick");
    }

    #[test]
    fn every_course_hides_a_safe_optional_moonstone_off_the_route() {
        for course in &crate::courses::COURSES {
            let level = Arc::new(Level::parse(course.source).unwrap());
            let secret = secret_spot(&level).expect("each authored course has a moonstone");
            let route = route_distance(&level.route, vec2(secret.x, secret.z));
            assert!(
                route >= SECRET_DETOUR,
                "{} moonstone sits on the racing line",
                course.id
            );
            let mut run = Run::with_level(level.clone());
            assert_eq!(run.secret, Some(secret));
            run.physics.bodies[run.marble]
                .set_translation(Vector::from_array(secret.to_array()), true);
            let mut found = 0;
            for _ in 0..240 {
                run.step(Controls::default(), 1. / 120.);
                found += u32::from(run.events.secret);
            }
            assert_eq!(found, 1, "{} moonstone collects exactly once", course.id);
            assert!(run.secret_found);
            assert_eq!(run.falls, 0, "{} moonstone spot is safe", course.id);
            assert!(
                !run.finished && run.gems() == 0,
                "the bonus is never required"
            );
        }
    }
}
