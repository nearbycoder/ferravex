use crate::presentation::{AcceptedEvent, Spark};
use ferravex_engine::{
    dynamics::{BodyDesc, BodyHandle, PhysicsWorld, QueryFilter, Shape},
    events::{EventQueue, Overflow},
    play::feedback::FeedbackPolicy,
    play::particles::{ParticleDesc, ParticlePool},
};

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 720;
pub const LEFT: f32 = 38.0;
pub const RIGHT: f32 = 602.0;
pub const TOP: f32 = 106.0;
pub const PADDLE_Y: f32 = 636.0;
pub const BALL_RADIUS: f32 = 6.0;
const PADDLE_HALF: f32 = 43.0;
const MAX_IMPACTS: usize = 8;
/// Extra balls released by a multi-ball capsule; never more than this at once.
pub const MAX_EXTRA_BALLS: usize = 4;
/// Paddle catches granted by a sticky capsule.
pub const STICKY_CATCHES: u32 = 3;
/// Seconds a caught ball waits before releasing itself.
pub const HOLD_SECONDS: f32 = 0.9;
/// Points for bonking the drifting satellite above the panels.
pub const SATELLITE_POINTS: u32 = 1000;
/// Points for clearing a sector without losing a ball.
pub const FLAWLESS_POINTS: u32 = 1000;
const SATELLITE_FIRST: f32 = 12.;
const SATELLITE_PERIOD: f32 = 24.;
const SATELLITE_PASS: f32 = 7.;
const SATELLITE_Y: f32 = 121.;
/// Capsule kinds rotate in this deterministic order, so runs replay exactly.
const PICKUP_ORDER: [Pickup; 6] = [
    Pickup::Wide,
    Pickup::Split,
    Pickup::Sticky,
    Pickup::Split,
    Pickup::Wide,
    Pickup::Sticky,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Phase {
    Ready = 0,
    Playing = 1,
    Transition = 2,
    Won = 3,
    Lost = 4,
}
/// Brief feedback for accepted gameplay events, driven by simulation time.
/// See [`Game::feedback`] for an example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feedback {
    Launched,
    BallSaved,
    LifeLost,
    SectorCleared,
    Restarted,
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Input {
    pub axis: f32,
    pub launch: bool,
    pub restart: bool,
}
pub struct Block {
    pub body: BodyHandle,
    pub center: [f32; 2],
    pub half: [f32; 2],
    pub health: u8,
    pub original_health: u8,
    pub flash: f32,
    pub row: usize,
}
/// What a falling capsule does when the paddle catches it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pickup {
    /// A wider paddle for nine seconds.
    #[default]
    Wide,
    /// Two extra balls burst from the paddle; losing them costs nothing.
    Split,
    /// The next three paddle returns catch the ball; launch (or wait) to fire.
    Sticky,
}
pub struct Drop {
    pub position: [f32; 2],
    pub kind: Pickup,
}
/// A bonus ball from a multi-ball capsule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ball {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
}
/// A ball caught by the sticky paddle, riding at `offset` from its center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Held {
    pub offset: f32,
    pub velocity: [f32; 2],
    pub remaining: f32,
}

pub struct Game {
    world: PhysicsWorld<2>,
    paddle_body: BodyHandle,
    pub paddle_x: f32,
    pub paddle_half: f32,
    pub ball: [f32; 2],
    pub velocity: [f32; 2],
    pub blocks: Vec<Block>,
    pub particles: ParticlePool,
    pub drops: Vec<Drop>,
    pub extra_balls: Vec<Ball>,
    pub held: Option<Held>,
    pub sticky: u32,
    pub trail: Vec<[f32; 2]>,
    pub phase: Phase,
    pub lives: u32,
    pub ball_save: f32,
    pub saves: u32,
    pub serve_charge: ferravex_engine::gameplay::charge::ChargeAction,
    pub serve_power: f32,
    charged_volley: bool,
    pub sector: u32,
    pub score: u32,
    pub chain: ferravex_engine::gameplay::combo::ScoreChain,
    pub time: f32,
    pub wide_time: f32,
    pub hits: u32,
    pub precision: ferravex_engine::gameplay::skill::PrecisionReturns,
    pub clears: u32,
    pub ccd_impacts: u64,
    /// Panel hits since any ball last touched the paddle.
    pub volley: u32,
    pub best_volley: u32,
    pub satellites: u32,
    pub lives_lost: u32,
    pub flawless_sectors: u32,
    satellite_bonked: Option<u32>,
    pub(crate) sector_lives_lost: u32,
    drops_spawned: u32,
    transition: f32,
    launch_count: u32,
    reward_rng: ferravex_engine::gameplay::loot::RewardRng,
    drop_pity: ferravex_engine::gameplay::pity::PityCounter,
    trail_clock: f32,
    paddle_velocity: f32,
    feedback: Option<Feedback>,
    feedback_time: f32,
    pub(crate) events: EventQueue<AcceptedEvent>,
    pub(crate) policy: FeedbackPolicy,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    pub fn new() -> Self {
        let mut world = PhysicsWorld::<2>::new([0.0; 2]);
        for (center, half) in [
            ([LEFT - 10.0, 380.0], [10.0, 274.0]),
            ([RIGHT + 10.0, 380.0], [10.0, 274.0]),
            ([320.0, TOP - 10.0], [282.0, 10.0]),
        ] {
            world
                .insert(BodyDesc::fixed(Shape::cuboid(half), center))
                .expect("valid wall");
        }
        let paddle_body = world
            .insert(BodyDesc::fixed(
                Shape::cuboid([PADDLE_HALF, 8.0]),
                [320.0, PADDLE_Y],
            ))
            .expect("valid paddle");
        let mut game = Self {
            world,
            paddle_body,
            paddle_x: 320.0,
            paddle_half: PADDLE_HALF,
            ball: [320.0, PADDLE_Y - 16.0],
            velocity: [0.0; 2],
            blocks: Vec::with_capacity(35),
            particles: ParticlePool::new(512),
            drops: Vec::with_capacity(8),
            extra_balls: Vec::with_capacity(MAX_EXTRA_BALLS),
            held: None,
            sticky: 0,
            trail: Vec::with_capacity(18),
            phase: Phase::Ready,
            lives: 3,
            ball_save: 0.,
            saves: 0,
            serve_charge: ferravex_engine::gameplay::charge::ChargeAction::new(0., 1.5, 0.)
                .unwrap(),
            serve_power: 0.,
            charged_volley: false,
            sector: 1,
            score: 0,
            chain: ferravex_engine::gameplay::combo::ScoreChain::new(4., 3, 4).unwrap(),
            time: 0.0,
            wide_time: 0.0,
            hits: 0,
            precision: ferravex_engine::gameplay::skill::PrecisionReturns::new(0.18, 50, 4)
                .unwrap(),
            clears: 0,
            ccd_impacts: 0,
            volley: 0,
            best_volley: 0,
            satellites: 0,
            lives_lost: 0,
            flawless_sectors: 0,
            satellite_bonked: None,
            sector_lives_lost: 0,
            drops_spawned: 0,
            transition: 0.0,
            launch_count: 0,
            reward_rng: ferravex_engine::gameplay::loot::RewardRng::new(0x4f52424954),
            drop_pity: ferravex_engine::gameplay::pity::PityCounter::new(6).unwrap(),
            trail_clock: 0.0,
            paddle_velocity: 0.,
            feedback: None,
            feedback_time: 0.,
            events: EventQueue::new(32, Overflow::EvictOldest),
            policy: FeedbackPolicy::default(),
        };
        game.build_sector();
        game
    }
    /// Restore the original world and show a fresh-run acknowledgement.
    ///
    /// ```
    /// use owned_orbit::game::{Feedback, Game, Phase};
    /// let mut game = Game::new();
    /// game.score = 500;
    /// game.restart();
    /// assert_eq!((game.score, game.lives, game.phase), (0, 3, Phase::Ready));
    /// assert_eq!(game.feedback(), Some(Feedback::Restarted));
    /// ```
    pub fn restart(&mut self) {
        let policy = self.policy;
        *self = Self::new();
        self.policy = policy;
        self.show_feedback(Feedback::Restarted, 2.5);
    }

