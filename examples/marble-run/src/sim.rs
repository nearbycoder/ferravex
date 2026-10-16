use ferravex_physics::d3::{
    ColliderBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle, Vector,
};
use ferravex_render::prelude::{Quat, Vec2, Vec3};
#[cfg(test)]
use ferravex_render::vec2;

use crate::level::Level;
use std::sync::Arc;
#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub roll: Vec2,
    pub brake: bool,
}

pub struct Run {
    pub level: Arc<Level>,
    pub physics: PhysicsWorld,
    pub marble: RigidBodyHandle,
    pub previous_position: Vec3,
    pub previous_rotation: Quat,
    pub collected: Vec<bool>,
    pub checkpoint: usize,
    pub elapsed: f32,
    pub falls: u32,
    pub finished: bool,
    pub notice: f32,
    pub recovered: bool,
    pub sweepers: Vec<RigidBodyHandle>,
    route_index: usize,
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
        let sweepers = level
            .sweepers
            .iter()
            .map(|p| {
                physics
                    .raw
                    .insert(
                        RigidBodyBuilder::kinematic_position_based()
                            .translation(Vector::from_array(p.position.to_array())),
                        ColliderBuilder::cuboid(p.size.x * 0.5, p.size.y * 0.5, p.size.z * 0.5)
                            .rotation(p.rotation(0.))
                            .friction(0.7)
                            .restitution(0.12),
                    )
                    .0
            })
            .collect();
        for p in &level.obstacles {
            physics.raw.insert(
                RigidBodyBuilder::fixed().translation(Vector::from_array(p.position.to_array())),
                ColliderBuilder::cylinder(p.height * 0.5, p.radius)
                    .rotation(p.rotation)
                    .friction(0.6)
                    .restitution(0.15),
            );
        }
        let (marble, _) = physics.raw.insert(
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
            elapsed: 0.,
            falls: 0,
            finished: false,
            notice: 0.,
            recovered: false,
            sweepers,
            route_index: level.spawn_route,
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
        self.physics.step(dt);
        let p = self.position();
        if p.y < self.level.tuning.fall_y {
            self.recover();
            return;
        }
        for (i, gem) in self.level.gems.iter().enumerate() {
            if p.distance(*gem) < self.level.tuning.gem_radius {
                self.collected[i] = true;
            }
        }
        for (i, cp) in self.level.checkpoints.iter().enumerate() {
            if i >= self.checkpoint && p.distance(cp.position) < self.level.tuning.checkpoint_radius
            {
                self.checkpoint = i + 1;
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
            eprintln!(
                "{} finished {:.1}s / {} falls",
                course.id, game.elapsed, game.falls
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
}
