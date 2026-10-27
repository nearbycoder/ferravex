//! Deterministic yard toys: BOWL, CHAOS, playful goals, strike scoring,
//! collapse detection, the pedestal secret and the lost-and-found net.
//!
//! Bowls and CHAOS are immediate yard edits, like drops. Everything else runs
//! inside accepted fixed ticks and reads the solver's observation-only impact
//! log. No clock, RNG or render cadence is involved, so the same inputs always
//! produce the same sparks, strikes and goal completions.
use super::{Body, MAX_BODIES, Sandbox};
use ferravex_physics::d3::{ColliderBuilder, RigidBodyBuilder, RigidBodyHandle};
use ferravex_render::audio::Surface;
use ferravex_render::prelude::*;

/// Crates from the authored stack that must topple for a STRIKE goal.
pub(super) const STRIKE_CRATES: usize = 10;
/// The authored stack: 5 layers of 3 x 2 crates.
pub(super) const STACK: usize = 30;
/// Rising flight height for AIRTIME.
pub(super) const AIRTIME_HEIGHT: f32 = 7.;
/// Solid bonks inside one simulated second for AVALANCHE.
pub(super) const AVALANCHE_BONKS: u32 = 50;
pub(super) const CHAOS_COUNT: usize = 16;
pub(super) const LOST_Y: f32 = -20.;
/// Bowling ball launch speed (m/s) and collider density.
pub(super) const BOWL_SPEED: f32 = 20.;
pub(super) const BOWL_DENSITY: f32 = 10.;
/// Simulated seconds after a bowl before its result is called.
const BOWL_WINDOW: u32 = 300;
const WINDOW: usize = 120;
const STILL_TICKS: u16 = 60;
const BONK_SPEED: f32 = 2.;
const SPARK_SPEED: f32 = 2.5;
const COLLAPSE_RUMBLE: f32 = 20.;
const COLLAPSE_COOLDOWN: f32 = 2.;
const SETTLE_TICKS: u32 = 300;
const MAX_SPARKS: usize = 12;
pub(super) const PEDESTALS: [Vec2; 4] = [
    Vec2::new(-9.4, -7.4),
    Vec2::new(-9.4, 7.4),
    Vec2::new(9.4, -7.4),
    Vec2::new(9.4, 7.4),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Goal {
    Strike,
    Airtime,
    Avalanche,
    CleanSweep,
}

impl Goal {
    pub const ALL: [Goal; 4] = [
        Goal::Strike,
        Goal::Airtime,
        Goal::Avalanche,
        Goal::CleanSweep,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Strike => "STRIKE",
            Self::Airtime => "AIRTIME",
            Self::Avalanche => "AVALANCHE",
            Self::CleanSweep => "CLEAN SWEEP",
        }
    }

    pub fn brief(self) -> &'static str {
        match self {
            Self::Strike => "Bowl into the big stack: topple 10 crates.",
            Self::Airtime => "Pulse something 7 m into the air.",
            Self::Avalanche => "50 bonks in one second. Try CHAOS.",
            Self::CleanSweep => "Topple all 30 stack crates. Orbit, bowl, repeat.",
        }
    }
}

/// One accepted impact worth a spark, in world meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Spark {
    pub at: Vec3,
    pub strength: f32,
    pub surface: Surface,
}

/// Accepted toy events since the start of the current lab advance.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct FunEvents {
    pub sparks: Vec<Spark>,
    /// Indices of bodies whose strong impacts deserve a hit flash.
    pub hit: Vec<usize>,
    pub collapse: Option<(Vec3, u32)>,
    pub goals: Vec<Goal>,
    pub secret: bool,
    pub chaos: usize,
    pub bowled: Option<Vec3>,
    /// A finished bowl: crates it toppled within its window.
    pub bowl_result: Option<usize>,
    pub returned: u32,
    pub toppled: Option<usize>,
}