    // This policy is supplied by the presentation owner; simulation never reads
    // renderer state. Clear existing cosmetics, so toggling back cannot revive them.
    pub(crate) fn apply_policy(&mut self, policy: FeedbackPolicy) {
        self.policy = policy;
        if policy.reduced_motion() || policy.intensity() == 0. {
            self.particles.clear();
            self.trail.clear();
            self.trail_clock = 0.;
        }
        if policy.reduced_flash() || policy.intensity() == 0. {
            for block in &mut self.blocks {
                block.flash = 0.;
            }
        }
    }

    pub(crate) fn armor_piercing(&self) -> bool {
        self.charged_volley && self.phase == Phase::Playing
    }

    fn record(&mut self, event: AcceptedEvent) {
        // Orbit drains after each fixed tick. Standalone Game users may leave
        // presentation unconsumed indefinitely; retain only the latest 32 events.
        let _ = self.events.send(event);
    }

    /// The current short acknowledgement, cleared by simulation time or a newer event.
    ///
    /// ```
    /// use owned_orbit::game::{Feedback, Game, Input};
    /// let mut game = Game::new();
    /// game.tick(Input { launch: true, ..Input::default() }, 1.0 / 120.0);
    /// assert_eq!(game.feedback(), Some(Feedback::Launched));
    /// ```
    pub fn feedback(&self) -> Option<Feedback> {
        self.feedback
    }

    /// Seconds until the next sector becomes ready; zero outside the transition.
    ///
    /// ```
    /// assert_eq!(owned_orbit::game::Game::new().transition_remaining(), 0.0);
    /// ```
    pub fn transition_remaining(&self) -> f32 {
        if self.phase == Phase::Transition {
            self.transition.max(0.)
        } else {
            0.
        }
    }

