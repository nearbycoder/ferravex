//! Deterministic sandbox toys: CHAOS drops, the domino run, playful goals,
//! collapse detection and the lost-and-found net.
//!
//! Everything here runs only inside accepted fixed ticks (or as a paused editor
//! operation) and reads the solver's observation-only impact log. Nothing uses
//! a clock, RNG or render cadence, so the same inputs always produce the same
//! sparks, collapses and goal completions. Presentation reads [`FunEvents`].
use super::{Body, MAX_BODIES, Sandbox, Shape};
use ferravex_physics::d2::{ColliderBuilder, RigidBodyBuilder, RigidBodyHandle};
use ferravex_render::audio::Surface;
use ferravex_render::prelude::*;
use ferravex_render::ui;

/// Launch something past this line (meters above the floor).
pub(super) const LIFTOFF_HEIGHT: f32 = 11.;
/// Rest something with its top edge at least this high.
pub(super) const TOWER_HEIGHT: f32 = 7.5;
/// Solid bonks inside one simulated second.
pub(super) const DEMOLITION_BONKS: u32 = 50;
/// Secret: something really leaves the building.
pub(super) const MOON_HEIGHT: f32 = 20.;
/// Bodies spawned by one CHAOS press (capacity permitting).
pub(super) const CHAOS_COUNT: usize = 14;
/// Below this, a body has left the lab and is returned above the floor.
pub(super) const LOST_Y: f32 = -30.;
pub(super) const DOMINO_COUNT: usize = 11;
pub(super) const DOMINO_HALF: Vec2 = Vec2::new(0.11, 0.8);
const WINDOW: usize = 120;
const STILL_TICKS: u16 = 45;
const BONK_SPEED: f32 = 2.;
const SPARK_SPEED: f32 = 2.5;
const COLLAPSE_RUMBLE: f32 = 20.;
/// The authored scene settles for this long unless the player acts first.
const SETTLE_TICKS: u32 = 300;
const COLLAPSE_COOLDOWN: f32 = 2.;
const MAX_SPARKS: usize = 12;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Scenario {
    #[default]
    Workshop,
    Dominoes,
}

impl Scenario {
    pub fn name(self) -> &'static str {
        match self {
            Self::Workshop => "Workshop",
            Self::Dominoes => "Domino run",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Goal {
    Liftoff,
    Skyscraper,
    Demolition,
    DominoRun,
}

impl Goal {
    pub const ALL: [Goal; 4] = [
        Goal::Liftoff,
        Goal::Skyscraper,
        Goal::Demolition,
        Goal::DominoRun,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Liftoff => "Liftoff",
            Self::Skyscraper => "Skyscraper",
            Self::Demolition => "Demolition",
            Self::DominoRun => "Domino run",
        }
    }

    pub fn brief(self) -> &'static str {
        match self {
            Self::Liftoff => "Pulse something past the 11 m line.",
            Self::Skyscraper => "Rest something 7.5 m up. Gravity disagrees.",
            Self::Demolition => "50 bonks in one second. Try CHAOS.",
            Self::DominoRun => "Topple all 11 dominoes in the domino run.",
        }
    }
}

/// One accepted impact worth a spark, in world meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Spark {
    pub at: Vec2,
    pub strength: f32,
    pub surface: Surface,
}

/// Accepted toy events since the start of the current `Lab` update.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct FunEvents {
    pub sparks: Vec<Spark>,
    /// Bodies whose strong impacts deserve a hit flash this frame.
    pub hit: Vec<usize>,
    pub collapse: Option<(Vec2, u32)>,
    pub goals: Vec<Goal>,
    pub secret: bool,
    pub chaos: usize,
    /// A new session-best resting height worth announcing, and where.
    pub tower: Option<(Vec2, f32)>,
    pub returned: u32,
    pub dominoes_down: Option<usize>,
}