impl FunEvents {
    fn clear(&mut self) {
        self.sparks.clear();
        self.hit.clear();
        self.collapse = None;
        self.goals.clear();
        self.secret = false;
        self.chaos = 0;
        self.bowled = None;
        self.bowl_result = None;
        self.returned = 0;
        self.toppled = None;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Stats {
    pub drops: u32,
    pub bowls: u32,
    pub pulses: u32,
    pub chaos: u32,
    pub collapses: u32,
    pub strikes: u32,
    pub bonks: u32,
    pub returns: u32,
}

#[derive(Debug)]
pub(super) struct Fun {
    pub goals: [bool; 4],
    pub airtime: f32,
    pub bonk_peak: u32,
    pub bonks_now: u32,
    /// Crates of this stack currently toppled.
    pub toppled: usize,
    /// Bowls used on the current stack, and the fewest for a clean sweep.
    pub stack_bowls: u32,
    pub sweep_best: Option<u32>,
    pub secret: bool,
    pub stats: Stats,
    pub events: FunEvents,
    pub chaos_serial: u32,
    window: [u8; WINDOW],
    cursor: usize,
    rumble: f32,
    rumble_center: Vec3,
    collapse_cooldown: f32,
    still: Vec<u16>,
    tethered: Vec<RigidBodyHandle>,
    stack: Vec<(RigidBodyHandle, Vec3)>,
    pending_bowl: Option<(u32, usize)>,
    swept: bool,
    scene_ticks: u32,
    touched: bool,
}

impl Default for Fun {
    fn default() -> Self {
        Self {
            goals: [false; 4],
            airtime: 0.,
            bonk_peak: 0,
            bonks_now: 0,
            toppled: 0,
            stack_bowls: 0,
            sweep_best: None,
            secret: false,
            stats: Stats::default(),
            events: FunEvents::default(),
            chaos_serial: 0,
            window: [0; WINDOW],
            cursor: 0,
            rumble: 0.,
            rumble_center: Vec3::ZERO,
            collapse_cooldown: 0.,
            still: Vec::with_capacity(MAX_BODIES),
            tethered: Vec::new(),
            stack: Vec::with_capacity(STACK),
            pending_bowl: None,
            swept: false,
            scene_ticks: 0,
            touched: false,
        }
    }
}

impl Fun {
    pub fn begin_frame(&mut self) {
        self.events.clear();
    }

    pub fn goals_done(&self) -> usize {
        self.goals.iter().filter(|done| **done).count()
    }

    pub fn current_goal(&self) -> Option<Goal> {
        Goal::ALL.into_iter().find(|goal| !self.goals[goal.index()])
    }

    pub fn progress(&self, goal: Goal) -> (f32, f32) {
        match goal {
            Goal::Strike => (self.toppled as f32, STRIKE_CRATES as f32),
            Goal::Airtime => (self.airtime, AIRTIME_HEIGHT),
            Goal::Avalanche => (self.bonk_peak as f32, AVALANCHE_BONKS as f32),
            Goal::CleanSweep => (self.toppled as f32, STACK as f32),
        }
    }

    /// CHAOS and bowls end the settling grace period at once.
    pub fn touch(&mut self) {
        self.touched = true;
    }

    pub fn live(&self) -> bool {
        self.touched || self.scene_ticks > SETTLE_TICKS
    }

    pub fn on_rebuild(&mut self, sandbox: &Sandbox) {
        self.window = [0; WINDOW];
        self.cursor = 0;
        self.bonks_now = 0;
        self.rumble = 0.;
        self.rumble_center = Vec3::ZERO;
        self.collapse_cooldown = 0.;
        self.still.clear();
        self.scene_ticks = 0;
        self.touched = false;
        self.toppled = 0;
        self.stack_bowls = 0;
        self.pending_bowl = None;
        self.swept = false;
        self.tethered.clear();
        for &joint in &sandbox.links {
            self.tethered.extend(sandbox.physics.joints[joint].bodies());
        }
        self.stack.clear();
        self.stack.extend(
            sandbox
                .bodies
                .iter()
                .take(STACK)
                .filter(|body| !body.ball && !body.compound)
                .map(|body| {
                    (
                        body.handle,
                        sandbox.physics.bodies[body.handle].translation(),
                    )
                }),
        );
    }

    fn free(&self, body: &Body) -> bool {
        !self.tethered.contains(&body.handle)
    }

    fn complete(&mut self, goal: Goal) {
        if !self.goals[goal.index()] {
            self.goals[goal.index()] = true;
            self.events.goals.push(goal);
        }
    }

    /// Record an accepted bowl so its toppled count can be called later.
    pub fn bowled(&mut self, at: Vec3) {
        self.touch();
        self.stats.bowls += 1;
        self.stack_bowls += 1;
        self.events.bowled = Some(at);
        self.pending_bowl = Some((self.scene_ticks, self.toppled));
    }

    pub fn after_tick(&mut self, sandbox: &mut Sandbox, dt: f32) {
        self.scene_ticks = self.scene_ticks.saturating_add(1);
        self.collapse_cooldown = (self.collapse_cooldown - dt).max(0.);
        let decay = (-3. * dt).exp();
        self.rumble *= decay;
        self.rumble_center *= decay;
        let live = self.live();

        let mut bonks = 0_u32;
        let mut sparks = std::mem::take(&mut self.events.sparks);
        for impact in sandbox.physics.impacts() {
            let index = |collider| {
                let handle = sandbox.physics.colliders[collider].parent();
                sandbox
                    .bodies
                    .iter()
                    .position(|body| Some(body.handle) == handle)
            };
            let (a, b) = (index(impact.collider1), index(impact.collider2));
            let Some(first) = a.or(b) else {
                continue;
            };
            if impact.approach_speed < BONK_SPEED || !impact.point.is_finite() {
                continue;
            }
            if live {
                bonks += 1;
                let weight = (impact.approach_speed / 4.).min(3.);
                self.rumble += weight;
                self.rumble_center += impact.point * weight;
            }
            if impact.approach_speed >= SPARK_SPEED {
                let body = &sandbox.bodies[first];
                let spark = Spark {
                    at: impact.point,
                    strength: impact.approach_speed,
                    surface: if body.compound {
                        Surface::Metal
                    } else if body.ball {
                        Surface::Stone
                    } else {
                        Surface::Wood
                    },
                };
                if sparks.len() < MAX_SPARKS {
                    sparks.push(spark);
                } else if let Some(weakest) = sparks
                    .iter_mut()
                    .min_by(|x, y| x.strength.total_cmp(&y.strength))
                    && weakest.strength < spark.strength
                {
                    *weakest = spark;
                }
                for index in [a, b].into_iter().flatten() {
                    if !self.events.hit.contains(&index) && self.events.hit.len() < 32 {
                        self.events.hit.push(index);
                    }
                }
            }
        }
        self.events.sparks = sparks;
        self.stats.bonks = self.stats.bonks.saturating_add(bonks);
        self.bonks_now = self.bonks_now + bonks.min(255) - u32::from(self.window[self.cursor]);
        self.window[self.cursor] = bonks.min(255) as u8;
        self.cursor = (self.cursor + 1) % WINDOW;
        self.bonk_peak = self.bonk_peak.max(self.bonks_now);
        if self.bonks_now >= AVALANCHE_BONKS {
            self.complete(Goal::Avalanche);
        }
        if self.rumble >= COLLAPSE_RUMBLE && self.collapse_cooldown == 0. {
            self.collapse_cooldown = COLLAPSE_COOLDOWN;
            self.stats.collapses += 1;
            let center = self.rumble_center / self.rumble.max(0.1);
            self.events.collapse = Some((center, self.rumble.round() as u32));
        }

        // Flight, the pedestal secret and the lost-and-found net.
        self.still.resize(sandbox.bodies.len(), 0);
        for (index, body) in sandbox.bodies.iter().enumerate() {
            if !self.free(body) {
                continue;
            }
            let rigid = &mut sandbox.physics.bodies[body.handle];
            let position = rigid.translation();
            if position.y < LOST_Y {
                rigid.set_translation(vec3(0., 10., 0.), true);
                rigid.set_linvel(Vec3::ZERO, true);
                rigid.set_angvel(Vec3::ZERO, true);
                self.events.returned += 1;
                self.stats.returns += 1;
                self.still[index] = 0;
                continue;
            }
            if rigid.linvel().y > 1.5 && (live || self.stats.pulses > 0) {
                self.airtime = self.airtime.max(position.y);
            }
            let calm = rigid.linvel().length() < 0.15 && rigid.angvel().length() < 0.3;
            self.still[index] = if calm {
                self.still[index].saturating_add(1)
            } else {
                0
            };
            if !self.secret
                && self.still[index] >= STILL_TICKS
                && (0.8..2.2).contains(&position.y)
                && PEDESTALS
                    .iter()
                    .any(|p| vec2(position.x, position.z).distance(*p) < 0.45)
            {
                self.secret = true;
                self.events.secret = true;
            }
        }
        if self.airtime >= AIRTIME_HEIGHT {
            self.complete(Goal::Airtime);
        }

        // Stack crates count as toppled once tipped or shoved off their spot.
        let toppled = self
            .stack
            .iter()
            .filter(|(handle, home)| {
                sandbox.physics.bodies.get(*handle).is_some_and(|rigid| {
                    let up = *rigid.rotation() * Vec3::Y;
                    let moved = vec2(
                        rigid.translation().x - home.x,
                        rigid.translation().z - home.z,
                    );
                    up.y.abs() < 0.8 || moved.length() > 0.9 || rigid.translation().y < home.y - 0.9
                })
            })
            .count();
        if toppled != self.toppled {
            self.toppled = toppled;
            self.events.toppled = Some(toppled);
        }
        if self.toppled >= STRIKE_CRATES && self.stack_bowls > 0 {
            self.complete(Goal::Strike);
        }
        if self.toppled == self.stack.len() && !self.stack.is_empty() && !self.swept {
            self.swept = true;
            if self.stack_bowls > 0 {
                self.sweep_best = Some(
                    self.sweep_best
                        .map_or(self.stack_bowls, |best| best.min(self.stack_bowls)),
                );
            }
            self.complete(Goal::CleanSweep);
        }
        if let Some((start, before)) = self.pending_bowl
            && self.scene_ticks.saturating_sub(start) >= BOWL_WINDOW
        {
            self.pending_bowl = None;
            let knocked = self.toppled.saturating_sub(before);
            if knocked >= 8 {
                self.stats.strikes += 1;
            }
            self.events.bowl_result = Some(knocked);
        }
    }
}

impl Sandbox {
    /// Observation-only impact log for sparks, bonks and collapse detection.
    pub(super) fn record_impacts(&mut self) {
        let mut settings = self.physics.impact_settings().unwrap_or_default();
        settings.capacity = 96;
        settings.min_approach_speed = 1.;
        self.physics
            .set_impact_settings(Some(settings))
            .expect("bounded yard impact log");
    }

    /// A heavy ball rolled from the camera's side of the yard at the stack.
    /// `yaw` is the orbit yaw; the ball starts inside the curbs.
    pub(super) fn bowl(&mut self, yaw: f32) -> Option<Vec3> {
        if self.bodies.len() >= MAX_BODIES || !yaw.is_finite() {
            return None;
        }
        let side = vec3(yaw.sin(), 0., yaw.cos());
        let reach = (8.6 / side.x.abs().max(1e-3))
            .min(6.6 / side.z.abs().max(1e-3))
            .min(8.6);
        let start = vec3(0., 0.62, 0.) + side * reach;
        // A shallow lob that arrives around the second layer of the stack.
        let aim = vec3(-0.58, 1.8, -0.09);
        let flat = vec3(aim.x - start.x, 0., aim.z - start.z);
        let time = flat.length() / BOWL_SPEED;
        let mut velocity = flat.normalize_or_zero() * BOWL_SPEED;
        velocity.y = (aim.y - start.y + 0.5 * 9.81 * time * time) / time.max(1e-3);
        let (handle, collider) = self.physics.insert(
            RigidBodyBuilder::dynamic()
                .translation(start)
                .linvel(velocity)
                .ccd_enabled(true),
            ColliderBuilder::ball(0.58).density(BOWL_DENSITY),
        );
        self.physics.colliders[collider].set_restitution(0.2);
        self.bodies.push(Body {
            handle,
            ball: true,
            compound: false,
            heavy: true,
            previous_position: start,
            previous_rotation: Quat::IDENTITY,
        });
        Some(start)
    }

    /// A deterministic ring of mixed crates and balls thrown in from above.
    pub(super) fn chaos(&mut self, serial: u32) -> usize {
        let room = MAX_BODIES
            .saturating_sub(self.bodies.len())
            .min(CHAOS_COUNT);
        for i in 0..room {
            let k = serial as usize * CHAOS_COUNT + i;
            let angle = i as f32 / CHAOS_COUNT as f32 * std::f32::consts::TAU + serial as f32 * 0.7;
            let radius = 3.2 + (k % 3) as f32 * 1.4;
            let position = vec3(
                angle.cos() * radius,
                9. + (k % 4) as f32 * 1.3,
                angle.sin() * radius * 0.8,
            );
            let ball = !k.is_multiple_of(3);
            self.spawn(position, ball);
            // A gentle swirl toward the center makes the pile-up converge.
            if let Some(body) = self.bodies.last()
                && body.previous_position == position
            {
                let swirl = vec3(-angle.sin(), 0., angle.cos()) * 2.5
                    - vec3(position.x, 0., position.z) * 0.25;
                self.physics.bodies[body.handle].set_linvel(swirl, true);
            }
        }
        room
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Commands, Lab};

    fn run(lab: &mut Lab, seconds: f32) {
        for _ in 0..(seconds * 120.) as u32 {
            lab.advance(1. / 120., Commands::default());
        }
    }

    #[test]
    fn untouched_yard_settles_without_goals_collapses_or_flight() {
        let mut lab = Lab::new();
        assert_eq!(lab.fun.stack.len(), STACK);
        let mut collapses = 0;
        for _ in 0..720 {
            lab.advance(1. / 120., Commands::default());
            collapses += u32::from(lab.fun.events.collapse.is_some());
            assert!(lab.fun.events.goals.is_empty());
        }
        assert_eq!(collapses, 0);
        assert_eq!(lab.fun.toppled, 0);
        assert!(lab.fun.bonk_peak < AVALANCHE_BONKS);
        assert!(lab.fun.airtime < AIRTIME_HEIGHT);
        assert_eq!(lab.fun.current_goal(), Some(Goal::Strike));
    }

    #[test]
    fn bowling_topples_the_stack_and_calls_the_result() {
        let mut lab = Lab::new();
        run(&mut lab, 3.);
        let frame = lab.advance(
            0.,
            Commands {
                bowl: true,
                ..Default::default()
            },
        );
        assert!(!frame.reset);
        assert!(lab.fun.events.bowled.is_some());
        assert!(lab.sandbox.bodies.last().unwrap().heavy);
        let mut result = None;
        for _ in 0..(BOWL_WINDOW + 10) {
            lab.advance(1. / 120., Commands::default());
            result = result.or(lab.fun.events.bowl_result);
        }
        let knocked = result.expect("every bowl gets a call");
        assert!(knocked >= STRIKE_CRATES, "knocked {knocked}");
        assert!(lab.fun.goals[Goal::Strike.index()]);
        assert_eq!(lab.fun.stats.bowls, 1);
    }

    #[test]
    fn orbiting_bowls_and_pulses_can_clean_sweep_the_stack() {
        let mut lab = Lab::new();
        run(&mut lab, 3.);
        let mut bowls = 0;
        for (round, yaw) in [0.68_f32, 2.25, 3.82, 5.39, 0.68, 2.25]
            .into_iter()
            .enumerate()
        {
            if lab.fun.goals[Goal::CleanSweep.index()] {
                break;
            }
            lab.orbit.yaw = yaw;
            lab.advance(
                0.,
                Commands {
                    bowl: true,
                    pulse: round >= 2,
                    ..Default::default()
                },
            );
            bowls += 1;
            run(&mut lab, 3.);
        }
        assert!(
            lab.fun.goals[Goal::CleanSweep.index()],
            "{}",
            lab.fun.toppled
        );
        assert_eq!(lab.fun.sweep_best, Some(bowls));
    }

    #[test]
    fn bowl_and_chaos_are_deterministic_bounded_immediate_edits() {
        let script = |lab: &mut Lab| {
            lab.advance(
                0.,
                Commands {
                    chaos: true,
                    bowl: true,
                    ..Default::default()
                },
            );
            run(lab, 2.);
        };
        let (mut a, mut b) = (Lab::new(), Lab::new());
        script(&mut a);
        script(&mut b);
        assert_eq!(a.sandbox.bodies.len(), 39 + CHAOS_COUNT + 1);
        let poses = |lab: &Lab| {
            lab.sandbox
                .bodies
                .iter()
                .map(|body| lab.sandbox.physics.bodies[body.handle].translation())
                .collect::<Vec<_>>()
        };
        assert_eq!(poses(&a), poses(&b));
        // Paused edits appear at once and stay frozen.
        let mut paused = Lab::new();
        paused.advance(
            0.,
            Commands {
                pause: true,
                chaos: true,
                ..Default::default()
            },
        );
        assert_eq!(paused.sandbox.bodies.len(), 39 + CHAOS_COUNT);
        assert_eq!(paused.session.tick(), 0);
        while paused.sandbox.drop_body(true) {}
        paused.advance(
            0.,
            Commands {
                chaos: true,
                bowl: true,
                ..Default::default()
            },
        );
        assert_eq!(paused.sandbox.bodies.len(), MAX_BODIES);
        assert!(paused.feedback.starts_with("Body limit reached"));
    }

    #[test]
    fn chaos_reaches_avalanche_and_a_collapse_callout() {
        let mut lab = Lab::new();
        lab.advance(
            0.,
            Commands {
                chaos: true,
                ..Default::default()
            },
        );
        let mut collapses = 0;
        let mut sparks = 0;
        for _ in 0..480 {
            lab.advance(1. / 120., Commands::default());
            collapses += u32::from(lab.fun.events.collapse.is_some());
            sparks += lab.fun.events.sparks.len();
        }
        assert!(sparks > 0 && collapses >= 1);
        assert!(
            lab.fun.bonk_peak >= AVALANCHE_BONKS,
            "{}",
            lab.fun.bonk_peak
        );
        assert!(lab.fun.goals[Goal::Avalanche.index()]);
    }

    #[test]
    fn pulse_launches_airtime_and_reset_keeps_session_goals() {
        let mut lab = Lab::new();
        run(&mut lab, 3.);
        lab.advance(
            1. / 120.,
            Commands {
                pulse: true,
                ..Default::default()
            },
        );
        run(&mut lab, 2.);
        assert!(lab.fun.airtime >= AIRTIME_HEIGHT, "{}", lab.fun.airtime);
        assert!(lab.fun.goals[Goal::Airtime.index()]);
        lab.advance(
            0.,
            Commands {
                reset: true,
                ..Default::default()
            },
        );
        assert!(lab.fun.goals[Goal::Airtime.index()]);
        assert_eq!((lab.fun.toppled, lab.fun.stack_bowls), (0, 0));
    }

    #[test]
    fn pedestal_secret_and_lost_and_found() {
        let mut lab = Lab::new();
        let ball = lab.sandbox.bodies[30].handle;
        lab.sandbox.physics.bodies[ball].set_translation(vec3(0., LOST_Y - 1., 0.), true);
        lab.advance(1. / 120., Commands::default());
        assert_eq!(lab.fun.events.returned, 1);
        assert!(lab.sandbox.physics.bodies[ball].translation().y > 9.);
        // A crate balanced on a corner pedestal unlocks the secret.
        lab.sandbox.spawn(vec3(9.4, 0.8 + 0.6, 7.4), false);
        let mut found = false;
        for _ in 0..240 {
            lab.advance(1. / 120., Commands::default());
            found |= lab.fun.events.secret;
        }
        assert!(found && lab.fun.secret);
    }
}