    fn show_feedback(&mut self, feedback: Feedback, seconds: f32) {
        self.feedback = Some(feedback);
        self.feedback_time = seconds;
    }
    pub fn remaining(&self) -> usize {
        self.blocks.iter().filter(|block| block.health > 0).count()
    }
    pub fn autoplay(&self) -> Input {
        let mut landing = self.ball[0];
        if self.velocity[1] > 0.0 {
            let flight =
                ((PADDLE_Y - 8.0 - BALL_RADIUS - self.ball[1]) / self.velocity[1]).max(0.0);
            landing = reflect_x(self.ball[0] + self.velocity[0] * flight);
        }
        let target = self.blocks.iter().filter(|b| b.health > 0).min_by(|a, b| {
            let da = (a.center[0] - landing).abs() + (a.center[1] - self.ball[1]).abs() * 0.05;
            let db = (b.center[0] - landing).abs() + (b.center[1] - self.ball[1]).abs() * 0.05;
            da.total_cmp(&db)
        });
        let offset = if let Some(target) = target {
            let dx = target.center[0] - landing;
            let dy = PADDLE_Y - target.center[1];
            (dx / dx.hypot(dy) / 0.86).clamp(-0.8, 0.8) * self.paddle_half
        } else {
            0.0
        };
        let desired = if self.velocity[1] > 0.0 {
            landing - offset
        } else {
            self.ball[0]
        };
        Input {
            axis: ((desired - self.paddle_x) / 10.0).clamp(-1.0, 1.0),
            launch: self.phase == Phase::Ready || self.held.is_some(),
            restart: false,
        }
    }
    pub fn tick(&mut self, input: Input, dt: f32) {
        if !dt.is_finite() || !(0.0..=0.1).contains(&dt) {
            return;
        }
        if input.restart || input.launch && matches!(self.phase, Phase::Won | Phase::Lost) {
            self.restart();
            return;
        }
        self.feedback_time = (self.feedback_time - dt).max(0.);
        if self.feedback_time == 0. {
            self.feedback = None;
        }
        self.particles.update(dt).expect("validated delta");
        for block in &mut self.blocks {
            block.flash = (block.flash - dt).max(0.0);
        }
        if matches!(self.phase, Phase::Won | Phase::Lost) {
            return;
        }
        self.time += dt;
        self.ball_save = (self.ball_save - dt).max(0.);
        self.chain.advance(dt).unwrap();
        if self.phase == Phase::Transition {
            self.transition -= dt;
            if self.transition <= 0.0 {
                self.sector += 1;
                self.build_sector();
                self.phase = Phase::Ready;
            }
            return;
        }
        if self.wide_time > 0.0 {
            self.wide_time = (self.wide_time - dt).max(0.0);
            if self.wide_time == 0.0 {
                self.resize_paddle(PADDLE_HALF);
            }
        }
        let axis = if input.axis.is_finite() {
            input.axis.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let previous_paddle = self.paddle_x;
        self.paddle_x = (self.paddle_x + axis * 590.0 * dt).clamp(
            LEFT + self.paddle_half + 1.0,
            RIGHT - self.paddle_half - 1.0,
        );
        self.paddle_velocity = if dt > 0. {
            (self.paddle_x - previous_paddle) / dt
        } else {
            0.
        };
        self.world
            .body_mut(self.paddle_body)
            .expect("paddle alive")
            .set_position([self.paddle_x, PADDLE_Y])
            .expect("finite paddle");
        if self.phase == Phase::Ready {
            self.serve_charge.step(dt, true).unwrap();
            self.ball = [self.paddle_x, PADDLE_Y - 8.0 - BALL_RADIUS - 1.0];
            if input.launch {
                self.serve_power = self.serve_charge.step(0., false).unwrap().unwrap_or(0.);
                self.charged_volley = self.serve_power >= 0.95;
                self.phase = Phase::Playing;
                self.launch_count += 1;
                self.ball_save = 2.5;
                let angle = 0.24 + (self.launch_count % 3) as f32 * 0.09;
                let speed = self.speed() * (1. + self.serve_power * 0.2);
                self.velocity = [angle.sin() * speed, -angle.cos() * speed];
                self.show_feedback(Feedback::Launched, 1.4);
                self.record(AcceptedEvent::Launch);
                self.emit(self.ball, [0.35, 0.85, 0.79, 1.], 10);
            } else {
                return;
            }
        } else if let Some(mut held) = self.held {
            // A sticky catch rides the paddle until launch or its short timer.
            held.remaining -= dt;
            let reach = self.paddle_half - BALL_RADIUS;
            self.ball = [
                self.paddle_x + held.offset.clamp(-reach, reach),
                PADDLE_Y - 8. - BALL_RADIUS - 1.,
            ];
            if input.launch || held.remaining <= 0. {
                self.held = None;
                self.velocity = held.velocity;
                self.record(AcceptedEvent::Spark(Spark::Release { at: self.ball }));
                self.emit(self.ball, [0.62, 0.92, 0.55, 1.], 8);
            } else {
                self.held = Some(held);
            }
        }
        self.move_ball(dt, true);
        for index in 0..self.extra_balls.len() {
            // Bonus balls reuse the primary's exact collision and reward rules.
            std::mem::swap(&mut self.ball, &mut self.extra_balls[index].position);
            std::mem::swap(&mut self.velocity, &mut self.extra_balls[index].velocity);
            self.move_ball(dt, false);
            std::mem::swap(&mut self.ball, &mut self.extra_balls[index].position);
            std::mem::swap(&mut self.velocity, &mut self.extra_balls[index].velocity);
        }
        let before = self.extra_balls.len();
        self.extra_balls.retain(|ball| ball.position[1] <= 677.);
        for _ in self.extra_balls.len()..before {
            self.record(AcceptedEvent::Spark(Spark::Fizzle));
        }
        self.check_satellite();
        self.update_drops(dt);
        self.trail_clock += dt;
        if self.trail_clock >= 1.0 / 90.0
            && !self.policy.reduced_motion()
            && self.policy.intensity() > 0.
        {
            self.trail_clock = 0.0;
            if self.trail.len() == 18 {
                self.trail.remove(0);
            }
            self.trail.push(self.ball);
        }
        if self.ball[1] > 677.0 && !self.extra_balls.is_empty() {
            // A bonus ball takes over; nobody loses a life to multi-ball.
            let next = self.extra_balls.remove(0);
            self.ball = next.position;
            self.velocity = next.velocity;
            self.trail.clear();
            self.record(AcceptedEvent::Spark(Spark::Backup { at: self.ball }));
        } else if self.ball[1] > 677.0 && self.ball_save > 0. {
            self.ball_save = 0.;
            self.saves += 1;
            self.precision.reset();
            self.charged_volley = false;
            self.ball = [self.paddle_x, PADDLE_Y - 8. - BALL_RADIUS - 1.];
            let speed = self.speed();
            self.velocity = [speed * 0.24, -speed * (1. - 0.24_f32.powi(2)).sqrt()];
            self.chain.break_chain();
            self.volley = 0;
            self.trail.clear();
            self.emit(self.ball, [0.35, 0.85, 0.79, 1.], 18);
            self.show_feedback(Feedback::BallSaved, 1.8);
            self.record(AcceptedEvent::Save);
        } else if self.ball[1] > 677.0 {
            self.lives -= 1;
            self.lives_lost += 1;
            self.sector_lives_lost += 1;
            self.precision.reset();
            self.chain.break_chain();
            self.volley = 0;
            self.trail.clear();
            self.velocity = [0.0; 2];
            self.drops.clear();
            self.held = None;
            self.sticky = 0;
            self.wide_time = 0.0;
            self.resize_paddle(PADDLE_HALF);
            let drain = [self.ball[0].clamp(LEFT, RIGHT), 653.];
            self.emit(drain, [0.88, 0.68, 0.40, 1.], 16);
            self.ball = [self.paddle_x, PADDLE_Y - 8. - BALL_RADIUS - 1.];
            self.serve_charge.cancel();
            self.serve_power = 0.;
            self.charged_volley = false;
            self.show_feedback(Feedback::LifeLost, 2.5);
            self.record(AcceptedEvent::LifeLost);
            self.record(AcceptedEvent::Spark(Spark::Drain { at: drain }));
            self.phase = if self.lives == 0 {
                Phase::Lost
            } else {
                Phase::Ready
            };
        } else if self.remaining() == 0 {
            self.phase = if self.sector == 2 {
                Phase::Won
            } else {
                Phase::Transition
            };
            self.transition = 1.4;
            self.velocity = [0.0; 2];
            self.trail.clear();
            self.drops.clear();
            self.extra_balls.clear();
            self.held = None;
            self.volley = 0;
            if self.sector_lives_lost == 0 {
                self.flawless_sectors += 1;
                self.score = self.score.saturating_add(FLAWLESS_POINTS);
                self.record(AcceptedEvent::Spark(Spark::Flawless(FLAWLESS_POINTS)));
            }
            self.show_feedback(Feedback::SectorCleared, 1.4);
            self.record(AcceptedEvent::SectorCleared);
        }
    }

    /// Where the bonus satellite drifts above the panels, if it is passing now.
    ///
    /// It crosses the gap between the top wall and the first row every 24
    /// seconds of play, starting 12 seconds in. Each pass can be bonked once;
    /// the ball flies straight through it, so collisions are unchanged.
    ///
    /// ```
    /// let mut game = owned_orbit::game::Game::new();
    /// assert_eq!(game.satellite(), None);
    /// game.phase = owned_orbit::game::Phase::Playing;
    /// game.time = 15.;
    /// assert!(game.satellite().is_some());
    /// ```
    pub fn satellite(&self) -> Option<[f32; 2]> {
        if self.phase != Phase::Playing || self.time < SATELLITE_FIRST {
            return None;
        }
        let elapsed = self.time - SATELLITE_FIRST;
        let pass = (elapsed / SATELLITE_PERIOD) as u32;
        let t = elapsed - pass as f32 * SATELLITE_PERIOD;
        if t >= SATELLITE_PASS || self.satellite_bonked == Some(pass) {
            return None;
        }
        let progress = t / SATELLITE_PASS;
        let span = RIGHT - LEFT - 36.;
        let x = if pass.is_multiple_of(2) {
            LEFT + 18. + span * progress
        } else {
            RIGHT - 18. - span * progress
        };
        Some([x, SATELLITE_Y + (t * 2.6).sin() * 3.])
    }

    fn check_satellite(&mut self) {
        let Some(at) = self.satellite() else {
            return;
        };
        let near = |p: [f32; 2]| (p[0] - at[0]).hypot(p[1] - at[1]) < BALL_RADIUS + 9.;
        if near(self.ball) || self.extra_balls.iter().any(|b| near(b.position)) {
            let pass = ((self.time - SATELLITE_FIRST) / SATELLITE_PERIOD) as u32;
            self.satellite_bonked = Some(pass);
            self.satellites += 1;
            self.score = self.score.saturating_add(SATELLITE_POINTS);
            self.emit(at, [0.85, 0.80, 1.0, 1.], 24);
            self.record(AcceptedEvent::Spark(Spark::Satellite {
                at,
                points: SATELLITE_POINTS,
            }));
        }
    }
    fn speed(&self) -> f32 {
        (300.0 + self.sector as f32 * 25.0 + self.clears as f32 * 1.1).min(430.0)
    }
    fn resize_paddle(&mut self, half: f32) {
        self.world.remove(self.paddle_body);
        self.paddle_half = half;
        self.paddle_x = self.paddle_x.clamp(LEFT + half + 1.0, RIGHT - half - 1.0);
        self.paddle_body = self
            .world
            .insert(BodyDesc::fixed(
                Shape::cuboid([half, 8.0]),
                [self.paddle_x, PADDLE_Y],
            ))
            .expect("valid paddle");
    }
    fn build_sector(&mut self) {
        for block in &self.blocks {
            self.world.remove(block.body);
        }
        self.blocks.clear();
        self.trail.clear();
        self.drops.clear();
        self.extra_balls.clear();
        self.held = None;
        self.sticky = 0;
        self.sector_lives_lost = 0;
        self.wide_time = 0.0;
        self.resize_paddle(PADDLE_HALF);
        self.paddle_x = 320.0;
        for row in 0..5 {
            for col in 0..7 {
                let center = [80.0 + col as f32 * 80.0, 165.0 + row as f32 * 40.0];
                let half = [35.0, 14.0];
                let body = self
                    .world
                    .insert(BodyDesc::fixed(Shape::cuboid(half), center))
                    .expect("valid block");
                let health = if self.sector == 2 && (row + col) % 3 == 0 {
                    2
                } else {
                    1
                };
                self.blocks.push(Block {
                    body,
                    center,
                    half,
                    health,
                    original_health: health,
                    flash: 0.0,
                    row,
                });
            }
        }
        self.ball = [self.paddle_x, PADDLE_Y - 15.0];
    }
    /// Move the ball currently in `ball`/`velocity`. Only the primary ball can
    /// be caught by the sticky paddle; bonus balls always bounce.
    fn move_ball(&mut self, dt: f32, primary: bool) {
        let mut remaining = dt;
        // Game-specific reflection is driven by the engine's exact continuous
        // circle casts against every wall, paddle, and destructible block.
        for _ in 0..MAX_IMPACTS {
            let speed = self.velocity[0].hypot(self.velocity[1]);
            if speed < 0.001 || remaining <= 1e-6 {
                break;
            }
            let direction = [self.velocity[0] / speed, self.velocity[1] / speed];
            let hit = self.world.cast_ball(
                self.ball,
                BALL_RADIUS,
                direction,
                speed * remaining,
                QueryFilter {
                    only_static: true,
                    ..QueryFilter::default()
                },
            );
            let Some(hit) = hit else {
                self.ball[0] += self.velocity[0] * remaining;
                self.ball[1] += self.velocity[1] * remaining;
                break;
            };
            self.ccd_impacts += 1;
            self.record(AcceptedEvent::Impact);
            self.ball = [
                hit.point[0] + hit.normal[0] * 0.025,
                hit.point[1] + hit.normal[1] * 0.025,
            ];
            remaining = (remaining - hit.distance / speed).max(0.0);
            if hit.body == self.paddle_body && hit.normal[1] < -0.1 && self.velocity[1] > 0.0 {
                let offset = ((self.ball[0] - self.paddle_x) / self.paddle_half).clamp(-1.0, 1.0);
                let horizontal = if offset.abs() < 0.08 {
                    if self.velocity[0] < 0.0 { -0.08 } else { 0.08 }
                } else {
                    offset
                } * 0.86;
                // The paddle's actual movement adds a little English. Angle and
                // total speed remain bounded so a save never creates a flat orbit.
                let horizontal =
                    (horizontal + self.paddle_velocity / 590. * 0.11).clamp(-0.90, 0.90);
                let next_speed = self.speed();
                self.velocity = [
                    horizontal * next_speed,
                    -(1.0 - horizontal * horizontal).sqrt() * next_speed,
                ];
                self.charged_volley = false;
                self.chain.break_chain();
                let bonus = self.precision.returned(offset).unwrap();
                self.score = self.score.saturating_add(bonus);
                if bonus > 0 {
                    self.record(AcceptedEvent::Precision(bonus));
                }
                self.volley = 0;
                self.emit(
                    self.ball,
                    [0.35, 0.85, 0.79, 1.0],
                    if bonus > 0 { 15 } else { 5 },
                );
                self.record(AcceptedEvent::Spark(Spark::Paddle {
                    at: self.ball,
                    bonus,
                }));
                if primary && self.sticky > 0 {
                    self.sticky -= 1;
                    self.held = Some(Held {
                        offset: self.ball[0] - self.paddle_x,
                        velocity: self.velocity,
                        remaining: HOLD_SECONDS,
                    });
                    self.velocity = [0.; 2];
                    self.ball[1] = PADDLE_Y - 8. - BALL_RADIUS - 1.;
                    self.record(AcceptedEvent::Spark(Spark::Caught));
                    break;
                }
            } else {
                let dot = self.velocity[0] * hit.normal[0] + self.velocity[1] * hit.normal[1];
                if dot < 0.0 {
                    self.velocity[0] -= 2.0 * dot * hit.normal[0];
                    self.velocity[1] -= 2.0 * dot * hit.normal[1];
                    // Avoid an unplayable near-horizontal loop between side walls.
                    if self.velocity[1].abs() < speed * 0.18 {
                        let sign = if self.velocity[1] < 0. { -1. } else { 1. };
                        self.velocity[1] = sign * speed * 0.18;
                        self.velocity[0] = self.velocity[0].signum()
                            * (speed * speed - self.velocity[1].powi(2)).sqrt();
                    }
                }
                if let Some(index) = self
                    .blocks
                    .iter()
                    .position(|b| b.body == hit.body && b.health > 0)
                {
                    self.hit_block(index);
                }
            }
        }
    }
    fn hit_block(&mut self, index: usize) {
        let block = &mut self.blocks[index];
        block.health = block
            .health
            .saturating_sub(if self.charged_volley { 2 } else { 1 });
        block.flash = if self.policy.reduced_flash() || self.policy.intensity() == 0. {
            0.
        } else {
            0.13
        };
        let position = block.center;
        let destroyed = block.health == 0;
        let row = block.row;
        let reinforced = block.original_health == 2;
        self.hits += 1;
        self.volley += 1;
        self.best_volley = self.best_volley.max(self.volley);
        self.precision.progress();
        let award = self.chain.award(if destroyed { 100 } else { 25 });
        self.score = self.score.saturating_add(award as u32);
        if destroyed {
            self.world.remove(block.body);
            self.clears += 1;
            let natural = ferravex_engine::gameplay::loot::RewardTable::new([3, 1])
                .unwrap()
                .roll(&mut self.reward_rng)
                == 1;
            if self.drops.len() < 8 && self.drop_pity.attempt(natural) {
                let kind = PICKUP_ORDER[self.drops_spawned as usize % PICKUP_ORDER.len()];
                self.drops_spawned += 1;
                self.drops.push(Drop { position, kind });
            }
        }
        self.record(AcceptedEvent::Panel(award as u32));
        self.record(AcceptedEvent::Spark(Spark::Panel {
            at: position,
            points: award as u32,
            destroyed,
            row,
            volley: self.volley,
        }));
        if crate::quips::is_volley_milestone(self.volley) {
            self.record(AcceptedEvent::Spark(Spark::Volley(self.volley)));
        }
        if destroyed && self.remaining() == 1 {
            let last = self.blocks.iter().find(|b| b.health > 0).map(|b| b.center);
            if let Some(at) = last {
                self.record(AcceptedEvent::Spark(Spark::LastPanel { at }));
            }
        }
        if destroyed {
            self.shatter(index, row, reinforced);
        } else {
            self.emit(position, [0.82, 0.93, 0.92, 1.0], 6);
        }
    }

    /// Panel-colored shards that tumble out and fall; purely cosmetic.
    fn shatter(&mut self, index: usize, row: usize, reinforced: bool) {
        if self.policy.reduced_motion() {
            return;
        }
        let block = &self.blocks[index];
        let (center, half) = (block.center, block.half);
        let color = panel_color(row);
        let color = [color[0], color[1], color[2], 1.];
        let count = ((if reinforced { 22. } else { 16. }) * self.policy.intensity()) as usize;
        let seed = self.hits.wrapping_mul(2_654_435_761);
        for i in 0..count {
            let hash = seed
                .wrapping_add((i as u32).wrapping_mul(0x9e37_79b9))
                .rotate_left(i as u32 % 13);
            let fx = (hash % 1000) as f32 / 1000. * 2. - 1.;
            let fy = (hash / 1000 % 1000) as f32 / 1000. * 2. - 1.;
            let _ = self.particles.spawn(ParticleDesc {
                position: [center[0] + fx * half[0], center[1] + fy * half[1], 0.],
                velocity: [
                    fx * 120. + self.velocity[0] * 0.12,
                    -40. - fy.abs() * 90.,
                    0.,
                ],
                acceleration: [0., 420., 0.],
                lifetime: 0.45 + (i % 5) as f32 * 0.06,
                start_color: color,
                end_color: [color[0] * 0.6, color[1] * 0.6, color[2] * 0.6, 0.],
                start_size: 2. + (i % 3) as f32,
                end_size: 1.,
            });
        }
        self.emit(center, [1., 0.95, 0.80, 1.], 8);
    }
    fn emit(&mut self, position: [f32; 2], color: [f32; 4], count: usize) {
        if self.policy.reduced_motion() {
            return;
        }
        let count = (count as f32 * self.policy.intensity()) as usize;
        for i in 0..count {
            let angle = i as f32 / count as f32 * std::f32::consts::TAU + self.time;
            let speed = 35.0 + (i % 5) as f32 * 16.0;
            let _ = self.particles.spawn(ParticleDesc {
                position: [position[0], position[1], 0.0],
                velocity: [angle.cos() * speed, angle.sin() * speed, 0.0],
                acceleration: [0.0, 65.0, 0.0],
                lifetime: 0.28 + (i % 4) as f32 * 0.08,
                start_color: color,
                end_color: [color[0], color[1], color[2], 0.0],
                start_size: 3.0,
                end_size: 1.0,
            });
        }
    }
    fn update_drops(&mut self, dt: f32) {
        let mut caught = [None; 8];
        let mut count = 0;
        self.drops.retain_mut(|drop| {
            drop.position[1] += 130.0 * dt;
            if drop.position[1] <= PADDLE_Y + 12. {
                let motion = ferravex_engine::gameplay::pickup::Magnet {
                    radius: 100.,
                    collect_radius: 0.,
                    speed: 180.,
                }
                .step(
                    ferravex_engine::math::Vec2::from_array(drop.position),
                    ferravex_engine::math::Vec2::new(self.paddle_x, PADDLE_Y),
                    dt,
                )
                .unwrap();
                drop.position = motion.position.to_array();
            }
            if (drop.position[1] - PADDLE_Y).abs() < 12.0
                && (drop.position[0] - self.paddle_x).abs() < self.paddle_half + 8.0
            {
                if count < caught.len() {
                    caught[count] = Some((drop.kind, drop.position));
                    count += 1;
                }
                return false;
            }
            drop.position[1] < 680.0
        });
        for (kind, at) in caught.into_iter().flatten() {
            self.score += 50;
            match kind {
                Pickup::Wide => {
                    self.resize_paddle(67.0);
                    self.wide_time = 9.0;
                    self.record(AcceptedEvent::Wide);
                }
                Pickup::Split => {
                    let speed = self.speed();
                    for angle in [-0.42_f32, 0.42] {
                        if self.extra_balls.len() < MAX_EXTRA_BALLS {
                            self.extra_balls.push(Ball {
                                position: [self.paddle_x, PADDLE_Y - 8. - BALL_RADIUS - 2.],
                                velocity: [angle.sin() * speed, -angle.cos() * speed],
                            });
                        }
                    }
                    self.record(AcceptedEvent::Split);
                }
                Pickup::Sticky => {
                    self.sticky = STICKY_CATCHES;
                    self.record(AcceptedEvent::Sticky);
                }
            }
            self.record(AcceptedEvent::Spark(Spark::Pickup { kind, at }));
            self.emit(at, [0.35, 0.85, 0.79, 1.], 12);
        }
    }
}
/// The authored panel color for a row, shared by the raster and shard bursts.
pub(crate) fn panel_color(row: usize) -> [f32; 3] {
    let [r, g, b] = match row {
        0 => [211, 173, 117],
        1 => [183, 153, 111],
        2 => [139, 156, 141],
        3 => [100, 157, 152],
        _ => [80, 133, 140],
    };
    [r as f32 / 255., g as f32 / 255., b as f32 / 255.]
}
fn reflect_x(x: f32) -> f32 {
    let min = LEFT + BALL_RADIUS;
    let max = RIGHT - BALL_RADIUS;
    let width = max - min;
    let phase = (x - min).rem_euclid(width * 2.0);
    min + if phase > width {
        width * 2.0 - phase
    } else {
        phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_cosmetics_leave_the_complete_autoplay_run_unchanged() {
        let mut standard = Game::new();
        let mut reduced = Game::new();
        reduced.apply_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
        for _ in 0..120 * 600 {
            let input = standard.autoplay();
            standard.tick(input, 1. / 120.);
            reduced.tick(input, 1. / 120.);
            assert_eq!(
                (
                    standard.phase,
                    standard.score,
                    standard.lives,
                    standard.ball,
                    standard.velocity,
                    standard.clears,
                    standard.saves
                ),
                (
                    reduced.phase,
                    reduced.score,
                    reduced.lives,
                    reduced.ball,
                    reduced.velocity,
                    reduced.clears,
                    reduced.saves
                )
            );
            assert_eq!(reduced.particles.len(), 0);
            assert!(reduced.trail.is_empty());
            assert!(reduced.blocks.iter().all(|b| b.flash == 0.));
            if matches!(standard.phase, Phase::Won | Phase::Lost) {
                break;
            }
        }
        assert_eq!(reduced.phase, Phase::Won);
        assert_eq!(reduced.clears, 70);
        assert_eq!(standard.events.len(), 32);
        assert_eq!(reduced.events.len(), 32);
        assert!(reduced.events.dropped() > 0); // unconsumed presentation stays bounded
    }

    #[test]
    fn new_effects_stay_disabled_until_reenabled_and_zero_intensity_retires_both_channels() {
        let mut game = Game::new();
        game.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        game.blocks[0].health = 2;
        game.hit_block(0);
        assert_eq!(game.blocks[0].flash, 0.);
        assert!(!game.particles.is_empty());
        game.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        game.blocks[1].health = 2;
        game.hit_block(1);
        assert!(game.blocks[1].flash > 0.);
        assert_eq!(game.particles.len(), 0);
        game.apply_policy(FeedbackPolicy::new(false, false, 0.).unwrap());
        game.hit_block(1);
        assert!(game.blocks.iter().all(|b| b.flash == 0.));
        assert_eq!(game.particles.len(), 0);
        game.apply_policy(FeedbackPolicy::default());
        assert!(game.blocks.iter().all(|b| b.flash == 0.));
        assert_eq!(game.particles.len(), 0);
        game.hit_block(2);
        assert!(!game.particles.is_empty());
    }

    #[test]
    fn life_loss_repositions_the_serve_and_feedback_expires_without_changing_lives() {
        let mut game = Game::new();
        game.phase = Phase::Playing;
        game.ball = [100., 676.];
        game.velocity = [0., 325.];
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.feedback(), Some(Feedback::LifeLost));
        assert_eq!(game.ball, [game.paddle_x, PADDLE_Y - 15.]);
        assert_eq!((game.lives, game.phase), (2, Phase::Ready));
        assert_eq!(game.serve_charge.power(), 0.);
        for _ in 0..301 {
            game.tick(Input::default(), 1. / 120.);
        }
        assert_eq!(game.feedback(), None);
        assert_eq!((game.lives, game.phase), (2, Phase::Ready));
    }

    #[test]
    fn sector_clear_feedback_counts_down_to_a_fresh_reinforced_field() {
        let mut game = Game::new();
        for index in 0..35 {
            game.hit_block(index);
        }
        game.phase = Phase::Playing;
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.feedback(), Some(Feedback::SectorCleared));
        assert_eq!(game.phase, Phase::Transition);
        assert_eq!(game.transition_remaining(), 1.4);
        let score = game.score;
        for _ in 0..169 {
            game.tick(Input::default(), 1. / 120.);
        }
        assert_eq!(
            (game.sector, game.phase, game.remaining()),
            (2, Phase::Ready, 35)
        );
        assert_eq!(game.transition_remaining(), 0.);
        assert_eq!(game.score, score);
        assert!(game.blocks.iter().any(|b| b.health == 2));
        assert_eq!(game.feedback(), None);
    }

