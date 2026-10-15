//! A small pinball table driven by Ferravex's original fixed-step physics world.
use ferravex_physics::d2::{
    ColliderBuilder, ColliderHandle, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle, Rotation,
    Vector,
};

pub const BALL_RADIUS: f32 = 0.19;
pub const FLIPPER_LENGTH: f32 = 1.65;
pub const PIVOTS: [[f32; 2]; 2] = [[3.0, 13.2], [6.9, 13.2]];
pub const BUMPERS: [[f32; 2]; 3] = [[2.6, 4.1], [6.2, 4.1], [4.4, 6.6]];
/// Capsule rails, also used by the renderer so collisions match the visible table.
pub const RAILS: [([f32; 2], [f32; 2]); 13] = [
    ([0.6, 2.0], [0.6, 11.6]),
    ([0.6, 2.0], [1.8, 0.7]),
    ([1.8, 0.7], [8.1, 0.7]),
    ([8.1, 0.7], [9.7, 2.3]),
    ([9.7, 2.3], [9.7, 15.1]),
    ([8.8, 3.7], [8.8, 15.1]),
    ([8.8, 15.1], [9.7, 15.1]),
    ([0.6, 11.6], [3.0, 13.2]),
    ([8.8, 11.6], [6.9, 13.2]),
    ([1.2, 8.2], [2.7, 10.7]),
    ([8.1, 8.2], [6.6, 10.7]),
    ([2.7, 10.7], [2.9, 11.3]),
    ([6.6, 10.7], [6.4, 11.3]),
];

#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub left: bool,
    pub right: bool,
    pub launch: bool,
}

