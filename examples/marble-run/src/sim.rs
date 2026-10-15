use ferravex_physics::d3::{
    ColliderBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle, Vector,
};
use ferravex_render::prelude::{Quat, Vec2, Vec3, vec2, vec3};

pub const RADIUS: f32 = 0.55;
pub const SPAWN: Vec3 = vec3(0., 1.05, 2.0);
pub const CHECKPOINTS: [Vec3; 2] = [vec3(10., 1.05, -12.), vec3(-3., 1.05, -25.)];
pub const FINISH: Vec3 = vec3(-3., 0.9, -39.);
pub const GEMS: [Vec3; 6] = [
    vec3(0., 1.0, -5.),
    vec3(5., 1.0, -12.),
    vec3(10., 1.0, -18.),
    vec3(4., 1.0, -25.),
    vec3(-3., 1.0, -30.),
    vec3(-3., 1.0, -36.),
];

#[derive(Clone, Copy)]
pub struct Platform {
    pub center: Vec3,
    pub size: Vec3,
}
pub const PLATFORMS: [Platform; 11] = [
    Platform {
        center: vec3(0., 0., 2.),
        size: vec3(6., 0.6, 6.),
    },
    Platform {
        center: vec3(0., 0., -5.25),
        size: vec3(3.4, 0.6, 8.5),
    },
    Platform {
        center: vec3(0., 0., -12.),
        size: vec3(5., 0.6, 5.),
    },
    Platform {
        center: vec3(4.9, 0., -12.),
        size: vec3(4.8, 0.6, 3.4),
    },
    Platform {
        center: vec3(10., 0., -12.),
        size: vec3(5.4, 0.6, 5.4),
    },
    Platform {
        center: vec3(10., 0., -18.6),
        size: vec3(3.4, 0.6, 7.8),
    },
    Platform {
        center: vec3(10., 0., -25.),
        size: vec3(5., 0.6, 5.),
    },
    Platform {
        center: vec3(3.6, 0., -25.),
        size: vec3(7.8, 0.6, 3.4),
    },
    Platform {
        center: vec3(-3., 0., -25.),
        size: vec3(5.4, 0.6, 5.4),
    },
    Platform {
        center: vec3(-3., 0., -31.85),
        size: vec3(3., 0.6, 8.3),
    },
    Platform {
        center: vec3(-3., 0., -39.),
        size: vec3(6., 0.6, 6.),
    },
];
pub const OBSTACLES: [Vec3; 8] = [
    vec3(-1.2, 0.95, -3.),
    vec3(1.2, 0.95, -7.),
    vec3(3., 0.95, -13.2),
    vec3(7., 0.95, -10.8),
    vec3(8.8, 0.95, -16.),
    vec3(11.2, 0.95, -21.),
    vec3(6., 0.95, -26.2),
    vec3(1., 0.95, -23.8),
];
// The autopilot supplies exactly the same bounded analog roll actions as a player.
const ROUTE: [Vec2; 15] = [
    vec2(0., -5.),
    vec2(0., -12.),
    vec2(5., -12.),
    vec2(10., -12.),
    vec2(10., -18.),
    vec2(10., -25.),
    vec2(4., -25.),
    vec2(-3., -25.),
    vec2(-3., -30.),
    vec2(-3., -33.),
    vec2(-3., -36.),
    vec2(-3., -39.),
    vec2(-3., -39.),
    vec2(-3., -39.),
    vec2(-3., -39.),
];

#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub roll: Vec2,
    pub brake: bool,
}