    #[test]
    fn moving_paddle_adds_english_and_shallow_wall_bounces_remain_playable() {
        for axis in [-1., 1.] {
            let mut game = Game::new();
            game.phase = Phase::Playing;
            game.ball = [320. + axis * 590. / 120., PADDLE_Y - 8. - BALL_RADIUS - 1.];
            game.velocity = [0., 325.];
            game.tick(
                Input {
                    axis,
                    ..Default::default()
                },
                1. / 120.,
            );
            assert!(game.velocity[1] < 0. && game.velocity[0] * axis > 0.);
            assert!((game.velocity[0].hypot(game.velocity[1]) - game.speed()).abs() < 0.01);
        }
        let mut game = Game::new();
        game.phase = Phase::Playing;
        game.ball = [RIGHT - BALL_RADIUS - 0.5, 500.];
        game.velocity = [325., 0.01];
        game.tick(Input::default(), 1. / 120.);
        assert!(game.velocity[0] < 0. && game.velocity[1].abs() >= 325. * 0.179);
    }

    #[test]
    fn swept_ball_destroys_thin_block_at_high_speed_without_tunneling() {
        let mut g = Game::new();
        g.phase = Phase::Playing;
        g.ball = [80.0, 380.0];
        g.velocity = [0.0, -12000.0];
        let before = g.remaining();
        g.tick(Input::default(), 1.0 / 120.0);
        assert!(g.remaining() < before);
        assert!(g.velocity[1] > 0.0);
        assert!(g.ccd_impacts > 0);
    }
    #[test]
    fn autoplay_clears_both_sectors_through_normal_controls() {
        let mut g = Game::new();
        for _ in 0..120 * 600 {
            let input = g.autoplay();
            g.tick(input, 1.0 / 120.0);
            if matches!(g.phase, Phase::Won | Phase::Lost) {
                break;
            }
        }
        assert_eq!(
            g.phase,
            Phase::Won,
            "sector {}, remaining {}, lives {}, time {}",
            g.sector,
            g.remaining(),
            g.lives,
            g.time
        );
        assert_eq!(g.clears, 70);
        assert!(g.ccd_impacts > 70);
        assert!(g.score >= 7000);
    }
    #[test]
    fn misses_consume_lives_and_restart_restores_world() {
        let mut g = Game::new();
        for expected in [2, 1, 0] {
            g.phase = Phase::Playing;
            g.ball = [100.0, 676.0];
            g.velocity = [0.0, 325.0];
            g.tick(Input::default(), 1.0 / 120.0);
            assert_eq!(g.lives, expected);
        }
        assert_eq!(g.phase, Phase::Lost);
        g.tick(
            Input {
                restart: true,
                ..Input::default()
            },
            1.0 / 120.0,
        );
        assert_eq!(g.phase, Phase::Ready);
        assert_eq!(g.lives, 3);
        assert_eq!(g.remaining(), 35);
        assert_eq!(g.score, 0);
    }
    #[test]
    fn pickups_expand_paddle_and_expire_without_invalid_handles() {
        let mut g = Game::new();
        g.phase = Phase::Playing;
        g.ball = [320.0, 500.0];
        g.velocity = [0.0, -325.0];
        g.drops.push(Drop {
            position: [g.paddle_x, PADDLE_Y - 2.0],
            kind: Pickup::Wide,
        });
        g.tick(Input::default(), 1.0 / 120.0);
        assert_eq!(g.paddle_half, 67.0);
        assert!(g.world.body(g.paddle_body).is_some());
        g.wide_time = 0.001;
        g.tick(Input::default(), 1.0 / 120.0);
        assert_eq!(g.paddle_half, PADDLE_HALF);
    }
}