pub struct Table {
    pub world: PhysicsWorld,
    pub ball: RigidBodyHandle,
    ball_collider: ColliderHandle,
    flippers: [RigidBodyHandle; 2],
    bumpers: [ColliderHandle; 3],
    contacts: [bool; 3],
    pub bumper_flash: [f32; 3],
    pub angles: [f32; 2],
    pub score: u32,
    pub lives: u8,
    pub waiting: bool,
    pub launches: u32,
    pub hits: u32,
    pub drains: u32,
    pub time: f32,
    pub trail: [[f32; 2]; 12],
    trail_tick: u32,
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Table {
    pub fn new() -> Self {
        let mut world = PhysicsWorld::new([0.0, 10.5]);
        for (a, b) in RAILS {
            world.raw.insert(
                RigidBodyBuilder::fixed(),
                ColliderBuilder::capsule_from_endpoints(
                    Vector::from_array(a),
                    Vector::from_array(b),
                    0.1,
                )
                .restitution(0.65)
                .friction(0.08),
            );
        }
        let bumpers = BUMPERS.map(|p| {
            world
                .raw
                .insert(
                    RigidBodyBuilder::fixed().translation(Vector::from_array(p)),
                    ColliderBuilder::ball(0.61).restitution(0.9).friction(0.02),
                )
                .1
        });
        let angles = [0.4, std::f32::consts::PI - 0.4];
        let flippers = std::array::from_fn(|i| {
            world
                .raw
                .insert(
                    RigidBodyBuilder::kinematic_position_based()
                        .translation(Vector::from_array(PIVOTS[i]))
                        .rotation(angles[i]),
                    ColliderBuilder::capsule_from_endpoints(
                        Vector::ZERO,
                        Vector::new(FLIPPER_LENGTH, 0.0),
                        0.16,
                    )
                    .friction(0.15)
                    .restitution(0.45),
                )
                .0
        });
        let (ball, ball_collider) = Self::spawn_ball(&mut world);
        Self {
            world,
            ball,
            ball_collider,
            flippers,
            bumpers,
            angles,
            contacts: [false; 3],
            bumper_flash: [0.0; 3],
            score: 0,
            lives: 3,
            waiting: true,
            launches: 0,
            hits: 0,
            drains: 0,
            time: 0.0,
            trail: [[9.25, 14.4]; 12],
            trail_tick: 0,
        }
    }

    fn spawn_ball(world: &mut PhysicsWorld) -> (RigidBodyHandle, ColliderHandle) {
        world.raw.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(9.25, 14.4))
                .gravity_scale(0.0)
                .ccd_enabled(true)
                .linear_damping(0.03),
            ColliderBuilder::ball(BALL_RADIUS)
                .density(2.0)
                .friction(0.08)
                .restitution(0.5),
        )
    }

    pub fn position(&self) -> [f32; 2] {
        self.world.bodies[self.ball].translation().to_array()
    }

    pub fn over(&self) -> bool {
        self.lives == 0
    }

    /// An automatic player presses the same controls as a human, with no ball steering.
    pub fn autoplay(&self) -> Controls {
        let p = self.position();
        let descending = self.world.bodies[self.ball].linvel().y > 0.0;
        let ready = p[1] > 12.95 && descending;
        Controls {
            left: ready && p[0] > 3.6 && p[0] < 5.6,
            right: ready && p[0] > 4.2 && p[0] < 6.3,
            launch: self.waiting,
        }
    }

    pub fn step(&mut self, dt: f32, controls: Controls) {
        if self.over() {
            return;
        }
        self.time += dt;
        for (i, pressed) in [controls.left, controls.right].into_iter().enumerate() {
            let target = if i == 0 {
                if pressed { -0.5 } else { 0.4 }
            } else {
                std::f32::consts::PI - if pressed { -0.5 } else { 0.4 }
            };
            self.angles[i] += (target - self.angles[i]).clamp(-18.0 * dt, 18.0 * dt);
            self.world.bodies[self.flippers[i]]
                .set_next_kinematic_rotation(Rotation::from_angle(self.angles[i]));
        }
        if controls.launch && self.waiting {
            self.waiting = false;
            self.launches += 1;
            let ball = &mut self.world.bodies[self.ball];
            ball.set_gravity_scale(1.0, true);
            ball.apply_impulse(Vector::new(0.0, -25.0) * ball.mass(), true);
        }
        self.world.step(dt);
        for (i, bumper) in self.bumpers.iter().enumerate() {
            self.bumper_flash[i] = (self.bumper_flash[i] - dt).max(0.0);
            let touching = self
                .world
                .raw
                .contact_pair(self.ball_collider, *bumper)
                .is_some_and(|pair| pair.has_any_active_contact());
            if touching && !self.contacts[i] && self.bumper_flash[i] <= 0.0 {
                self.score += 100;
                self.hits += 1;
                self.bumper_flash[i] = 0.18;
                let ball = &mut self.world.bodies[self.ball];
                let outward = (ball.translation() - Vector::from_array(BUMPERS[i]))
                    .try_normalize()
                    .unwrap_or(Vector::new(0.0, -1.0));
                // The powered pop bumper adds energy at actual physical contact.
                ball.apply_impulse(outward * ball.mass() * 7.5, true);
            }
            self.contacts[i] = touching;
        }
        self.trail_tick += 1;
        if self.trail_tick.is_multiple_of(3) {
            self.trail.rotate_right(1);
            self.trail[0] = self.position();
        }
        let p = self.position();
        if !self.waiting
            && p[0] > 8.95
            && p[1] > 14.2
            && self.world.bodies[self.ball].linvel().length() < 1.5
        {
            // A ball that falls back into the real launch lane can be served again.
            self.waiting = true;
        }
        if p[1] > 16.0 || p[0] < -1.0 || p[0] > 11.0 {
            self.lives -= 1;
            self.drains += 1;
            if self.lives > 0 {
                self.world.raw.remove_body(self.ball);
                (self.ball, self.ball_collider) = Self::spawn_ball(&mut self.world);
                self.contacts = [false; 3];
                self.waiting = true;
                self.trail = [self.position(); 12];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: f32 = 1.0 / 120.0;

    #[test]
    fn launch_is_gated_and_uses_impulse() {
        let mut table = Table::new();
        for _ in 0..30 {
            table.step(DT, Controls::default());
        }
        assert_eq!(table.launches, 0);
        assert!((table.position()[1] - 14.4).abs() < 0.01);
        let launch = Controls {
            launch: true,
            ..Controls::default()
        };
        table.step(DT, launch);
        assert!(table.world.bodies[table.ball].linvel().y < -20.0);
        table.step(DT, launch);
        assert_eq!(table.launches, 1);
    }

    #[test]
    fn bumper_scores_on_real_contact_not_proximity() {
        let mut table = Table::new();
        table.waiting = false;
        table.world.bodies[table.ball].set_translation(Vector::new(2.6, 3.0), true);
        table.step(DT, Controls::default());
        assert_eq!(table.score, 0);
        table.world.bodies[table.ball].set_translation(Vector::new(2.6, 3.32), true);
        table.world.bodies[table.ball].set_linvel(Vector::new(0.0, 2.0), true);
        table.step(DT, Controls::default());
        assert_eq!(table.score, 100);
        assert!(table.world.bodies[table.ball].linvel().y < 0.0);
        for _ in 0..10 {
            table.step(DT, Controls::default());
        }
        assert_eq!(table.score, 100);
    }

    #[test]
    fn three_drains_end_game_without_duplicate_life_loss() {
        let mut table = Table::new();
        for lives in [2, 1, 0] {
            table.world.bodies[table.ball].set_translation(Vector::new(4.9, 16.2), true);
            table.step(DT, Controls::default());
            assert_eq!(table.lives, lives);
        }
        for _ in 0..120 {
            table.step(DT, Controls::default());
        }
        assert_eq!(table.drains, 3);
        assert!(table.over());
    }

    #[test]
    fn moving_flipper_transfers_energy_to_ball() {
        let mut table = Table::new();
        table.waiting = false;
        table.world.bodies[table.ball].set_translation(Vector::new(4.1, 13.25), true);
        for _ in 0..8 {
            table.step(
                DT,
                Controls {
                    left: true,
                    ..Controls::default()
                },
            );
        }
        assert!(table.world.bodies[table.ball].linvel().y < -1.0);
        assert!(table.angles[0] < 0.0);
    }

    #[test]
    fn autoplay_plays_real_table_and_scores() {
        let mut table = Table::new();
        for _ in 0..7200 {
            let input = table.autoplay();
            table.step(DT, input);
            assert!(table.position().into_iter().all(f32::is_finite));
        }
        eprintln!(
            "pinball: score={} hits={} launches={} drains={}",
            table.score, table.hits, table.launches, table.drains
        );
        assert!(table.launches > 0);
        assert!(
            table.hits >= 2,
            "autoplay should reach and hit the bumper field"
        );
    }
}

mod app;
#[cfg(not(target_arch = "wasm32"))]
pub use app::run;