impl FunEvents {
    fn clear(&mut self) {
        self.sparks.clear();
        self.hit.clear();
        self.collapse = None;
        self.goals.clear();
        self.secret = false;
        self.chaos = 0;
        self.tower = None;
        self.returned = 0;
        self.dominoes_down = None;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Stats {
    pub drops: u32,
    pub pulses: u32,
    pub chaos: u32,
    pub collapses: u32,
    pub bonks: u32,
    pub returns: u32,
}

/// Session toy state owned by the lab beside the physics sandbox.
#[derive(Debug)]
pub(super) struct Fun {
    pub scenario: Scenario,
    pub goals: [bool; 4],
    /// Session-best resting top height (m).
    pub tower: f32,
    /// Session-best flight height (m).
    pub altitude: f32,
    /// Session-best bonks in one simulated second.
    pub bonk_peak: u32,
    /// Bonks in the most recent simulated second.
    pub bonks_now: u32,
    /// Fastest full domino run, in ticks.
    pub domino_best: Option<u32>,
    pub dominoes_down: usize,
    pub secret: bool,
    pub stats: Stats,
    pub events: FunEvents,
    pub pending_chaos: bool,
    pub chaos_serial: u32,
    window: [u8; WINDOW],
    cursor: usize,
    rumble: f32,
    rumble_center: Vec2,
    collapse_cooldown: f32,
    still: Vec<u16>,
    tethered: Vec<RigidBodyHandle>,
    dominoes: Vec<RigidBodyHandle>,
    scene_ticks: u32,
    domino_done: bool,
    /// Settling of the authored scene is not demolition. Bonk goals and
    /// collapse callouts start after a grace period, or at once with CHAOS.
    touched: bool,
}

impl Default for Fun {
    fn default() -> Self {
        Self {
            scenario: Scenario::Workshop,
            goals: [false; 4],
            tower: 0.,
            altitude: 0.,
            bonk_peak: 0,
            bonks_now: 0,
            domino_best: None,
            dominoes_down: 0,
            secret: false,
            stats: Stats::default(),
            events: FunEvents::default(),
            pending_chaos: false,
            chaos_serial: 0,
            window: [0; WINDOW],
            cursor: 0,
            rumble: 0.,
            rumble_center: Vec2::ZERO,
            collapse_cooldown: 0.,
            still: Vec::with_capacity(MAX_BODIES),
            tethered: Vec::new(),
            dominoes: Vec::new(),
            scene_ticks: 0,
            domino_done: false,
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

    /// The first unfinished goal, in teaching order.
    pub fn current_goal(&self) -> Option<Goal> {
        Goal::ALL.into_iter().find(|goal| !self.goals[goal.index()])
    }

    /// Session-best progress toward a goal as (value, target).
    pub fn progress(&self, goal: Goal) -> (f32, f32) {
        match goal {
            Goal::Liftoff => (self.altitude, LIFTOFF_HEIGHT),
            Goal::Skyscraper => (self.tower, TOWER_HEIGHT),
            Goal::Demolition => (self.bonk_peak as f32, DEMOLITION_BONKS as f32),
            Goal::DominoRun => (self.dominoes_down as f32, DOMINO_COUNT as f32),
        }
    }

    pub fn domino_count(&self) -> usize {
        self.dominoes.len()
    }

    /// A rebuilt scene keeps session goals/bests but discards per-scene tracking.
    pub fn on_rebuild(&mut self, sandbox: &Sandbox) {
        self.window = [0; WINDOW];
        self.cursor = 0;
        self.bonks_now = 0;
        self.rumble = 0.;
        self.rumble_center = Vec2::ZERO;
        self.collapse_cooldown = 0.;
        self.pending_chaos = false;
        self.still.clear();
        self.scene_ticks = 0;
        self.touched = false;
        self.dominoes_down = 0;
        self.domino_done = false;
        self.tethered.clear();
        for &joint in &sandbox.links {
            for body in sandbox.physics.joints[joint].bodies() {
                self.tethered.push(body);
            }
        }
        self.dominoes.clear();
        if self.scenario == Scenario::Dominoes {
            self.dominoes.extend(
                sandbox
                    .bodies
                    .iter()
                    .filter(|body| matches!(body.shape, Shape::Plank(_)))
                    .map(|body| body.handle),
            );
        }
    }

    /// CHAOS ends the settling grace period: everything after it counts.
    pub fn touch(&mut self) {
        self.touched = true;
    }

    /// Whether bonks count toward goals and collapse callouts yet.
    pub fn live(&self) -> bool {
        self.touched || self.scene_ticks > SETTLE_TICKS
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

    /// Observe the tick that just executed. Only the lost-and-found net edits
    /// physics, and only for bodies that already fell out of the lab.
    pub fn after_tick(&mut self, sandbox: &mut Sandbox, dt: f32) {
        self.scene_ticks = self.scene_ticks.saturating_add(1);
        self.collapse_cooldown = (self.collapse_cooldown - dt).max(0.);
        let decay = (-3. * dt).exp();
        self.rumble *= decay;
        self.rumble_center *= decay;

        // Solid impacts against at least one dynamic lab body.
        let live = self.live();
        let mut bonks = 0_u32;
        let mut sparks = std::mem::take(&mut self.events.sparks);
        for impact in sandbox.physics.impacts() {
            let parent = |collider| sandbox.physics.colliders[collider].parent();
            let index = |collider| {
                let handle = parent(collider);
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
            let at = impact.point.truncate();
            if live {
                bonks += 1;
                let weight = (impact.approach_speed / 4.).min(3.);
                self.rumble += weight;
                self.rumble_center += at * weight;
            }
            if impact.approach_speed >= SPARK_SPEED {
                let surface = surface(sandbox.bodies[first].shape);
                let spark = Spark {
                    at,
                    strength: impact.approach_speed,
                    surface,
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
        if self.bonks_now > self.bonk_peak {
            self.bonk_peak = self.bonks_now;
        }
        if self.bonks_now >= DEMOLITION_BONKS {
            self.complete(Goal::Demolition);
        }
        if self.rumble >= COLLAPSE_RUMBLE && self.collapse_cooldown == 0. {
            self.collapse_cooldown = COLLAPSE_COOLDOWN;
            self.stats.collapses += 1;
            let center = self.rumble_center / self.rumble.max(0.1);
            self.events.collapse = Some((center, self.rumble.round() as u32));
        }

        // Heights, resting stacks and the lost-and-found net.
        self.still.resize(sandbox.bodies.len(), 0);
        let mut best_flight = self.altitude;
        let mut best_rest: Option<(Vec2, f32)> = None;
        for (index, body) in sandbox.bodies.iter().enumerate() {
            if !self.free(body) {
                continue;
            }
            let rigid = &mut sandbox.physics.bodies[body.handle];
            let position = rigid.translation();
            if position.y < LOST_Y {
                rigid.set_translation(vec2(0., 12.), true);
                rigid.set_linvel(Vec2::ZERO, true);
                rigid.set_angvel(0., true);
                self.events.returned += 1;
                self.stats.returns += 1;
                self.still[index] = 0;
                continue;
            }
            // Only rising bodies count as flight, so CHAOS spawns and editor
            // placements high in the lab are not launches.
            if rigid.linvel().y > 1.5 && (live || self.stats.pulses > 0) {
                best_flight = best_flight.max(position.y);
            }
            let calm = rigid.linvel().length() < 0.15 && rigid.angvel().abs() < 0.3;
            self.still[index] = if calm {
                self.still[index].saturating_add(1)
            } else {
                0
            };
            if self.still[index] >= STILL_TICKS {
                let top = top_edge(body.shape, position, rigid.rotation().angle());
                if best_rest.is_none_or(|(_, height)| top > height) {
                    best_rest = Some((position, top));
                }
            }
        }
        if best_flight > self.altitude {
            self.altitude = best_flight;
        }
        if self.altitude >= LIFTOFF_HEIGHT {
            self.complete(Goal::Liftoff);
        }
        if self.altitude >= MOON_HEIGHT && !self.secret {
            self.secret = true;
            self.events.secret = true;
        }
        if let Some((at, top)) = best_rest
            && top > self.tower + 0.05
        {
            // Announce meaningful new heights above the authored pyramid only.
            if top > 6.5 && top >= self.tower + 0.4 {
                self.events.tower = Some((at, top));
            }
            self.tower = top;
            if top >= TOWER_HEIGHT {
                self.complete(Goal::Skyscraper);
            }
        }

        if !self.dominoes.is_empty() {
            let down = self
                .dominoes
                .iter()
                .filter(|&&handle| {
                    sandbox
                        .physics
                        .bodies
                        .get(handle)
                        .is_some_and(|rigid| rigid.rotation().angle().abs() > 0.7)
                })
                .count();
            if down != self.dominoes_down {
                self.dominoes_down = down;
                self.events.dominoes_down = Some(down);
            }
            if down == self.dominoes.len() && !self.domino_done {
                self.domino_done = true;
                self.domino_best = Some(
                    self.domino_best
                        .map_or(self.scene_ticks, |best| best.min(self.scene_ticks)),
                );
                self.complete(Goal::DominoRun);
            }
        }
    }

    /// Seconds for the last completed domino run in this scene, if any.
    pub fn domino_seconds(&self) -> Option<f32> {
        self.domino_best
            .map(|ticks| ticks as f32 / super::TICK_HZ as f32)
    }
}

pub(super) fn surface(shape: Shape) -> Surface {
    match shape {
        Shape::Dumbbell => Surface::Metal,
        Shape::Ball(_) => Surface::Stone,
        Shape::Box(_) | Shape::Plank(_) => Surface::Wood,
    }
}

fn top_edge(shape: Shape, position: Vec2, angle: f32) -> f32 {
    match shape {
        Shape::Ball(radius) => position.y + radius,
        Shape::Box(half) | Shape::Plank(half) => {
            let (sin, cos) = angle.sin_cos();
            position.y + half.x * sin.abs() + half.y * cos.abs()
        }
        Shape::Dumbbell => position.y + 0.28,
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
            .expect("bounded lab impact log");
    }

    pub(super) fn with_scenario(scenario: Scenario) -> Self {
        match scenario {
            Scenario::Workshop => Self::new(),
            Scenario::Dominoes => Self::dominoes(),
        }
    }

    /// Eleven planks across the floor and a kicker ball rolling off the left
    /// platform into the first one. The authored mechanisms keep swinging.
    fn dominoes() -> Self {
        let mut world = Self::shell();
        for index in 0..DOMINO_COUNT {
            let x = -4.5 + index as f32 * 0.9;
            world.insert_free(
                vec2(x, DOMINO_HALF.y),
                Shape::Plank(DOMINO_HALF),
                Color::from_rgba(236, 229, 211, 255),
                0.02,
                Vec2::ZERO,
            );
        }
        world.insert_free(
            vec2(-7.6, 1.6 + 0.45),
            Shape::Ball(0.44),
            ui::CYAN,
            0.42,
            vec2(5., 0.),
        );
        world.mechanisms();
        world
    }

    /// Insert a free dynamic body with continuous collision enabled.
    pub(super) fn insert_free(
        &mut self,
        position: Vec2,
        shape: Shape,
        color: Color,
        restitution: f32,
        velocity: Vec2,
    ) -> Option<RigidBodyHandle> {
        if self.bodies.len() >= MAX_BODIES || !position.is_finite() || !velocity.is_finite() {
            return None;
        }
        let collider = match shape {
            Shape::Ball(radius) => ColliderBuilder::ball(radius),
            Shape::Box(half) | Shape::Plank(half) => ColliderBuilder::cuboid(half.x, half.y),
            Shape::Dumbbell => return None,
        };
        let (handle, _) = self.physics.insert(
            RigidBodyBuilder::dynamic()
                .translation(position)
                .linvel(velocity)
                .ccd_enabled(true),
            collider.restitution(restitution),
        );
        self.bodies.push(Body {
            handle,
            shape,
            color,
            previous_position: position,
            previous_angle: 0.,
        });
        Some(handle)
    }

    /// A deterministic arc of mixed balls and crates thrown in from above.
    /// Returns the number actually spawned (bounded by the body cap).
    pub(super) fn chaos(&mut self, serial: u32) -> usize {
        let room = MAX_BODIES
            .saturating_sub(self.bodies.len())
            .min(CHAOS_COUNT);
        for i in 0..room {
            let k = serial as usize * CHAOS_COUNT + i;
            let x = -10.2 + 20.4 * (i as f32 + 0.5) / CHAOS_COUNT as f32;
            let y = 11.2 + ((k * 7) % 5) as f32 * 0.3;
            let velocity = vec2(((k * 13) % 7) as f32 - 3., -1.5 - (k % 3) as f32);
            let shape = if k.is_multiple_of(3) {
                Shape::Box(vec2(0.48, 0.48))
            } else {
                Shape::Ball(0.44)
            };
            let color = if k.is_multiple_of(3) {
                ui::AMBER
            } else if k.is_multiple_of(5) {
                Color::from_rgba(240, 120, 140, 255)
            } else {
                ui::CYAN
            };
            let restitution = if k.is_multiple_of(3) { 0.08 } else { 0.42 };
            self.insert_free(vec2(x, y), shape, color, restitution, velocity);
        }
        room
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lab, LabInput, TICK_HZ, ToyInput};

    fn run(lab: &mut Lab, seconds: f32) {
        for _ in 0..(seconds * 120.) as u32 {
            lab.update_toys(1. / TICK_HZ, LabInput::default(), ToyInput::default());
        }
    }

    #[test]
    fn workshop_settle_never_completes_goals_or_fakes_a_collapse_without_input() {
        let mut lab = Lab::new();
        let mut collapses = 0;
        for _ in 0..600 {
            lab.update_toys(1. / TICK_HZ, LabInput::default(), ToyInput::default());
            collapses += u32::from(lab.fun.events.collapse.is_some());
        }
        assert_eq!(lab.fun.goals, [false; 4]);
        assert!(lab.fun.tower > 5.5 && lab.fun.tower < TOWER_HEIGHT);
        assert!(lab.fun.altitude < LIFTOFF_HEIGHT);
        assert!(lab.fun.bonk_peak < DEMOLITION_BONKS);
        assert_eq!(collapses, 0);
        assert_eq!(lab.fun.current_goal(), Some(Goal::Liftoff));
    }

    #[test]
    fn chaos_is_deterministic_bounded_and_waits_for_a_running_tick() {
        let toys = ToyInput {
            chaos: true,
            ..Default::default()
        };
        let mut a = Lab::new();
        let mut b = Lab::new();
        a.update_toys(0., LabInput::default(), toys);
        assert_eq!(a.sandbox.bodies.len(), 34, "zero-tick frames only queue");
        a.update_toys(1. / TICK_HZ, LabInput::default(), ToyInput::default());
        assert_eq!(a.sandbox.bodies.len(), 34 + CHAOS_COUNT);
        assert_eq!(a.fun.stats.chaos, 1);
        b.update_toys(1. / TICK_HZ, LabInput::default(), toys);
        assert_eq!(b.sandbox.bodies.len(), 34 + CHAOS_COUNT);
        run(&mut a, 2.);
        run(&mut b, 2.);
        let pose = |lab: &Lab| {
            lab.sandbox
                .bodies
                .iter()
                .map(|body| lab.sandbox.physics.bodies[body.handle].translation())
                .collect::<Vec<_>>()
        };
        assert_eq!(pose(&a), pose(&b));
        // Paused CHAOS is an immediate editor placement that stays frozen.
        let mut paused = Lab::new();
        paused.session.set_paused(true);
        paused.update_toys(5., LabInput::default(), toys);
        assert_eq!(paused.sandbox.bodies.len(), 34 + CHAOS_COUNT);
        assert_eq!(paused.session.tick(), 0);
        // Capacity is respected.
        while paused.sandbox.spawn(vec2(0., 10.), true) {}
        paused.update_toys(0., LabInput::default(), toys);
        assert_eq!(paused.sandbox.bodies.len(), MAX_BODIES);
        assert!(paused.feedback.starts_with("Body limit reached"));
    }

    #[test]
    fn chaos_key_edges_are_read_only_on_actual_ticks_and_never_repeat() {
        let mut lab = Lab::new();
        let mut edge = true;
        let mut reads = 0;
        let mut actions = || {
            reads += 1;
            (crate::LabCommands::default(), std::mem::take(&mut edge))
        };
        let frame = lab.update_toys_with_actions(
            0.5 / TICK_HZ,
            LabInput::default(),
            ToyInput::default(),
            &mut actions,
        );
        assert_eq!(frame.steps, 0);
        assert_eq!(
            lab.sandbox.bodies.len(),
            34,
            "a zero-tick frame keeps the edge"
        );
        let frame = lab.update_toys_with_actions(
            3.5 / TICK_HZ,
            LabInput::default(),
            ToyInput::default(),
            &mut actions,
        );
        assert_eq!(frame.steps, 4);
        assert_eq!(reads, 4);
        assert_eq!(
            lab.sandbox.bodies.len(),
            34 + CHAOS_COUNT,
            "catch-up never repeats it"
        );
        assert_eq!(lab.fun.stats.chaos, 1);
        // Pause discards a queued UI CHAOS, like other stale requests.
        lab.update_toys(
            0.,
            LabInput::default(),
            ToyInput {
                chaos: true,
                ..Default::default()
            },
        );
        lab.update_toys(
            0.,
            LabInput {
                toggle_pause: true,
                ..Default::default()
            },
            ToyInput::default(),
        );
        assert!(!lab.fun.pending_chaos);
        assert_eq!(lab.sandbox.bodies.len(), 34 + CHAOS_COUNT);
    }

    #[test]
    fn chaos_and_pulse_reach_demolition_and_collapse_callouts() {
        let mut lab = Lab::new();
        let mut collapses = 0;
        let mut sparks = 0;
        lab.update_toys(
            1. / TICK_HZ,
            LabInput::default(),
            ToyInput {
                chaos: true,
                ..Default::default()
            },
        );
        for tick in 0..480 {
            let input = LabInput {
                pulse: (tick == 240).then_some(vec2(0., 1.)),
                ..Default::default()
            };
            lab.update_toys(1. / TICK_HZ, input, ToyInput::default());
            collapses += u32::from(lab.fun.events.collapse.is_some());
            sparks += lab.fun.events.sparks.len();
            assert!(lab.fun.events.sparks.len() <= MAX_SPARKS);
        }
        assert!(sparks > 0);
        assert!(collapses >= 1, "a CHAOS landing is a collapse moment");
        assert!(
            lab.fun.bonk_peak >= DEMOLITION_BONKS,
            "peak {}",
            lab.fun.bonk_peak
        );
        assert!(lab.fun.goals[Goal::Demolition.index()]);
    }

    #[test]
    fn pulses_launch_bodies_past_the_liftoff_line() {
        let mut lab = Lab::new();
        // A ball placed just above the floor, pulsed from directly beneath.
        lab.update_toys(
            1. / TICK_HZ,
            LabInput {
                drop: Some((vec2(3.4, 9.), false)),
                ..Default::default()
            },
            ToyInput::default(),
        );
        run(&mut lab, 2.5);
        let ball = lab.sandbox.bodies.last().unwrap().handle;
        let at = lab.sandbox.physics.bodies[ball].translation();
        lab.update_toys(
            1. / TICK_HZ,
            LabInput {
                pulse: Some(at - vec2(0., 0.3)),
                ..Default::default()
            },
            ToyInput::default(),
        );
        run(&mut lab, 2.);
        assert!(lab.fun.altitude >= LIFTOFF_HEIGHT, "{}", lab.fun.altitude);
        assert!(lab.fun.goals[Goal::Liftoff.index()]);
        assert_eq!(lab.fun.current_goal(), Some(Goal::Skyscraper));
    }

    #[test]
    fn stacking_crates_on_the_pyramid_reaches_the_skyscraper_goal() {
        let mut lab = Lab::new();
        run(&mut lab, 3.);
        let start = lab.fun.tower;
        for _ in 0..2 {
            lab.update_toys(
                1. / TICK_HZ,
                LabInput {
                    ball: Some(false),
                    drop: Some((vec2(-1.35, 9.5), false)),
                    ..Default::default()
                },
                ToyInput::default(),
            );
            run(&mut lab, 3.);
        }
        assert!(lab.fun.tower > start + 1.5, "{start} -> {}", lab.fun.tower);
        assert!(lab.fun.goals[Goal::Skyscraper.index()]);
    }

    #[test]
    fn domino_run_topples_every_plank_and_scene_toggle_round_trips() {
        let mut lab = Lab::new();
        let scene = ToyInput {
            scene: true,
            ..Default::default()
        };
        lab.update_toys(0., LabInput::default(), scene);
        assert_eq!(lab.fun.scenario, Scenario::Dominoes);
        assert_eq!(lab.fun.domino_count(), DOMINO_COUNT);
        assert_eq!(lab.session.tick(), 0);
        let mut announced = 0;
        for _ in 0..1200 {
            lab.update_toys(1. / TICK_HZ, LabInput::default(), ToyInput::default());
            announced += usize::from(lab.fun.events.dominoes_down.is_some());
        }
        assert_eq!(lab.fun.dominoes_down, DOMINO_COUNT);
        assert!(announced > 1);
        assert!(lab.fun.goals[Goal::DominoRun.index()]);
        let seconds = lab.fun.domino_seconds().unwrap();
        assert!(seconds > 0.5 && seconds < 10., "{seconds}");
        // RESET rebuilds the current scene; the toggle returns to the workshop.
        lab.update_toys(
            0.,
            LabInput {
                reset: true,
                ..Default::default()
            },
            ToyInput::default(),
        );
        assert_eq!(lab.fun.scenario, Scenario::Dominoes);
        assert_eq!(lab.fun.dominoes_down, 0);
        assert!(lab.fun.goals[Goal::DominoRun.index()], "goals persist");
        lab.update_toys(0., LabInput::default(), scene);
        assert_eq!(lab.fun.scenario, Scenario::Workshop);
        assert_eq!(lab.sandbox.bodies.len(), 34);
        assert_eq!(lab.fun.domino_count(), 0);
    }

    #[test]
    fn lost_bodies_return_and_moon_shots_unlock_the_secret() {
        let mut lab = Lab::new();
        lab.fun.touch();
        let ball = lab.sandbox.bodies[25].handle;
        lab.sandbox.physics.bodies[ball].set_translation(vec2(0., LOST_Y - 1.), true);
        lab.update_toys(1. / TICK_HZ, LabInput::default(), ToyInput::default());
        assert_eq!(lab.fun.events.returned, 1);
        assert!(lab.sandbox.physics.bodies[ball].translation().y > 11.);
        lab.sandbox.physics.bodies[ball].set_translation(vec2(0., MOON_HEIGHT + 1.), true);
        lab.sandbox.physics.bodies[ball].set_linvel(vec2(0., 3.), true);
        lab.update_toys(1. / TICK_HZ, LabInput::default(), ToyInput::default());
        assert!(lab.fun.events.secret && lab.fun.secret);
        lab.update_toys(1. / TICK_HZ, LabInput::default(), ToyInput::default());
        assert!(!lab.fun.events.secret, "the secret is announced once");
    }
}