#[cfg(test)]
mod chain_loop_tests {
    use super::*;
    #[test]
    fn successive_panels_in_a_volley_build_bonus_points() {
        let mut g = Game::new();
        for i in 0..4 {
            g.hit_block(i);
        }
        assert_eq!(g.score, 500);
        assert_eq!(g.chain.multiplier(), 2);
        g.chain.advance(5.).unwrap();
        g.hit_block(4);
        assert_eq!(g.score, 600);
        assert_eq!(Game::new().score, 0);
    }
}

#[cfg(test)]
mod reward_loop_tests {
    use super::*;
    #[test]
    fn actual_panel_destruction_never_leaves_more_than_five_unrewarded_clears() {
        let mut g = Game::new();
        let mut drought = 0;
        let mut drops = 0;
        for index in 0..35 {
            g.hit_block(index);
            if g.drops.is_empty() {
                drought += 1;
                assert!(drought < 6);
            } else {
                drought = 0;
                drops += 1;
                g.drops.clear();
            }
        }
        assert!(drops >= 5);
        let mut replay = Game::new();
        for index in 0..35 {
            replay.hit_block(index);
            replay.drops.clear();
        }
        assert_eq!(g.drop_pity.misses(), replay.drop_pity.misses());
    }
}

#[cfg(test)]
mod drop_magnet_tests {
    use super::*;
    #[test]
    fn nearby_capsules_follow_the_paddle_but_distant_ones_stay_on_course() {
        let mut g = Game::new();
        g.drops.push(Drop {
            position: [g.paddle_x + 65., PADDLE_Y - 60.],
            kind: Pickup::Wide,
        });
        let mut caught = false;
        for _ in 0..120 {
            g.update_drops(1. / 120.);
            caught |= g.wide_time > 0.;
        }
        assert!(caught);
        assert_eq!(g.paddle_half, 67.);
        g.drops.push(Drop {
            position: [LEFT + 10., 400.],
            kind: Pickup::Wide,
        });
        let x = g.drops[0].position[0];
        g.update_drops(0.1);
        assert_eq!(g.drops[0].position[0], x);
    }
}

