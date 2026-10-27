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
/// Where the armed magnet crane catches the ball: the open top-centre lane.
pub const CRANE: [f32; 2] = [4.4, 2.05];
/// Catch radius around [`CRANE`] for the ball's centre.
pub const CRANE_RADIUS: f32 = 0.5;
/// Seconds the crane holds a caught ball before dropping it.
pub const CRANE_HOLD: f32 = 1.2;
/// Base points for a bumper contact before the yard multiplier.
pub const BUMPER_POINTS: u32 = 100;
/// Points for a launch whose first bumper contact is the lit skill-shot bumper.
pub const SKILL_SHOT_POINTS: u32 = 1000;
/// Jackpot points per multiplier step; a jackpot at x2 is worth 5,000.
pub const JACKPOT_POINTS: u32 = 2500;
/// One-time hidden award for keeping a single ball alive this long.
pub const OVERTIME_SECONDS: f32 = 45.;
pub const OVERTIME_POINTS: u32 = 5000;
/// A bumper hit within this many seconds of the previous one extends a chain.
pub const CHAIN_WINDOW: f32 = 1.2;
pub const MAX_MULTIPLIER: u32 = 5;
/// While a ball waits, the skill-shot lamp visits centre, right, then left.
const SKILL_ORDER: [usize; 3] = [2, 1, 0];
/// Seconds the skill-shot lamp rests on each bumper while a ball waits.
pub const SKILL_STEP: f32 = 0.4;
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
    /// Yard multiplier for bumper points, `1..=MAX_MULTIPLIER`. Each crane
    /// jackpot raises it; an unsaved drain resets it (a ball save keeps it).
    pub multiplier: u32,
    /// Bumper lamps lit toward the next crane. All three arm the crane.
    pub lamps: [bool; 3],
    /// An armed crane catches the ball near [`CRANE`] for a jackpot.
    pub crane_armed: bool,
    /// Seconds left holding a caught ball. The ball is pinned while positive.
    pub crane_hold: f32,
    /// Bumper hits in the current chain; zero once the chain window lapses.
    pub chain: u32,
    chain_timer: f32,
    /// Longest chain this game.
    pub best_chain: u32,
    /// Bumper lit for the skill shot. Cycles while waiting, frozen in flight.
    pub skill_target: usize,
    /// True from a launch until the first bumper contact or the ball drops
    /// into the lower playfield. The crane ignores a ball on its launch arc.
    pub launch_flight: bool,
    /// Seconds the current life has had a ball in play.
    pub ball_time: f32,
    /// The hidden overtime award has been collected this game.
    pub overtime: bool,
    pub jackpots: u32,
    pub skill_shots: u32,
    pub crane_arms: u32,
    pub secrets: u32,
    /// Points awarded by each bumper's latest accepted contact, including the
    /// multiplier, chain bonus and skill shot. Presentation pops read this.
    pub bumper_points: [u32; 3],
    /// Points from the most recent jackpot.
    pub last_jackpot: u32,
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
            multiplier: 1,
            lamps: [false; 3],
            crane_armed: false,
            crane_hold: 0.,
            chain: 0,
            chain_timer: 0.,
            best_chain: 0,
            skill_target: SKILL_ORDER[0],
            launch_flight: false,
            ball_time: 0.,
            overtime: false,
            jackpots: 0,
            skill_shots: 0,
            crane_arms: 0,
            secrets: 0,
            bumper_points: [0; 3],
            last_jackpot: 0,
        }
    }

    /// Points the next plain bumper contact is worth at the current multiplier.
    pub fn bumper_value(&self) -> u32 {
        BUMPER_POINTS * self.multiplier
    }

    /// Extra points a hit would add as the `chain`-th consecutive contact.
    /// Chains of three or more pay an escalating bonus, capped at 500.
    ///
    /// ```
    /// use salvage_pinball::Table;
    /// assert_eq!(Table::chain_bonus(2), 0);
    /// assert_eq!(Table::chain_bonus(3), 50);
    /// assert_eq!(Table::chain_bonus(40), 500);
    /// ```
    pub fn chain_bonus(chain: u32) -> u32 {
        (chain.saturating_sub(2) * 50).min(500)
    }

    /// Accepted bumper contact: scoring, lamps, chain and the skill shot.
    fn bumper_hit(&mut self, i: usize) {
        self.chain = if self.chain_timer > 0. {
            self.chain + 1
        } else {
            1
        };
        self.chain_timer = CHAIN_WINDOW;
        self.best_chain = self.best_chain.max(self.chain);
        let mut points = self.bumper_value() + Self::chain_bonus(self.chain);
        if self.launch_flight {
            self.launch_flight = false;
            if i == self.skill_target {
                self.skill_shots += 1;
                points += SKILL_SHOT_POINTS;
            }
        }
        self.score += points;
        self.bumper_points[i] = points;
        if !self.crane_armed {
            self.lamps[i] = true;
            if self.lamps == [true; 3] {
                self.crane_armed = true;
                self.crane_arms += 1;
            }
        }
    }

    /// Pin a caught ball to the magnet, or drop it once the hold expires.
    fn crane(&mut self, dt: f32) {
        let ball = &mut self.world.bodies[self.ball];
        if self.crane_hold > 0. {
            self.crane_hold = (self.crane_hold - dt).max(0.);
            if self.crane_hold > 0. {
                ball.set_translation(Vector::from_array(CRANE), true);
                ball.set_linvel(Vector::ZERO, true);
            } else {
                // Alternate the drop so repeated jackpots do not replay one path.
                let side = if self.jackpots.is_multiple_of(2) {
                    1.
                } else {
                    -1.
                };
                ball.set_gravity_scale(1.0, true);
                ball.set_linvel(Vector::new(side * 1.1, 1.5), true);
            }
            return;
        }
        let near = (ball.translation() - Vector::from_array(CRANE)).length() < CRANE_RADIUS;
        if self.crane_armed && !self.launch_flight && !self.waiting && near {
            self.crane_armed = false;
            self.crane_hold = CRANE_HOLD;
            self.jackpots += 1;
            self.last_jackpot = JACKPOT_POINTS * self.multiplier;
            self.score += self.last_jackpot;
            self.multiplier = (self.multiplier + 1).min(MAX_MULTIPLIER);
            self.lamps = [false; 3];
            ball.set_gravity_scale(0., true);
            ball.set_translation(Vector::from_array(CRANE), true);
            ball.set_linvel(Vector::ZERO, true);
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
        self.chain_timer = (self.chain_timer - dt).max(0.);
        if self.chain_timer == 0. {
            self.chain = 0;
        }
        if self.waiting {
            let step = (self.time / SKILL_STEP) as usize;
            self.skill_target = SKILL_ORDER[step % SKILL_ORDER.len()];
        }
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
            self.launch_flight = true;
            if self.save_available {
                self.ball_save = 5.;
                self.save_available = false;
            }
            let ball = &mut self.world.bodies[self.ball];
            ball.set_gravity_scale(1.0, true);
            ball.apply_impulse(Vector::new(0.0, -25.0) * ball.mass(), true);
        }
        self.crane(dt);
        self.world.step(dt);
        for i in 0..2 {
            self.angles[i] = self.world.bodies[self.flippers[i]].rotation().angle();
        }
        for (i, bumper) in self.bumpers.into_iter().enumerate() {
            self.bumper_flash[i] = (self.bumper_flash[i] - dt).max(0.0);
            self.bumper_notice[i] = (self.bumper_notice[i] - dt).max(0.0);
            let touching = self
                .world
                .raw
                .contact_pair(self.ball_collider, bumper)
                .is_some_and(|pair| pair.has_any_active_contact());
            if touching && !self.contacts[i] && self.bumper_flash[i] <= 0.0 {
                self.bumper_hit(i);
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
        if self.launch_flight && p[1] > 4.8 && p[0] < 8.7 {
            // The launch arc missed every bumper and fell into the playfield.
            self.launch_flight = false;
        }
        if !self.waiting {
            self.ball_time += dt;
            if !self.overtime && self.ball_time >= OVERTIME_SECONDS {
                self.overtime = true;
                self.secrets += 1;
                self.score += OVERTIME_POINTS;
            }
        }
        if !self.waiting
            && p[0] > 8.95
            && p[1] > 14.2
            && self.world.bodies[self.ball].linvel().length() < 1.5
        {
            // A ball that falls back into the real launch lane can be served again.
            self.waiting = true;
            self.launch_flight = false;
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
                self.multiplier = 1;
                self.lamps = [false; 3];
                self.crane_armed = false;
                self.ball_time = 0.;
            }
            self.chain = 0;
            self.chain_timer = 0.;
            self.crane_hold = 0.;
            self.launch_flight = false;
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

    /// Teleport onto a bumper's top and run one tick, as the presentation tests do.
    fn touch_bumper(table: &mut Table, bumper: usize) -> u32 {
        let before = table.score;
        table.bumper_flash = [0.; 3];
        let [x, y] = BUMPERS[bumper];
        table.world.bodies[table.ball].set_translation(Vector::new(x, y - 0.78), true);
        table.world.bodies[table.ball].set_linvel(Vector::new(0., 2.), true);
        table.step(DT, Controls::default());
        table.score - before
    }

    fn park(table: &mut Table) {
        // Away from every bumper and outside the crane's catch zone.
        table.world.bodies[table.ball].set_translation(Vector::new(7.4, 6.6), true);
        table.world.bodies[table.ball].set_linvel(Vector::ZERO, true);
        table.step(DT, Controls::default());
    }

    fn drain(table: &mut Table) {
        table.world.bodies[table.ball].set_translation(Vector::new(4.9, 16.2), true);
        table.world.bodies[table.ball].set_linvel(Vector::new(0., 8.), true);
        table.step(DT, Controls::default());
    }

    #[test]
    fn three_lamps_arm_the_crane_which_pays_a_jackpot_and_raises_the_multiplier() {
        let mut table = Table::new();
        table.waiting = false;
        for bumper in [0, 0, 1] {
            touch_bumper(&mut table, bumper);
            park(&mut table);
            for _ in 0..(CHAIN_WINDOW / DT) as usize {
                table.step(DT, Controls::default());
                table.world.bodies[table.ball].set_translation(Vector::new(7.4, 6.6), true);
            }
        }
        assert_eq!(table.lamps, [true, true, false]);
        assert!(!table.crane_armed);
        assert_eq!(touch_bumper(&mut table, 2), 100, "chain lapsed: plain hit");
        assert!(table.crane_armed);
        assert_eq!(table.crane_arms, 1);
        // While armed, further hits keep the lamps lit without re-arming.
        park(&mut table);
        touch_bumper(&mut table, 0);
        assert_eq!(table.crane_arms, 1);

        let before = table.score;
        table.world.bodies[table.ball].set_translation(Vector::from_array(CRANE), true);
        table.world.bodies[table.ball].set_linvel(Vector::new(3., -4.), true);
        table.step(DT, Controls::default());
        assert_eq!(table.jackpots, 1);
        assert_eq!(table.score - before, JACKPOT_POINTS);
        assert_eq!(table.last_jackpot, JACKPOT_POINTS);
        assert_eq!(table.multiplier, 2);
        assert_eq!(table.lamps, [false; 3]);
        assert!(!table.crane_armed && table.crane_hold > 0.);
        // The magnet holds the ball still, then drops it back into play.
        for _ in 0..(CRANE_HOLD / DT) as usize - 2 {
            table.step(DT, Controls::default());
            assert_eq!(table.position(), CRANE);
        }
        for _ in 0..60 {
            table.step(DT, Controls::default());
        }
        assert_eq!(table.crane_hold, 0.);
        assert!(table.position()[1] > CRANE[1] + 0.5, "dropped ball falls");
        assert_eq!(table.jackpots, 1, "no re-catch without re-arming");
        assert_eq!(table.bumper_value(), 200);
    }

    #[test]
    fn launch_arc_ignores_the_crane_and_the_skill_shot_needs_the_lit_bumper() {
        // Launch while the centre lamp is lit: the arc always reaches the left
        // bumper first, so there is no skill shot, and an armed crane on the
        // arc's path does not catch the ball.
        let mut table = Table::new();
        table.crane_armed = true;
        table.step(
            DT,
            Controls {
                launch: true,
                ..Default::default()
            },
        );
        assert_eq!(table.skill_target, 2);
        assert!(table.launch_flight);
        let mut first = None;
        for _ in 0..400 {
            table.step(DT, Controls::default());
            if table.hits > 0 {
                first = Some(table.score);
                break;
            }
        }
        assert_eq!(first, Some(100));
        assert_eq!((table.skill_shots, table.jackpots), (0, 0));
        assert!(!table.launch_flight);

        // Wait for the hopping lamp to reach the left bumper, then launch.
        let mut table = Table::new();
        while table.skill_target != 0 {
            table.step(DT, Controls::default());
        }
        table.step(
            DT,
            Controls {
                launch: true,
                ..Default::default()
            },
        );
        for _ in 0..400 {
            table.step(DT, Controls::default());
            if table.hits > 0 {
                break;
            }
        }
        assert_eq!(table.skill_shots, 1);
        assert_eq!(table.score, 100 + SKILL_SHOT_POINTS);
        assert_eq!(table.bumper_points[0], 100 + SKILL_SHOT_POINTS);
    }

    #[test]
    fn quick_hits_build_a_capped_chain_bonus_that_lapses() {
        let mut table = Table::new();
        table.waiting = false;
        let mut paid = Vec::new();
        for bumper in [0, 1, 0, 1, 0] {
            paid.push(touch_bumper(&mut table, bumper));
            park(&mut table);
        }
        assert_eq!(paid, [100, 100, 150, 200, 250]);
        assert_eq!((table.chain, table.best_chain), (5, 5));
        for _ in 0..(CHAIN_WINDOW / DT) as usize + 1 {
            table.step(DT, Controls::default());
            table.world.bodies[table.ball].set_translation(Vector::new(7.4, 6.6), true);
        }
        assert_eq!(table.chain, 0);
        assert_eq!(touch_bumper(&mut table, 1), 100);
        assert_eq!(table.best_chain, 5);
    }

    #[test]
    fn a_save_keeps_the_multiplier_but_an_unsaved_drain_resets_the_ball_rules() {
        let mut table = Table::new();
        table.step(
            DT,
            Controls {
                launch: true,
                ..Default::default()
            },
        );
        table.multiplier = 3;
        table.lamps = [true, false, true];
        drain(&mut table);
        assert_eq!((table.saves, table.lives), (1, 3));
        assert_eq!(table.multiplier, 3);
        assert_eq!(table.lamps, [true, false, true]);
        table.step(
            DT,
            Controls {
                launch: true,
                ..Default::default()
            },
        );
        table.crane_armed = true;
        drain(&mut table);
        assert_eq!(table.lives, 2);
        assert_eq!(table.multiplier, 1);
        assert_eq!(table.lamps, [false; 3]);
        assert!(!table.crane_armed);
        assert_eq!((table.chain, table.ball_time), (0, 0.));
    }

    #[test]
    fn overtime_secret_pays_once_per_game_on_a_long_ball() {
        let mut table = Table::new();
        table.waiting = false;
        let mut paid = 0;
        for _ in 0..((OVERTIME_SECONDS + 20.) / DT) as usize {
            // Hold the ball still away from everything; only time passes.
            table.world.bodies[table.ball].set_translation(Vector::new(7.4, 6.6), true);
            table.world.bodies[table.ball].set_linvel(Vector::ZERO, true);
            let before = table.score;
            table.step(DT, Controls::default());
            paid += table.score - before;
        }
        assert!(table.overtime);
        assert_eq!((table.secrets, paid), (1, OVERTIME_POINTS));
        drain(&mut table);
        for _ in 0..((OVERTIME_SECONDS + 1.) / DT) as usize {
            table.waiting = false;
            table.world.bodies[table.ball].set_translation(Vector::new(7.4, 6.6), true);
            table.step(DT, Controls::default());
        }
        assert_eq!(table.secrets, 1, "one secret per game");
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
