//! A small pinball table driven by Ferravex's original fixed-step physics world.
//!
//! [`PinballGame`] supplies the shared session lifecycle and render-frame input
//! buffering; [`Table`] remains usable independently at a fixed timestep.
mod game;
pub use game::{PinballFeedback, PinballGame};

use ferravex_physics::d2::{
    ColliderBuilder, ColliderHandle, JointBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle,
    Vector,
};

use ferravex_physics::{JointHandle, JointMotor};

// The ball interacts with table fixtures and flippers; capsule rails/flippers
// do not have capsule/capsule contacts and cannot strike one another.
const BALL_LAYER: u32 = 1;
const FLIPPER_LAYER: u32 = 2;
const FIXTURE_LAYER: u32 = 4;
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

/// Held flippers and a launch request. For [`Table::step`], launch is a one-tick
/// command; [`PinballGame::advance`] accepts held input and buffers rising edges.
#[derive(Clone, Copy, Debug, Default)]
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
    hinges: [JointHandle; 2],
    bumpers: [ColliderHandle; 3],
    contacts: [bool; 3],
    // Accepted contact serials let presentation observe hits without changing
    // the simulation's debounce or replaying a suppressed flash.
    bumper_hits: [u64; 3],
    ball_start_score: u32,
    last_ball_points: Option<u32>,
    /// Simulation debounce remaining for each powered bumper. Presentation
    /// observes accepted hits separately; accessibility must not clear this.
    pub bumper_flash: [f32; 3],
    /// Time remaining for each bumper's readable, contact-confirmed +100 label.
    pub bumper_notice: [f32; 3],
    /// Time remaining for the last unsaved drain's next-ball prompt.
    pub drain_notice: f32,
    /// Controls actually applied to the flipper motors on the latest tick.
    pub flipper_pressed: [bool; 2],
    pub angles: [f32; 2],
    pub score: u32,
    pub lives: u8,
    pub waiting: bool,
    pub launches: u32,
    pub hits: u32,
    pub drains: u32,
    pub saves: u32,
    pub ball_save: f32,
    pub save_notice: f32,
    save_available: bool,
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
                .collision_layers(FIXTURE_LAYER, BALL_LAYER)
                .restitution(0.65)
                .friction(0.08),
            );
        }
        let bumpers = BUMPERS.map(|p| {
            world
                .raw
                .insert(
                    RigidBodyBuilder::fixed().translation(Vector::from_array(p)),
                    ColliderBuilder::ball(0.61)
                        .collision_layers(FIXTURE_LAYER, BALL_LAYER)
                        .restitution(0.9)
                        .friction(0.02),
                )
                .1
        });
        let angles = [0.4, std::f32::consts::PI - 0.4];
        let flippers = std::array::from_fn(|i| {
            world
                .raw
                .insert(
                    RigidBodyBuilder::dynamic()
                        .gravity_scale(0.)
                        .ccd_enabled(true)
                        .angular_damping(0.4)
                        .translation(Vector::from_array(PIVOTS[i]))
                        .rotation(angles[i]),
                    ColliderBuilder::capsule_from_endpoints(
                        Vector::ZERO,
                        Vector::new(FLIPPER_LENGTH, 0.0),
                        0.16,
                    )
                    .collision_layers(FLIPPER_LAYER, BALL_LAYER)
                    .density(4.)
                    .friction(0.15)
                    .restitution(0.45),
                )
                .0
        });
        let anchor = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
        let hinges = std::array::from_fn(|i| {
            world
                .insert_joint(
                    anchor,
                    flippers[i],
                    JointBuilder::hinge()
                        .local_anchor1(Vector::from_array(PIVOTS[i]))
                        .limits(
                            if i == 0 { -1. } else { -0.15 },
                            if i == 0 { 0.15 } else { 1. },
                        )
                        .motor(JointMotor::position(0., 40., 18., 1000.)),
                )
                .unwrap()
        });
        let (ball, ball_collider) = Self::spawn_ball(&mut world);
        Self {
            world,
            ball,
            ball_collider,
            flippers,
            hinges,
            bumpers,
            angles,
            contacts: [false; 3],
            bumper_hits: [0; 3],
            ball_start_score: 0,
            last_ball_points: None,
            bumper_flash: [0.0; 3],
            bumper_notice: [0.0; 3],
            drain_notice: 0.,
            flipper_pressed: [false; 2],
            score: 0,
            lives: 3,
            waiting: true,
            launches: 0,
            hits: 0,
            drains: 0,
            saves: 0,
            ball_save: 0.,
            save_notice: 0.,
            save_available: true,
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
                .collision_layers(BALL_LAYER, FIXTURE_LAYER | FLIPPER_LAYER)
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

    /// Points recovered on the current life, including any saved/re-served ball.
    /// An unsaved drain banks the total and starts the next life at zero. This
    /// is a score breakdown, not an extra bonus award; it is zero after game-over.
    ///
    /// ```
    /// use salvage_pinball::{Controls, PinballGame};
    /// let mut game = PinballGame::new(false);
    /// assert_eq!(game.table().ball_points(), 0);
    /// game.advance(1.0 / 120.0, Controls { launch: true, ..Default::default() });
    /// assert_eq!(game.table().ball_points(), 0); // launching does not score
    /// ```
    pub fn ball_points(&self) -> u32 {
        self.score.saturating_sub(self.ball_start_score)
    }

    /// Total recovered on the most recently drained, unsaved life. Saves leave
    /// this unchanged; a fresh table has no completed life. Reading it consumes
    /// no event and never awards those points a second time.
    ///
    /// ```
    /// use salvage_pinball::{Controls, Table};
    /// let mut table = Table::new();
    /// assert_eq!(table.last_ball_points(), None);
    /// // Put the ball past the drain and execute an actual physics tick.
    /// table.world.bodies[table.ball].set_translation([4.9, 17.2].into(), true);
    /// table.step(1.0 / 120.0, Controls::default());
    /// assert_eq!(table.last_ball_points(), Some(0));
    /// assert_eq!(table.lives, 2);
    /// ```
    pub fn last_ball_points(&self) -> Option<u32> {
        self.last_ball_points
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
        self.ball_save = (self.ball_save - dt).max(0.);
        self.save_notice = (self.save_notice - dt).max(0.);
        self.drain_notice = (self.drain_notice - dt).max(0.);
        for (i, pressed) in [controls.left, controls.right].into_iter().enumerate() {
            self.flipper_pressed[i] = pressed;
            let target = if pressed {
                if i == 0 { -0.9 } else { 0.9 }
            } else {
                0.
            };
            self.world.joints[self.hinges[i]]
                .set_motor(Some(JointMotor::position(
                    target,
                    if pressed { 60. } else { 40. },
                    if pressed { 22. } else { 12. },
                    if pressed { 1600. } else { 1000. },
                )))
                .unwrap();
        }
        if controls.launch && self.waiting {
            self.waiting = false;
            self.launches += 1;
            if self.save_available {
                self.ball_save = 5.;
                self.save_available = false;
            }
            let ball = &mut self.world.bodies[self.ball];
            ball.set_gravity_scale(1.0, true);
            ball.apply_impulse(Vector::new(0.0, -25.0) * ball.mass(), true);
        }
        self.world.step(dt);
        for i in 0..2 {
            self.angles[i] = self.world.bodies[self.flippers[i]].rotation().angle();
        }
        for (i, bumper) in self.bumpers.iter().enumerate() {
            self.bumper_flash[i] = (self.bumper_flash[i] - dt).max(0.0);
            self.bumper_notice[i] = (self.bumper_notice[i] - dt).max(0.0);
            let touching = self
                .world
                .raw
                .contact_pair(self.ball_collider, *bumper)
                .is_some_and(|pair| pair.has_any_active_contact());
            if touching && !self.contacts[i] && self.bumper_flash[i] <= 0.0 {
                self.score += 100;
                self.hits += 1;
                self.bumper_hits[i] = self.bumper_hits[i].wrapping_add(1);
                self.bumper_flash[i] = 0.18;
                self.bumper_notice[i] = 0.75;
                let ball = &mut self.world.bodies[self.ball];
                let outward = (ball.translation() - Vector::from_array(BUMPERS[i]))
                    .try_normalize()
                    .unwrap_or(Vector::new(0.0, -1.0));
                // The powered pop bumper adds energy at actual physical contact.
                ball.apply_impulse(outward * ball.mass() * 7.5, true);
            }
            self.contacts[i] = touching;
        }
        // Powered bumpers/flippers can add energy, but a readable table needs a
        // ceiling. Apply this after real contacts rather than steering the ball.
        let ball = &mut self.world.bodies[self.ball];
        let bounded = ball.linvel().clamp_length_max(34.);
        if bounded != ball.linvel() {
            ball.set_linvel(bounded, true);
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
            let saved = self.ball_save > 0.;
            if saved {
                self.saves += 1;
                self.save_notice = 2.;
            } else {
                self.last_ball_points = Some(self.ball_points());
                self.ball_start_score = self.score;
                self.lives -= 1;
                self.save_available = true;
                self.drain_notice = 2.5;
                self.save_notice = 0.;
            }
            self.ball_save = 0.;
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

    #[test]
    fn early_drain_saves_once_and_powered_contacts_have_a_speed_ceiling() {
        let mut table = Table::new();
        table.step(
            DT,
            Controls {
                launch: true,
                ..Default::default()
            },
        );
        table.world.bodies[table.ball].set_translation(Vector::new(4.9, 16.2), true);
        table.world.bodies[table.ball].set_linvel(Vector::new(0., 8.), true);
        table.step(DT, Controls::default());
        assert_eq!((table.saves, table.lives), (1, 3));
        assert!(table.waiting);
        table.step(
            DT,
            Controls {
                launch: true,
                ..Default::default()
            },
        );
        table.world.bodies[table.ball].set_translation(Vector::new(4.9, 16.2), true);
        table.world.bodies[table.ball].set_linvel(Vector::new(0., 8.), true);
        table.step(DT, Controls::default());
        assert_eq!((table.saves, table.lives), (1, 2));
        table.world.bodies[table.ball].set_linvel(Vector::new(-100., 0.), true);
        table.step(DT, Controls::default());
        assert!(table.world.bodies[table.ball].linvel().length() <= 34.01);
    }

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
    fn both_flippers_attack_promptly_and_return_when_released() {
        let mut table = Table::new();
        for _ in 0..6 {
            table.step(
                DT,
                Controls {
                    left: true,
                    right: true,
                    launch: false,
                },
            );
        }
        assert!(
            table.angles[0] < -0.15,
            "left flipper should rise within 50ms: {:?}",
            table.angles
        );
        assert!(table.angles[1].sin() < -0.15, "right tip must also rise");
        assert_eq!(table.flipper_pressed, [true; 2]);
        for _ in 0..30 {
            table.step(DT, Controls::default());
        }
        assert!((table.angles[0] - 0.4).abs() < 0.03);
        assert!((table.angles[1] - (std::f32::consts::PI - 0.4)).abs() < 0.03);
        assert_eq!(table.flipper_pressed, [false; 2]);
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