#[cfg(test)]
mod ball_save_tests {
    use super::*;
    #[test]
    fn a_real_launch_gets_one_opening_save_without_making_late_misses_free() {
        let mut g = Game::new();
        g.tick(
            Input {
                launch: true,
                ..Input::default()
            },
            1. / 120.,
        );
        g.ball = [100., 676.];
        g.velocity = [0., 325.];
        g.tick(Input::default(), 1. / 120.);
        assert_eq!((g.lives, g.saves), (3, 1));
        assert_eq!(g.ball_save, 0.);
        assert!(g.velocity[1] < 0.);
        g.ball = [100., 676.];
        g.velocity = [0., 325.];
        g.tick(Input::default(), 1. / 120.);
        assert_eq!(g.lives, 2);
        g.tick(
            Input {
                launch: true,
                ..Input::default()
            },
            1. / 120.,
        );
        g.ball_save = 0.;
        g.ball = [100., 676.];
        g.velocity = [0., 325.];
        g.tick(Input::default(), 1. / 120.);
        assert_eq!(g.lives, 1);
    }
}

#[cfg(test)]
mod charge_serve_tests {
    use super::*;
    #[test]
    fn waiting_charges_a_real_serve_and_full_power_breaks_reinforced_panels() {
        let mut g = Game::new();
        g.sector = 2;
        g.build_sector();
        for _ in 0..240 {
            g.tick(Input::default(), 1. / 120.);
        }
        assert_eq!(g.serve_charge.power(), 1.);
        g.tick(
            Input {
                launch: true,
                ..Input::default()
            },
            1. / 120.,
        );
        assert!(g.charged_volley);
        assert_eq!(g.serve_power, 1.);
        assert!((g.velocity[0].hypot(g.velocity[1]) - g.speed() * 1.2).abs() < 0.01);
        assert_eq!(g.blocks[0].health, 2);
        g.hit_block(0);
        assert_eq!(g.blocks[0].health, 0);
        g.ball = [g.paddle_x, PADDLE_Y - 8. - BALL_RADIUS - 1.];
        g.velocity = [0., 350.];
        g.tick(Input::default(), 1. / 120.);
        assert!(!g.charged_volley);
    }
}

