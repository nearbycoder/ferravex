//! Fixed-tick, renderer-independent submarine rescue rules.
//!
//! ```
//! use deep_pressure::Game;
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! assert_eq!(game.sortie(), 1);
//! game.tick(&Input { primary: true, ..Input::default() });
//! let initial = game.position();
//! for _ in 0..60 { game.tick(&Input { right: true, ..Input::default() }); }
//! assert!(game.position().x > initial.x + 40.0);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{HazardKind, MISSIONS, Mission, UPGRADE_COSTS, UPGRADE_NAMES};
use ferravex_arcade::{Cue, Input, Outcome, TICK_DT};
use ferravex_math::{Vec2, vec2};

pub(crate) const WORLD_W: f32 = 640.;
pub(crate) const WORLD_H: f32 = 400.;
pub(crate) const RADIUS: f32 = 11.;
pub(crate) const BASE: Vec2 = vec2(50., 50.);
/// Winch reach around an air bell and the slowest refused approach speed.
pub(crate) const WINCH_RANGE: f32 = 25.;
pub(crate) const DOCK_RANGE: f32 = 31.;
pub(crate) const WINCH_SPEED: f32 = 35.;
/// Seconds of air below which the boat raises its low-air alarm.
pub(crate) const LOW_AIR: f32 = 12.;
/// Ticks inside which a clean sortie still earns gold.
pub(crate) const GOLD_TICKS: u32 = 9_000;
/// Instrument-panel buttons that mirror X (ballast) and C (sonar).
pub(crate) const BALLAST_BUTTON: (f32, f32, f32, f32) = (716., 384., 102., 58.);
pub(crate) const SONAR_BUTTON: (f32, f32, f32, f32) = (826., 384., 102., 58.);
const NAV_W: usize = 32;
const NAV_H: usize = 20;
const NAV_CELLS: usize = NAV_W * NAV_H;
/// Longest guidance trail, in 20-unit chart cells.
pub(crate) const TRAIL: usize = 48;
/// Most presentation events one tick can record.
const EVENT_CAP: usize = 8;
/// What damaged the hull.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    Reef,
    Hazard(HazardKind),
}
/// One thing the latest tick accepted or refused, recorded only so the view
/// can make it felt where it happened. Rules never read these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Event {
    Launch,
    Fitted {
        index: usize,
        price: u32,
    },
    RefitRefused {
        index: usize,
        maxed: bool,
    },
    Winched {
        index: usize,
    },
    /// Winch or dock refused for speed beside the target at `at`.
    TooFast {
        at: Vec2,
    },
    /// Winch refused beside the air bell at `at` because every berth is taken.
    CabinFull {
        at: Vec2,
    },
    /// The dock landed `landed` people; `cleared` finished the sortie.
    Docked {
        landed: u32,
        cleared: bool,
    },
    Ballast {
        neutral: bool,
    },
    Sonar,
    /// The hull lost `loss` at chart point `at`.
    Impact {
        at: Vec2,
        loss: f32,
        cause: Cause,
    },
    LowAir,
    Wreck {
        hull: bool,
    },
    Recovered,
    NextSortie,
}
/// Bounded per-tick event list, cleared at the start of every tick.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Events {
    list: [Event; EVENT_CAP],
    len: usize,
}
impl Events {
    const fn new() -> Self {
        Self {
            list: [Event::NextSortie; EVENT_CAP],
            len: 0,
        }
    }
    fn push(&mut self, event: Event) {
        if self.len < EVENT_CAP {
            self.list[self.len] = event;
            self.len += 1;
        }
    }
    pub(crate) fn as_slice(&self) -> &[Event] {
        &self.list[..self.len]
    }
}
/// A guidance path along the pilot's navigation grid, in chart units.
pub(crate) struct Trail {
    pub points: [Vec2; TRAIL],
    pub len: usize,
    /// The same route length the pilot uses to budget air.
    pub distance: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Briefing,
    Diving,
    Debrief,
    Wreck,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RescueState {
    Waiting,
    Onboard,
    Safe,
}
/// One complete rescue career. All world resources and histories are bounded.
#[derive(Clone, Debug)]
pub struct Game {
    pub(crate) mission: usize,
    pub(crate) phase: Phase,
    pub(crate) position: Vec2,
    pub(crate) velocity: Vec2,
    pub(crate) facing: f32,
    pub(crate) oxygen: f32,
    pub(crate) hull: f32,
    pub(crate) upgrades: [u8; 3],
    pub(crate) divers: [RescueState; 6],
    pub(crate) credits: u32,
    pub(crate) score: u32,
    pub(crate) recovery: u8,
    pub(crate) selected: usize,
    pub(crate) balanced: bool,
    pub(crate) elapsed: u32,
    pub(crate) total_ticks: u64,
    pub(crate) sonar: u32,
    pub(crate) sonar_cooldown: u32,
    pub(crate) impact_cooldown: u32,
    pub(crate) impacts: u32,
    pub(crate) stage_impacts: u32,
    pub(crate) stage_credits: u32,
    pub(crate) stage_score: u32,
    pub(crate) rescued_before: usize,
    pub(crate) medals: [u8; 6],
    pub(crate) notice: String,
    pub(crate) failure: &'static str,
    /// Presentation-only record of the latest tick; see [`Event`].
    pub(crate) events: Events,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin at the nursery with two berths, 42 seconds of air and three recovery tugs.
    pub fn new() -> Self {
        Self {
            mission: 0,
            phase: Phase::Briefing,
            position: BASE,
            velocity: Vec2::ZERO,
            facing: 1.,
            oxygen: 42.,
            hull: 100.,
            upgrades: [0; 3],
            divers: [RescueState::Waiting; 6],
            credits: 20,
            score: 0,
            recovery: 3,
            selected: 0,
            balanced: false,
            elapsed: 0,
            total_ticks: 0,
            sonar: 0,
            sonar_cooldown: 0,
            impact_cooldown: 0,
            impacts: 0,
            stage_impacts: 0,
            stage_credits: 20,
            stage_score: 0,
            rescued_before: 0,
            medals: [0; 6],
            notice: "Read the chart. Space launches; 1 / 2 / 3 buys refits.".into(),
            failure: "",
            events: Events::new(),
        }
    }
    /// Current sortie number, one-based and always in 1..=6.
    pub fn sortie(&self) -> usize {
        self.mission + 1
    }
    /// Authoritative submarine position in the 640 by 400 simulation chart.
    pub fn position(&self) -> Vec2 {
        self.position
    }
    /// Seconds of breathing mixture left; dock to refill before this reaches zero.
    pub fn oxygen(&self) -> f32 {
        self.oxygen
    }
    /// Survivors safely landed across this career, excluding people still aboard.
    pub fn rescued(&self) -> usize {
        self.rescued_before + self.delivered()
    }
    /// Accepted-event score, including completed-sortie bonuses.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Terminal result. A recoverable wreck remains Playing until retried.
    pub fn outcome(&self) -> Outcome {
        if self.phase == Phase::Complete {
            Outcome::Won
        } else if self.phase == Phase::Wreck && self.recovery == 0 {
            Outcome::Lost
        } else {
            Outcome::Playing
        }
    }
    pub(crate) fn chart(&self) -> &'static Mission {
        &MISSIONS[self.mission]
    }
    pub(crate) fn max_air(&self) -> f32 {
        42. + 12. * f32::from(self.upgrades[0])
    }
    pub(crate) fn max_hull(&self) -> f32 {
        100. + 20. * f32::from(self.upgrades[1])
    }
    pub(crate) fn capacity(&self) -> usize {
        2 + usize::from(self.upgrades[2])
    }
    pub(crate) fn onboard(&self) -> usize {
        self.divers[..self.chart().divers.len()]
            .iter()
            .filter(|s| **s == RescueState::Onboard)
            .count()
    }
    pub(crate) fn delivered(&self) -> usize {
        self.divers[..self.chart().divers.len()]
            .iter()
            .filter(|s| **s == RescueState::Safe)
            .count()
    }
    pub(crate) fn depth(&self) -> u32 {
        self.chart().depth + (self.position.y * 2.).round() as u32
    }
    pub(crate) fn upgrade_cost(&self, index: usize) -> u32 {
        UPGRADE_COSTS[index] + u32::from(self.upgrades[index]) * 20
    }
    /// True while any diver of this sortie still waits in an air bell.
    pub(crate) fn waiting(&self) -> bool {
        self.divers[..self.chart().divers.len()].contains(&RescueState::Waiting)
    }
    /// The medal this sortie earns if its last survivor lands now.
    pub(crate) fn medal_pace(&self) -> u8 {
        if self.stage_impacts == 0 && self.elapsed < GOLD_TICKS {
            3
        } else if self.stage_impacts <= 4 {
            2
        } else {
            1
        }
    }
    /// Seconds of air the pilot keeps for a trip of `home_distance` chart
    /// units back to the dock: cruise time, passenger drain and a margin.
    pub(crate) fn air_reserve(&self, home_distance: f32) -> f32 {
        home_distance / 55. * (1.5 + self.onboard() as f32 * 0.18) + 8.
    }
    fn reset_dive(&mut self) {
        self.position = BASE;
        self.velocity = Vec2::ZERO;
        self.facing = 1.;
        self.oxygen = self.max_air();
        self.hull = self.max_hull();
        self.balanced = false;
        self.divers = [RescueState::Waiting; 6];
        self.elapsed = 0;
        self.sonar = 0;
        self.sonar_cooldown = 0;
        self.impact_cooldown = 0;
        self.stage_impacts = 0;
        self.failure = "";
    }
    fn launch(&mut self) -> Cue {
        self.reset_dive();
        self.stage_credits = self.credits;
        self.stage_score = self.score;
        self.phase = Phase::Diving;
        self.notice = "Arrows thrust. X sets ballast. Slow + Space winches or docks.".into();
        self.events.push(Event::Launch);
        Cue::Action
    }
    fn buy(&mut self, index: usize) -> Cue {
        if index >= 3 {
            return Cue::None;
        }
        let price = self.upgrade_cost(index);
        if self.upgrades[index] >= 2 {
            self.notice = "That refit is already at its maximum level.".into();
            self.events.push(Event::RefitRefused { index, maxed: true });
            return Cue::None;
        }
        if self.credits < price {
            self.notice = format!(
                "{} costs {} credits. Rescue people to earn pay.",
                UPGRADE_NAMES[index], price
            );
            self.events.push(Event::RefitRefused {
                index,
                maxed: false,
            });
            return Cue::None;
        }
        self.credits -= price;
        self.upgrades[index] += 1;
        self.oxygen = self.max_air();
        self.hull = self.max_hull();
        self.notice = format!(
            "{} fitted. Launch when you are ready.",
            UPGRADE_NAMES[index]
        );
        self.events.push(Event::Fitted { index, price });
        Cue::Good
    }
    fn recover(&mut self) -> Cue {
        if self.recovery == 0 {
            return Cue::None;
        }
        self.recovery -= 1;
        self.credits = self.stage_credits;
        self.score = self.stage_score;
        self.reset_dive();
        self.phase = Phase::Diving;
        self.notice = format!(
            "Sortie reset; {} tugs left. Earlier rescues and refits are safe.",
            self.recovery
        );
        self.events.push(Event::Recovered);
        Cue::Action
    }
    fn damage(&mut self, amount: f32, reason: &'static str) -> Cue {
        if self.impact_cooldown > 0 {
            return Cue::None;
        }
        self.hull = (self.hull - amount / (1. + 0.4 * f32::from(self.upgrades[1]))).max(0.);
        self.impact_cooldown = 50;
        self.stage_impacts += 1;
        self.impacts += 1;
        self.notice = reason.into();
        Cue::Bad
    }
    /// [`Self::damage`] that also records where an accepted hit landed.
    fn hit(&mut self, amount: f32, reason: &'static str, cause: Cause, at: Vec2) -> Cue {
        let before = self.hull;
        let cue = self.damage(amount, reason);
        if cue == Cue::Bad {
            self.events.push(Event::Impact {
                at,
                loss: before - self.hull,
                cause,
            });
        }
        cue
    }
    /// Air bell within winch reach of the boat, if a diver still waits there.
    fn bell_in_reach(&self) -> Option<Vec2> {
        self.chart()
            .divers
            .iter()
            .enumerate()
            .find(|(index, (point, _))| {
                self.divers[*index] == RescueState::Waiting
                    && self.position.distance(*point) < WINCH_RANGE
            })
            .map(|(_, (point, _))| *point)
    }
    /// True when docking now would land someone or restore air or hull.
    fn dock_has_work(&self) -> bool {
        self.onboard() > 0
            || self.oxygen <= self.max_air() - 0.1
            || self.hull <= self.max_hull() - 0.1
    }
    fn try_winch(&mut self) -> Cue {
        if self.velocity.length() > WINCH_SPEED {
            self.notice = "Too fast for the winch. Hold Space to brake, then tap it again.".into();
            // Feedback only beside something the tap could have served.
            if self.position.distance(BASE) < DOCK_RANGE {
                if self.dock_has_work() {
                    self.events.push(Event::TooFast { at: BASE });
                }
            } else if let Some(at) = self.bell_in_reach() {
                self.events.push(if self.onboard() >= self.capacity() {
                    Event::CabinFull { at }
                } else {
                    Event::TooFast { at }
                });
            }
            return Cue::None;
        }
        if self.position.distance(BASE) < DOCK_RANGE {
            let aboard = self.onboard();
            if aboard == 0
                && self.oxygen > self.max_air() - 0.1
                && self.hull > self.max_hull() - 0.1
            {
                return Cue::None;
            }
            let diver_count = self.chart().divers.len();
            for state in &mut self.divers[..diver_count] {
                if *state == RescueState::Onboard {
                    *state = RescueState::Safe;
                }
            }
            self.oxygen = self.max_air();
            self.hull = self.max_hull();
            self.velocity = Vec2::ZERO;
            self.credits += aboard as u32 * 12;
            self.score += aboard as u32 * 250;
            self.notice = if aboard > 0 {
                format!("{} survivors landed safely. Air and hull restored.", aboard)
            } else {
                "Dock serviced: air and hull restored. No rescue reward claimed.".into()
            };
            let cleared = self.delivered() == self.chart().divers.len();
            self.events.push(Event::Docked {
                landed: aboard as u32,
                cleared,
            });
            if cleared {
                let medal = self.medal_pace();
                self.medals[self.mission] = medal;
                self.score +=
                    400 + u32::from(medal) * 100 + GOLD_TICKS.saturating_sub(self.elapsed) / 30;
                self.phase = if self.mission == 5 {
                    Phase::Complete
                } else {
                    Phase::Debrief
                };
                return Cue::Win;
            }
            return if aboard > 0 { Cue::Good } else { Cue::Action };
        }
        if self.onboard() >= self.capacity() {
            self.notice = "Cabin full. Bring these survivors back to the yellow dock.".into();
            if let Some(at) = self.bell_in_reach() {
                self.events.push(Event::CabinFull { at });
            }
            return Cue::None;
        }
        for (index, (point, name)) in self.chart().divers.iter().enumerate() {
            if self.divers[index] == RescueState::Waiting
                && self.position.distance(*point) < WINCH_RANGE
            {
                self.divers[index] = RescueState::Onboard;
                self.notice = format!(
                    "{} aboard: {} / {} berths. Return to the dock to land survivors.",
                    name,
                    self.onboard(),
                    self.capacity()
                );
                self.events.push(Event::Winched { index });
                return Cue::Good;
            }
        }
        self.notice =
            "No air bell in winch range. Get close to a cyan beacon, then slow down.".into();
        Cue::None
    }
    fn solid(&self, p: Vec2, radius: f32) -> bool {
        p.x < radius
            || p.x > WORLD_W - radius
            || p.y < radius
            || p.y > WORLD_H - radius
            || self.chart().reefs.iter().any(|reef| reef.blocks(p, radius))
    }
    fn move_ship(&mut self, input: &Input) -> Cue {
        let direction = vec2(
            u8::from(input.right) as f32 - u8::from(input.left) as f32,
            u8::from(input.down) as f32 - u8::from(input.up) as f32,
        )
        .clamp_length_max(1.);
        let current = if self.position.y > 170. {
            self.chart().current
        } else {
            0.
        };
        self.velocity +=
            (direction * 210. + vec2(current, if self.balanced { 0. } else { -12. })) * TICK_DT;
        self.velocity *= 0.975;
        if input.primary_down {
            self.velocity *= 0.83;
        }
        let speed = 96. / (1. + self.onboard() as f32 * 0.075);
        self.velocity = self.velocity.clamp_length_max(speed);
        if self.velocity.x.abs() > 3. {
            self.facing = self.velocity.x.signum();
        }
        let old = self.position;
        let target = old + self.velocity * TICK_DT;
        let mut cue = Cue::None;
        if self.solid(vec2(target.x, old.y), RADIUS) {
            let impact = self.velocity.x.abs();
            let at = vec2(old.x + self.velocity.x.signum() * RADIUS, old.y);
            self.velocity.x *= -0.28;
            if impact > 22. {
                cue = self.hit(
                    8. + impact * 0.12,
                    "Reef strike! Hold Space to brake near stone.",
                    Cause::Reef,
                    at,
                );
            }
        } else {
            self.position.x = target.x;
        }
        if self.solid(vec2(self.position.x, target.y), RADIUS) {
            let impact = self.velocity.y.abs();
            let at = vec2(self.position.x, old.y + self.velocity.y.signum() * RADIUS);
            self.velocity.y *= -0.28;
            if impact > 22. {
                cue = self.hit(
                    8. + impact * 0.12,
                    "Reef strike! Hold Space to brake before tight turns.",
                    Cause::Reef,
                    at,
                );
            }
        } else {
            self.position.y = target.y;
        }
        cue
    }
    /// Execute one fixed 60 Hz tick. Menus never sample real time or input devices.
    pub fn tick(&mut self, input: &Input) -> Cue {
        self.events = Events::new();
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        match self.phase {
            Phase::Briefing => {
                if let Some(slot) = input.slot.filter(|s| (1..=3).contains(s)) {
                    self.selected = slot - 1;
                    return self.buy(self.selected);
                }
                if input.left_pressed {
                    self.selected = (self.selected + 2) % 3;
                }
                if input.right_pressed {
                    self.selected = (self.selected + 1) % 3;
                }
                if input.secondary {
                    return self.buy(self.selected);
                }
                if input.click {
                    for index in 0..3 {
                        if card_hit(input.pointer, index) {
                            self.selected = index;
                            return self.buy(index);
                        }
                    }
                    if launch_hit(input.pointer) {
                        return self.launch();
                    }
                }
                if input.primary {
                    return self.launch();
                }
                return Cue::None;
            }
            Phase::Debrief => {
                if input.primary || input.click && launch_hit(input.pointer) {
                    self.rescued_before += self.delivered();
                    self.mission += 1;
                    self.phase = Phase::Briefing;
                    self.reset_dive();
                    self.notice = "1 / 2 / 3 buys a refit. Space launches the next sortie.".into();
                    self.events.push(Event::NextSortie);
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Wreck => {
                return if input.primary || input.click && launch_hit(input.pointer) {
                    self.recover()
                } else {
                    Cue::None
                };
            }
            Phase::Complete => return Cue::None,
            Phase::Diving => {}
        }
        self.elapsed += 1;
        self.total_ticks += 1;
        self.impact_cooldown = self.impact_cooldown.saturating_sub(1);
        self.sonar = self.sonar.saturating_sub(1);
        self.sonar_cooldown = self.sonar_cooldown.saturating_sub(1);
        let air_before = self.oxygen;
        let mut cue = Cue::None;
        // The instrument buttons are the same commands as X and C.
        let clicked = |button| input.click && button_hit(input.pointer, button);
        if input.secondary || clicked(BALLAST_BUTTON) {
            self.balanced = !self.balanced;
            self.notice = if self.balanced {
                "Neutral ballast: no passive rise."
            } else {
                "Light ballast: the submarine rises when you release the controls."
            }
            .into();
            self.events.push(Event::Ballast {
                neutral: self.balanced,
            });
            cue = Cue::Action;
        }
        if (input.tertiary || clicked(SONAR_BUTTON)) && self.sonar_cooldown == 0 && self.oxygen > 3.
        {
            self.sonar = 180;
            self.sonar_cooldown = 240;
            self.oxygen -= 2.;
            self.notice = "Sonar shows danger circles for 3 seconds. Air cost: 2 seconds.".into();
            self.events.push(Event::Sonar);
            cue = Cue::Action;
        }
        let movement = self.move_ship(input);
        if movement != Cue::None {
            cue = movement;
        }
        let rate = 1.
            + self.mission as f32 * 0.045
            + self.onboard() as f32 * 0.18
            + if self.position.y > 280. { 0.3 } else { 0. };
        self.oxygen = (self.oxygen - TICK_DT * rate).max(0.);
        if air_before >= LOW_AIR && self.oxygen < LOW_AIR && self.oxygen > 0. {
            self.events.push(Event::LowAir);
        }
        if self.mission >= 4 && self.position.y > 315. && self.upgrades[1] == 0 {
            self.hull = (self.hull - TICK_DT * 0.6).max(0.);
        }
        for hazard in self.chart().hazards {
            let center = hazard.position(self.elapsed);
            if hazard.active(self.elapsed)
                && self.position.distance(center) < RADIUS + hazard.radius()
            {
                let reason = match hazard.kind {
                    HazardKind::Vent => {
                        "Thermal plume! Wait for its dark phase or give it a wide berth."
                    }
                    HazardKind::Jelly => "Jellyfish discharge! Give the moving bell a wide berth.",
                    HazardKind::Mine => "Old mine struck the hull. Sonar shows its danger radius.",
                };
                let at = self.position + (center - self.position).normalize_or_zero() * RADIUS;
                let hit = self.hit(
                    if hazard.kind == HazardKind::Mine {
                        27.
                    } else {
                        16.
                    },
                    reason,
                    Cause::Hazard(hazard.kind),
                    at,
                );
                if hit != Cue::None {
                    cue = hit;
                }
            }
        }
        if self.hull <= 0. || self.oxygen <= 0. {
            self.failure = if self.hull <= 0. {
                "The pressure hull failed."
            } else {
                "The breathing mixture ran out."
            };
            self.events.push(Event::Wreck {
                hull: self.hull <= 0.,
            });
            self.phase = Phase::Wreck;
            self.velocity = Vec2::ZERO;
            return Cue::Bad;
        }
        if input.primary {
            let result = self.try_winch();
            if result != Cue::None {
                cue = result;
            }
        }
        cue
    }
    /// A legal read-only rescue pilot. It navigates the same chart and submits
    /// ordinary held thrust and edge commands; it never changes simulation state.
    pub fn pilot(&self, _tick: u64) -> Input {
        match self.phase {
            Phase::Briefing => {
                let desired = if self.upgrades[0] < 1 {
                    Some(0)
                } else if self.upgrades[2] < 1 {
                    Some(2)
                } else if self.upgrades[1] < 1 {
                    Some(1)
                } else if self.upgrades[0] < 2 {
                    Some(0)
                } else {
                    None
                };
                if let Some(index) = desired
                    && self.credits >= self.upgrade_cost(index)
                {
                    return Input {
                        slot: Some(index + 1),
                        ..Input::default()
                    };
                }
                return Input {
                    primary: true,
                    ..Input::default()
                };
            }
            Phase::Debrief | Phase::Wreck => {
                return Input {
                    primary: true,
                    ..Input::default()
                };
            }
            Phase::Complete => return Input::default(),
            Phase::Diving => {}
        }
        if !self.balanced {
            return Input {
                secondary: true,
                ..Input::default()
            };
        }
        let (home_distance, _) = self.route(BASE);
        let return_air = self.air_reserve(home_distance);
        let returning = self.onboard() >= self.capacity()
            || self.oxygen < return_air
            || self.divers[..self.chart().divers.len()]
                .iter()
                .all(|s| *s != RescueState::Waiting);
        let mut destination = BASE;
        if !returning {
            let mut shortest = f32::MAX;
            for (index, (point, _)) in self.chart().divers.iter().enumerate() {
                if self.divers[index] != RescueState::Waiting {
                    continue;
                }
                let (distance, _) = self.route(*point);
                if distance < shortest {
                    shortest = distance;
                    destination = *point;
                }
            }
        }
        if self.position.distance(destination) < 23. {
            return Input {
                primary: self.velocity.length() < 32.,
                primary_down: true,
                ..Input::default()
            };
        }
        let (_, waypoint) = self.route(destination);
        let offset = waypoint - self.position;
        let desired = offset.normalize_or_zero() * (offset.length() * 3.).min(78.);
        let error = desired - self.velocity;
        Input {
            right: error.x > 5.,
            left: error.x < -5.,
            down: error.y > 5.,
            up: error.y < -5.,
            ..Input::default()
        }
    }
    fn nav_clear(&self, p: Vec2) -> bool {
        !self.solid(p, RADIUS + 5.)
            && !self.chart().hazards.iter().any(|hazard| {
                let extra = if hazard.kind == HazardKind::Jelly {
                    36.
                } else {
                    0.
                };
                p.distance(hazard.point) < hazard.radius() + RADIUS + 7. + extra
            })
    }
    /// Breadth-first step counts from `goal` over clear navigation cells;
    /// the boat's own cell `start` is always admitted.
    fn field(&self, start: usize, goal: usize) -> [u16; NAV_CELLS] {
        let mut distances = [u16::MAX; NAV_CELLS];
        let mut queue = [0usize; NAV_CELLS];
        distances[goal] = 0;
        queue[0] = goal;
        let mut head = 0;
        let mut tail = 1;
        while head < tail {
            let current = queue[head];
            head += 1;
            for next in neighbors(current).into_iter().flatten() {
                if distances[next] == u16::MAX && (next == start || self.nav_clear(nav_point(next)))
                {
                    distances[next] = distances[current] + 1;
                    queue[tail] = next;
                    tail += 1;
                }
            }
        }
        distances
    }
    fn route(&self, destination: Vec2) -> (f32, Vec2) {
        let start = nav_cell(self.position);
        let goal = nav_cell(destination);
        let distances = self.field(start, goal);
        if start == goal {
            return (self.position.distance(destination), destination);
        }
        if distances[start] == u16::MAX {
            return (10_000., destination);
        }
        let mut cursor = start;
        let mut direction = None;
        let mut waypoint = nav_point(start);
        for _ in 0..4 {
            let Some(next) = downhill(&distances, cursor) else {
                break;
            };
            let step = nav_point(next) - nav_point(cursor);
            if direction.is_some_and(|d: Vec2| d != step) {
                break;
            }
            direction = Some(step);
            cursor = next;
            waypoint = nav_point(next);
            if cursor == goal {
                waypoint = destination;
                break;
            }
        }
        (f32::from(distances[start]) * 20., waypoint)
    }
    /// The whole grid route the pilot would follow toward `destination`
    /// (at most [`TRAIL`] cells), for drawing a guidance path. Read-only.
    pub(crate) fn trail(&self, destination: Vec2) -> Trail {
        let start = nav_cell(self.position);
        let goal = nav_cell(destination);
        let distances = self.field(start, goal);
        let mut trail = Trail {
            points: [destination; TRAIL],
            len: 0,
            distance: if start == goal {
                self.position.distance(destination)
            } else if distances[start] == u16::MAX {
                10_000.
            } else {
                f32::from(distances[start]) * 20.
            },
        };
        if distances[start] == u16::MAX {
            return trail;
        }
        let mut cursor = start;
        while cursor != goal && trail.len < TRAIL {
            let Some(next) = downhill(&distances, cursor) else {
                break;
            };
            cursor = next;
            trail.points[trail.len] = if cursor == goal {
                destination
            } else {
                nav_point(cursor)
            };
            trail.len += 1;
        }
        trail
    }
}
fn nav_cell(p: Vec2) -> usize {
    let x = (p.x / 20.).clamp(0., 31.) as usize;
    let y = (p.y / 20.).clamp(0., 19.) as usize;
    y * NAV_W + x
}
fn nav_point(index: usize) -> Vec2 {
    vec2(
        (index % NAV_W) as f32 * 20. + 10.,
        (index / NAV_W) as f32 * 20. + 10.,
    )
}
/// The neighbor of `cursor` closest to the goal (first minimum, as the pilot).
fn downhill(distances: &[u16; NAV_CELLS], cursor: usize) -> Option<usize> {
    neighbors(cursor)
        .into_iter()
        .flatten()
        .filter(|&n| distances[n] < distances[cursor])
        .min_by_key(|&n| distances[n])
}
fn neighbors(index: usize) -> [Option<usize>; 4] {
    let x = index % NAV_W;
    let y = index / NAV_W;
    [
        if x > 0 { Some(index - 1) } else { None },
        if x + 1 < NAV_W { Some(index + 1) } else { None },
        if y > 0 { Some(index - NAV_W) } else { None },
        if y + 1 < NAV_H {
            Some(index + NAV_W)
        } else {
            None
        },
    ]
}
pub(crate) fn card_hit(p: Vec2, index: usize) -> bool {
    p.x >= 64. + index as f32 * 204.
        && p.x <= 252. + index as f32 * 204.
        && p.y >= 335.
        && p.y <= 435.
}
pub(crate) fn launch_hit(p: Vec2) -> bool {
    p.x >= 64. && p.x <= 652. && p.y >= 479. && p.y <= 519.
}
/// Whether canvas point `p` is inside an instrument button `(x, y, w, h)`.
pub(crate) fn button_hit(p: Vec2, (x, y, w, h): (f32, f32, f32, f32)) -> bool {
    p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
}

#[cfg(test)]
mod tests {
    use super::*;
    fn launched() -> Game {
        let mut g = Game::new();
        g.tick(&Input {
            primary: true,
            ..Input::default()
        });
        g
    }
    #[test]
    fn legal_pilot_rescues_all_twenty_seven_across_six_sorties() {
        let mut g = Game::new();
        let mut visited = [false; 6];
        for tick in 0..120_000 {
            visited[g.mission] = true;
            let input = g.pilot(tick);
            g.tick(&input);
            if g.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(
            g.outcome(),
            Outcome::Won,
            "sortie {} phase {:?} pos {:?} oxygen {} hull {} states {:?} notice {}",
            g.sortie(),
            g.phase,
            g.position,
            g.oxygen,
            g.hull,
            g.divers,
            g.notice
        );
        assert_eq!(g.rescued(), 27);
        assert!(visited.into_iter().all(|v| v));
        assert!(g.medals.into_iter().all(|m| m > 0));
        assert!(g.score > 6_750);
        let snapshot = (
            g.score,
            g.credits,
            g.oxygen,
            g.position,
            g.rescued(),
            g.total_ticks,
        );
        for _ in 0..60 {
            assert_eq!(
                g.tick(&Input {
                    primary: true,
                    secondary: true,
                    tertiary: true,
                    right: true,
                    slot: Some(1),
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(
            snapshot,
            (
                g.score,
                g.credits,
                g.oxygen,
                g.position,
                g.rescued(),
                g.total_ticks
            )
        );
    }
    #[test]
    fn briefing_is_inert_and_invalid_refits_spend_nothing() {
        let mut g = Game::new();
        for _ in 0..100 {
            g.tick(&Input::default());
        }
        assert_eq!(g.oxygen, 42.);
        assert_eq!(g.elapsed, 0);
        assert_eq!(
            g.tick(&Input {
                slot: Some(1),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(
            g.tick(&Input {
                slot: Some(5),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(g.credits, 20);
        assert_eq!(g.upgrades, [0; 3]);
        g.credits = 100;
        assert_eq!(
            g.tick(&Input {
                slot: Some(1),
                ..Input::default()
            }),
            Cue::Good
        );
        assert_eq!(g.credits, 70);
        assert_eq!(g.max_air(), 54.);
    }
    #[test]
    fn cabin_refits_have_two_paid_levels_and_reject_further_spending() {
        let mut g = Game::new();
        g.credits = 300;
        let buy = Input {
            slot: Some(3),
            ..Input::default()
        };
        assert_eq!(g.tick(&buy), Cue::Good);
        assert_eq!(g.capacity(), 3);
        assert_eq!(g.credits, 265);
        assert_eq!(g.tick(&buy), Cue::Good);
        assert_eq!(g.capacity(), 4);
        assert_eq!(g.credits, 210);
        assert_eq!(g.tick(&buy), Cue::None);
        assert_eq!(g.capacity(), 4);
        assert_eq!(g.credits, 210);
        assert_eq!(
            g.tick(&Input {
                slot: Some(usize::MAX),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(g.credits, 210);
    }
    #[test]
    fn movement_has_inertia_and_ballast_changes_passive_rise() {
        let mut g = launched();
        g.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        for _ in 0..45 {
            g.tick(&Input {
                right: true,
                ..Input::default()
            });
        }
        let before = g.position;
        g.tick(&Input::default());
        assert!(g.position.x > before.x);
        let mut floating = launched();
        floating.position = vec2(90., 230.);
        let mut neutral = floating.clone();
        neutral.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        for _ in 0..60 {
            floating.tick(&Input::default());
            neutral.tick(&Input::default());
        }
        assert!(floating.position.y < neutral.position.y - 3.);
        assert!((neutral.position.y - 230.).abs() < 0.01);
    }
    #[test]
    fn collision_blocks_reefs_and_fast_impact_hurts() {
        let mut g = launched();
        g.position = vec2(208., 170.);
        g.velocity = vec2(90., 0.);
        g.balanced = true;
        g.tick(&Input {
            right: true,
            ..Input::default()
        });
        assert!(g.position.x < 210.);
        assert!(g.hull < 100.);
        assert!(g.velocity.x < 0.);
        for _ in 0..120 {
            g.tick(&Input {
                right: true,
                ..Input::default()
            });
        }
        assert!(g.position.x <= 209.);
        assert!(!g.solid(g.position, RADIUS));
    }
    #[test]
    fn winch_requires_slow_nearby_boat_and_cabin_capacity() {
        let mut g = launched();
        assert_eq!(g.try_winch(), Cue::None);
        g.position = g.chart().divers[0].0;
        g.velocity = vec2(40., 0.);
        assert_eq!(g.try_winch(), Cue::None);
        g.velocity = Vec2::ZERO;
        assert_eq!(g.try_winch(), Cue::Good);
        assert_eq!(g.onboard(), 1);
        assert_eq!(g.try_winch(), Cue::None);
        assert_eq!(g.score, 0);
        g.position = g.chart().divers[1].0;
        assert_eq!(g.try_winch(), Cue::Good);
        g.position = g.chart().divers[2].0;
        assert_eq!(g.try_winch(), Cue::None);
        assert_eq!(g.onboard(), 2);
    }
    #[test]
    fn docking_lands_actual_survivors_and_cannot_duplicate_rewards() {
        let mut g = launched();
        g.position = g.chart().divers[0].0;
        g.try_winch();
        assert_eq!(g.rescued(), 0);
        g.position = BASE;
        g.oxygen = 8.;
        g.hull = 40.;
        assert_eq!(g.try_winch(), Cue::Good);
        assert_eq!(g.rescued(), 1);
        assert_eq!(g.credits, 32);
        assert_eq!(g.score, 250);
        assert_eq!(g.oxygen, 42.);
        assert_eq!(g.hull, 100.);
        assert_eq!(g.try_winch(), Cue::None);
        assert_eq!(g.credits, 32);
        assert_eq!(g.score, 250);
    }
    #[test]
    fn sonar_has_air_cost_and_rejects_cooldown_spam() {
        let mut g = launched();
        let ping = Input {
            tertiary: true,
            ..Input::default()
        };
        g.tick(&ping);
        assert!((g.oxygen - (40. - TICK_DT)).abs() < 0.001);
        assert_eq!(g.sonar, 180);
        let before = g.oxygen;
        g.tick(&ping);
        assert!((before - g.oxygen - TICK_DT).abs() < 0.001);
        assert_eq!(g.sonar, 179);
    }
    #[test]
    fn wreck_rolls_back_current_sortie_earnings_and_exhausts_tugs() {
        let mut g = launched();
        g.position = g.chart().divers[0].0;
        g.try_winch();
        g.position = BASE;
        g.try_winch();
        g.oxygen = 0.001;
        g.tick(&Input::default());
        assert_eq!(g.phase, Phase::Wreck);
        assert_eq!(g.outcome(), Outcome::Playing);
        g.tick(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(g.recovery, 2);
        assert_eq!(g.score, 0);
        assert_eq!(g.credits, 20);
        assert_eq!(g.rescued(), 0);
        for _ in 0..3 {
            g.oxygen = 0.001;
            g.tick(&Input::default());
            g.tick(&Input {
                primary: true,
                ..Input::default()
            });
        }
        assert_eq!(g.outcome(), Outcome::Lost);
        let before = g.oxygen;
        g.tick(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(g.oxygen, before);
    }
    #[test]
    fn timed_vents_and_armor_change_damage() {
        let vent = crate::content::Hazard::vent(0., 0., 0);
        assert!(!vent.active(179));
        assert!(vent.active(180));
        assert!(!vent.active(300));
        let mut bare = launched();
        let mut armored = bare.clone();
        armored.upgrades[1] = 1;
        bare.damage(27., "test");
        armored.damage(27., "test");
        assert!(armored.hull > bare.hull);
        let before = bare.hull;
        bare.damage(27., "test");
        assert_eq!(bare.hull, before);
    }
    #[test]
    fn chart_beacons_are_clear_and_have_safe_routes() {
        assert_eq!(MISSIONS.iter().map(|m| m.divers.len()).sum::<usize>(), 27);
        for mission in 0..6 {
            let mut g = Game::new();
            g.mission = mission;
            for (p, name) in g.chart().divers {
                assert!(!g.solid(*p, RADIUS), "{mission} {name}");
                assert!(g.route(*p).0 < 10_000., "unreachable {mission} {name}");
            }
        }
    }
    #[test]
    fn pointer_actions_match_drawn_card_and_launch_bounds() {
        let mut g = Game::new();
        g.credits = 60;
        g.tick(&Input {
            click: true,
            pointer: vec2(300., 370.),
            ..Input::default()
        });
        assert_eq!(g.upgrades, [0, 1, 0]);
        assert_eq!(g.credits, 35);
        g.tick(&Input {
            click: true,
            pointer: vec2(300., 500.),
            ..Input::default()
        });
        assert_eq!(g.phase, Phase::Diving);
    }
    fn click(at: (f32, f32, f32, f32)) -> Input {
        Input {
            click: true,
            pointer: vec2(at.0 + at.2 * 0.5, at.1 + at.3 * 0.5),
            ..Input::default()
        }
    }
    #[test]
    fn instrument_buttons_mirror_ballast_and_sonar_keys() {
        let mut keyed = launched();
        let mut clicked = keyed.clone();
        keyed.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(clicked.tick(&click(BALLAST_BUTTON)), Cue::Action);
        assert!(keyed.balanced && clicked.balanced);
        assert_eq!(keyed.position, clicked.position);
        keyed.tick(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(clicked.tick(&click(SONAR_BUTTON)), Cue::Action);
        assert_eq!((keyed.sonar, clicked.sonar), (180, 180));
        assert_eq!(keyed.oxygen, clicked.oxygen);
        // Cooldown refuses the button exactly as it refuses C.
        let before = clicked.oxygen;
        assert_eq!(clicked.tick(&click(SONAR_BUTTON)), Cue::None);
        assert!((before - clicked.oxygen - TICK_DT).abs() < 0.001);
        // Clicks elsewhere (the chart, the briefing cards) change nothing.
        let mut idle = launched();
        let mut reference = idle.clone();
        idle.tick(&Input {
            click: true,
            pointer: vec2(300., 370.),
            ..Input::default()
        });
        reference.tick(&Input::default());
        assert_eq!(
            (idle.balanced, idle.sonar, idle.oxygen),
            (reference.balanced, reference.sonar, reference.oxygen)
        );
        // Only while diving: the briefing ignores the instrument buttons.
        let mut briefing = Game::new();
        briefing.tick(&click(BALLAST_BUTTON));
        assert!(!briefing.balanced && briefing.phase == Phase::Briefing);
    }
    #[test]
    fn events_record_accepted_actions_and_clear_every_tick() {
        let mut g = launched();
        assert_eq!(g.events.as_slice(), &[Event::Launch]);
        g.balanced = true;
        g.position = g.chart().divers[0].0;
        let winch = Input {
            primary: true,
            ..Input::default()
        };
        assert_eq!(g.tick(&winch), Cue::Good);
        assert_eq!(g.events.as_slice(), &[Event::Winched { index: 0 }]);
        g.tick(&Input::default());
        assert!(g.events.as_slice().is_empty());
        // A fast pass beside a bell is refused with feedback at that bell.
        g.position = g.chart().divers[1].0;
        g.velocity = vec2(60., 0.);
        g.tick(&winch);
        assert!(matches!(g.events.as_slice(), [Event::TooFast { .. }]));
        // The same tap far from anything stays silent (it is just braking).
        g.position = vec2(330., 60.);
        g.velocity = vec2(60., 0.);
        g.tick(&winch);
        assert!(g.events.as_slice().is_empty());
        // Landing at the dock reports the people delivered.
        g.position = BASE;
        g.velocity = Vec2::ZERO;
        g.tick(&winch);
        assert_eq!(
            g.events.as_slice(),
            &[Event::Docked {
                landed: 1,
                cleared: false
            }]
        );
    }
    #[test]
    fn full_cabin_low_air_impacts_and_wrecks_are_reported() {
        let mut g = launched();
        g.balanced = true;
        for index in 0..2 {
            g.position = g.chart().divers[index].0;
            g.velocity = Vec2::ZERO;
            g.try_winch();
        }
        g.position = g.chart().divers[2].0;
        g.events = Events::new();
        assert_eq!(g.try_winch(), Cue::None);
        assert_eq!(
            g.events.as_slice(),
            &[Event::CabinFull {
                at: g.chart().divers[2].0
            }]
        );
        // Crossing the low-air line raises one alarm, not one per tick.
        g.oxygen = LOW_AIR + 0.01;
        g.tick(&Input::default());
        assert_eq!(g.events.as_slice(), &[Event::LowAir]);
        g.tick(&Input::default());
        assert!(g.events.as_slice().is_empty());
        // A reef strike records its cause, place and the hull it cost.
        let mut hit = launched();
        hit.position = vec2(208., 170.);
        hit.velocity = vec2(90., 0.);
        hit.balanced = true;
        hit.tick(&Input {
            right: true,
            ..Input::default()
        });
        let [Event::Impact { at, loss, cause }] = hit.events.as_slice() else {
            panic!("{:?}", hit.events);
        };
        assert_eq!(*cause, Cause::Reef);
        assert!((loss - (100. - hit.hull)).abs() < 0.001);
        assert!(at.x > hit.position.x);
        let mut sunk = launched();
        sunk.oxygen = 0.001;
        sunk.tick(&Input::default());
        assert_eq!(sunk.events.as_slice(), &[Event::Wreck { hull: false }]);
    }
    #[test]
    fn guidance_trail_follows_the_pilot_route_without_mutation() {
        let mut g = launched();
        g.position = g.chart().divers[2].0;
        let before = (g.position, g.velocity, g.oxygen, g.elapsed);
        let trail = g.trail(BASE);
        assert!(trail.len > 10 && trail.len <= TRAIL);
        assert_eq!(trail.points[trail.len - 1], BASE);
        assert_eq!(trail.distance, g.route(BASE).0);
        assert!(
            trail.points[..trail.len]
                .iter()
                .all(|p| !g.solid(*p, RADIUS))
        );
        assert_eq!(before, (g.position, g.velocity, g.oxygen, g.elapsed));
        // The reserve mark is the same rule of thumb the pilot turns on.
        assert_eq!(
            g.air_reserve(trail.distance),
            trail.distance / 55. * 1.5 + 8.
        );
        g.position = BASE;
        assert_eq!(g.trail(BASE).len, 0);
    }
    #[test]
    fn medal_pace_matches_the_awarded_medal() {
        let mut g = launched();
        assert_eq!(g.medal_pace(), 3);
        g.elapsed = GOLD_TICKS;
        assert_eq!(g.medal_pace(), 2);
        g.stage_impacts = 5;
        assert_eq!(g.medal_pace(), 1);
    }
}