pub struct Run {
    pub physics: PhysicsWorld,
    pub marble: RigidBodyHandle,
    pub previous_position: Vec3,
    pub previous_rotation: Quat,
    pub collected: [bool; 6],
    pub checkpoint: usize,
    pub elapsed: f32,
    pub falls: u32,
    pub finished: bool,
    pub notice: f32,
    pub recovered: bool,
    route_index: usize,
}
impl Run {
    pub fn new() -> Self {
        let mut physics = PhysicsWorld::new([0., -16., 0.]);
        for p in PLATFORMS {
            physics.raw.insert(
                RigidBodyBuilder::fixed().translation(Vector::from_array(p.center.to_array())),
                ColliderBuilder::cuboid(p.size.x * 0.5, p.size.y * 0.5, p.size.z * 0.5)
                    .friction(0.9),
            );
        }
        for p in OBSTACLES {
            physics.raw.insert(
                RigidBodyBuilder::fixed().translation(Vector::from_array(p.to_array())),
                ColliderBuilder::cylinder(0.65, 0.38)
                    .friction(0.6)
                    .restitution(0.15),
            );
        }
        let (marble, _) = physics.raw.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::from_array(SPAWN.to_array()))
                .linear_damping(0.28)
                .angular_damping(0.22)
                .ccd_enabled(true),
            ColliderBuilder::ball(RADIUS)
                .density(2.)
                .friction(1.0)
                .restitution(0.08),
        );
        Self {
            physics,
            marble,
            previous_position: SPAWN,
            previous_rotation: Quat::IDENTITY,
            collected: [false; 6],
            checkpoint: 0,
            elapsed: 0.,
            falls: 0,
            finished: false,
            notice: 0.,
            recovered: false,
            route_index: 0,
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
            SPAWN
        } else {
            CHECKPOINTS[self.checkpoint - 1]
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
        self.route_index = match self.checkpoint {
            0 => 0,
            1 => 4,
            _ => 8,
        };
    }
    pub fn step(&mut self, input: Controls, dt: f32) {
        if self.finished {
            return;
        }
        self.elapsed += dt;
        self.notice = (self.notice - dt).max(0.);
        self.previous_position = self.position();
        self.previous_rotation = self.rotation();
        let body = &mut self.physics.bodies[self.marble];
        let velocity = body.linvel();
        let horizontal = Vec2::new(velocity.x, velocity.z);
        let desired = input.roll.clamp_length_max(1.) * if input.brake { 1.0 } else { 4.5 };
        let delta =
            (desired - horizontal).clamp_length_max(1.8) * if input.brake { 8.0 } else { 4.0 };
        let mass = body.mass();
        body.apply_impulse(Vector::new(delta.x, 0., delta.y) * (mass * dt), true);
        // Torque makes the ball truly roll. Surface friction and rigid-body contacts
        // decide its angular motion; render rotation comes from the rigid body.
        body.apply_torque_impulse(
            Vector::new(delta.y, 0., -delta.x) * (mass * RADIUS * 0.35 * dt),
            true,
        );
        if input.brake {
            body.apply_torque_impulse(-body.angvel() * (mass * RADIUS * RADIUS * dt * 1.5), true);
        }
        self.physics.step(dt);
        let p = self.position();
        if p.y < -7. {
            self.recover();
            return;
        }
        for (i, gem) in GEMS.iter().enumerate() {
            if p.distance(*gem) < 0.95 {
                self.collected[i] = true;
            }
        }
        for (i, cp) in CHECKPOINTS.iter().enumerate() {
            if i >= self.checkpoint && p.distance(*cp) < 1.5 {
                self.checkpoint = i + 1;
                self.recovered = false;
                self.notice = 2.;
            }
        }
        if self.gems() == GEMS.len() && p.distance(FINISH) < 1.25 {
            self.finished = true;
        }
    }
    pub fn autopilot(&mut self) -> Controls {
        let p = self.position();
        let flat = Vec2::new(p.x, p.z);
        if flat.distance(ROUTE[self.route_index]) < 0.5 && self.route_index + 1 < ROUTE.len() {
            self.route_index += 1;
        }
        let delta = ROUTE[self.route_index] - flat;
        let v = self.physics.bodies[self.marble].linvel();
        let desired_velocity = delta.clamp_length_max(1.8) * 1.6;
        Controls {
            roll: ((desired_velocity - Vec2::new(v.x, v.z) * 0.2) / 4.5).clamp_length_max(1.),
            brake: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gravity_and_contacts_keep_marble_on_platform() {
        let mut game = Run::new();
        for _ in 0..240 {
            game.step(Controls::default(), 1. / 120.);
        }
        assert!((game.position().y - (0.3 + RADIUS)).abs() < 0.03);
        assert_eq!(game.falls, 0);
    }
    #[test]
    fn fall_recovers_at_checkpoint_without_losing_gems() {
        let mut game = Run::new();
        game.physics.bodies[game.marble]
            .set_translation(Vector::from_array(CHECKPOINTS[0].to_array()), true);
        game.step(Controls::default(), 1. / 120.);
        assert_eq!(game.checkpoint, 1);
        game.collected[0] = true;
        game.physics.bodies[game.marble].set_translation(Vector::new(30., -9., 0.), true);
        game.step(Controls::default(), 1. / 120.);
        assert_eq!(game.falls, 1);
        assert!(game.position().distance(CHECKPOINTS[0]) < 0.01);
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
            .set_translation(Vector::from_array(FINISH.to_array()), true);
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