#[cfg(test)]
mod precision_loop_tests {
    use super::*;
    fn paddle_return(g: &mut Game, offset: f32) {
        g.phase = Phase::Playing;
        g.ball = [
            g.paddle_x + offset * g.paddle_half,
            PADDLE_Y - 8. - BALL_RADIUS - 1.,
        ];
        g.velocity = [0., 325.];
        g.tick(Input::default(), 1. / 120.);
    }
    #[test]
    fn actual_center_returns_reward_progress_but_empty_bounces_cannot_farm() {
        let mut g = Game::new();
        for (index, expected) in [(0, 50), (1, 100)] {
            g.hit_block(index);
            let score = g.score;
            paddle_return(&mut g, 0.);
            assert_eq!(g.score - score, expected);
            assert!(g.velocity[1] < 0.);
        }
        let score = g.score;
        paddle_return(&mut g, 0.);
        assert_eq!(g.score, score);
        g.hit_block(2);
        let score = g.score;
        paddle_return(&mut g, 0.7);
        assert_eq!(g.score, score);
        assert_eq!(g.precision.streak(), 0);
        assert_eq!(g.precision.best(), 2);
    }
}

#[cfg(test)]
mod polish_tests {
    use super::*;
    const DT: f32 = 1. / 120.;

    fn catch(g: &mut Game, kind: Pickup) {
        g.phase = Phase::Playing;
        g.drops.push(Drop {
            position: [g.paddle_x, PADDLE_Y - 2.],
            kind,
        });
        g.update_drops(DT);
    }

    fn sparks(g: &mut Game) -> Vec<Spark> {
        g.events
            .drain()
            .filter_map(|e| match e {
                AcceptedEvent::Spark(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn capsule_kinds_rotate_deterministically_and_replay_identically() {
        let kinds = |g: &mut Game| {
            let mut kinds = Vec::new();
            for index in 0..35 {
                g.hit_block(index);
                kinds.extend(g.drops.drain(..).map(|d| d.kind));
            }
            kinds
        };
        let first = kinds(&mut Game::new());
        assert_eq!(first, kinds(&mut Game::new()));
        assert!(first.len() >= 5);
        assert_eq!(first[..3], [Pickup::Wide, Pickup::Split, Pickup::Sticky]);
    }

    #[test]
    fn multi_ball_adds_free_balls_and_a_drained_primary_hands_over_without_a_life() {
        let mut g = Game::new();
        g.ball = [320., 400.];
        g.velocity = [0., -325.];
        catch(&mut g, Pickup::Split);
        assert_eq!(g.extra_balls.len(), 2);
        assert_eq!(g.score, 50);
        assert!(g.extra_balls.iter().all(|b| b.velocity[1] < 0.));
        // A bonus ball leaving the field costs nothing.
        g.extra_balls[0].position = [100., 676.];
        g.extra_balls[0].velocity = [0., 325.];
        g.tick(Input::default(), DT);
        assert_eq!((g.extra_balls.len(), g.lives), (1, 3));
        // The primary drains with no ball save: the survivor takes over.
        let survivor = g.extra_balls[0].position;
        g.ball_save = 0.;
        g.ball = [500., 676.];
        g.velocity = [0., 325.];
        g.tick(Input::default(), DT);
        assert_eq!((g.lives, g.phase), (3, Phase::Playing));
        assert!(g.extra_balls.is_empty());
        assert!((g.ball[1] - survivor[1]).abs() < 10.);
        assert!(sparks(&mut g).contains(&Spark::Fizzle));
        // Capped, and cleared by a lost life.
        for _ in 0..4 {
            catch(&mut g, Pickup::Split);
        }
        assert_eq!(g.extra_balls.len(), MAX_EXTRA_BALLS);
        g.extra_balls.clear();
        g.ball = [500., 676.];
        g.velocity = [0., 325.];
        g.tick(Input::default(), DT);
        assert_eq!(g.lives, 2);
    }

    #[test]
    fn sticky_paddle_catches_rides_and_releases_on_launch_or_timer() {
        let mut g = Game::new();
        catch(&mut g, Pickup::Sticky);
        assert_eq!(g.sticky, STICKY_CATCHES);
        let paddle_return = |g: &mut Game| {
            g.ball = [g.paddle_x + 12., PADDLE_Y - 8. - BALL_RADIUS - 1.];
            g.velocity = [0., 325.];
            g.tick(Input::default(), DT);
        };
        paddle_return(&mut g);
        let held = g.held.expect("caught");
        assert_eq!(g.velocity, [0.; 2]);
        assert!(held.velocity[1] < 0. && held.velocity[0] > 0.);
        // The ball rides the moving paddle at its catch offset.
        g.tick(
            Input {
                axis: 1.,
                ..Input::default()
            },
            DT,
        );
        assert!((g.ball[0] - g.paddle_x - held.offset).abs() < 0.01);
        assert!(g.held.is_some());
        g.tick(
            Input {
                launch: true,
                ..Input::default()
            },
            DT,
        );
        assert!(g.held.is_none() && g.velocity == held.velocity);
        assert_eq!(g.phase, Phase::Playing);
        // Waiting also releases; after three catches the paddle bounces again.
        paddle_return(&mut g);
        for _ in 0..(HOLD_SECONDS / DT) as usize + 2 {
            g.tick(Input::default(), DT);
        }
        assert!(g.held.is_none() && g.velocity[1] < 0.);
        paddle_return(&mut g);
        g.held = None;
        paddle_return(&mut g);
        assert!(g.held.is_none() && g.velocity[1] < 0.);
        assert_eq!(g.sticky, 0);
    }

    #[test]
    fn satellite_bonks_once_per_pass_without_deflecting_the_ball() {
        let mut g = Game::new();
        g.phase = Phase::Playing;
        assert_eq!(g.satellite(), None);
        g.time = SATELLITE_FIRST + 3.;
        let at = g.satellite().expect("passing");
        assert!(at[0] > LEFT && at[0] < RIGHT);
        g.ball = [at[0], at[1] + 4.];
        g.velocity = [0., -300.];
        g.check_satellite();
        assert_eq!((g.satellites, g.score), (1, SATELLITE_POINTS));
        assert_eq!(g.velocity, [0., -300.]);
        assert_eq!(g.satellite(), None);
        g.check_satellite();
        assert_eq!(g.satellites, 1);
        // The next pass is fresh, and it flies the other way.
        g.time += SATELLITE_PERIOD;
        let next = g.satellite().expect("next pass");
        assert!(next[0] > at[0]);
    }

    #[test]
    fn flawless_sectors_pay_a_bonus_but_a_lost_ball_forfeits_it() {
        for lose_first in [false, true] {
            let mut g = Game::new();
            if lose_first {
                g.phase = Phase::Playing;
                g.ball = [100., 676.];
                g.velocity = [0., 325.];
                g.tick(Input::default(), DT);
            }
            for index in 0..35 {
                g.hit_block(index);
            }
            g.phase = Phase::Playing;
            let score = g.score;
            g.tick(Input::default(), DT);
            assert_eq!(g.phase, Phase::Transition);
            let bonus = if lose_first { 0 } else { FLAWLESS_POINTS };
            assert_eq!(g.score - score, bonus);
            assert_eq!(g.flawless_sectors, u32::from(!lose_first));
        }
    }

    #[test]
    fn volleys_count_panel_hits_until_any_paddle_touch_and_announce_milestones() {
        let mut g = Game::new();
        for index in 0..5 {
            g.hit_block(index);
        }
        assert_eq!((g.volley, g.best_volley), (5, 5));
        let milestones: Vec<_> = sparks(&mut g)
            .into_iter()
            .filter_map(|s| match s {
                Spark::Volley(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(milestones, [3, 5]);
        g.phase = Phase::Playing;
        g.ball = [g.paddle_x, PADDLE_Y - 8. - BALL_RADIUS - 1.];
        g.velocity = [0., 325.];
        g.tick(Input::default(), DT);
        assert_eq!((g.volley, g.best_volley), (0, 5));
    }

    #[test]
    fn the_last_panel_is_announced_exactly_once() {
        let mut g = Game::new();
        for index in 0..34 {
            g.hit_block(index);
        }
        let last: Vec<_> = sparks(&mut g)
            .into_iter()
            .filter(|s| matches!(s, Spark::LastPanel { .. }))
            .collect();
        assert_eq!(
            last,
            [Spark::LastPanel {
                at: g.blocks[34].center
            }]
        );
    }
}
