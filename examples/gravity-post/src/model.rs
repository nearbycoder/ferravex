//! Renderer-independent fixed-tick lander rules.
//!
//! ```
//! use gravity_post::{Game, Phase};
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! assert_eq!(game.phase(), Phase::Briefing);
//! game.step(&Input { primary: true, ..Input::default() });
//! assert_eq!(game.phase(), Phase::Flight);
//! let fuel = game.fuel();
//! game.step(&Input { up: true, ..Input::default() });
//! assert!(game.fuel() < fuel);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{PLANETS, Planet, Role};
use ferravex_arcade::{Cue, Input, Outcome, TICK_DT};
use ferravex_math::Vec2;
use ferravex_play::motion::approach_vec2;

pub(crate) const FEET: f32 = 10.;
pub(crate) const PAD_HALF: f32 = 32.;
const HALF_BODY: f32 = 15.;
const MAX_TILT: f32 = 65. * std::f32::consts::PI / 180.;

/// Contract and career lifecycle; shared host pause/help is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Briefing,
    Flight,
    Debrief,
    Wreck,
    Certified,
    Grounded,
}

/// A complete eight-world career with fixed-size content and resource storage.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub(crate) phase: Phase,
    pub(crate) index: usize,
    pub(crate) pos: Vec2,
    pub(crate) velocity: Vec2,
    pub(crate) angle: f32,
    pub(crate) fuel: f32,
    pub(crate) hull: f32,
    pub(crate) cargo: f32,
    pub(crate) loaded: bool,
    pub(crate) docked: Option<usize>,
    pub(crate) last_pad: usize,
    pub(crate) reserves: [f32; 4],
    pub(crate) repaired: [bool; 4],
    pub(crate) beacon: [bool; 8],
    pub(crate) stamps: [u8; 8],
    pub(crate) upgrades: [u8; 3],
    pub(crate) permits: u8,
    pub(crate) credits: u32,
    pub(crate) score: u32,
    pub(crate) checkpoint: (u32, u32),
    pub(crate) ticks: u32,
    pub(crate) impact_cooldown: u8,
    pub(crate) thrusting: bool,
    pub(crate) braking: bool,
    pub(crate) note: &'static str,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin a fresh career with three recovery permits and 60 refit credits.
    pub fn new() -> Self {
        let mut game = Self {
            phase: Phase::Briefing,
            index: 0,
            pos: Vec2::ZERO,
            velocity: Vec2::ZERO,
            angle: 0.,
            fuel: 0.,
            hull: 100.,
            cargo: 100.,
            loaded: false,
            docked: None,
            last_pad: 0,
            reserves: [0.; 4],
            repaired: [false; 4],
            beacon: [false; 8],
            stamps: [0; 8],
            upgrades: [0; 3],
            permits: 3,
            credits: 60,
            score: 0,
            checkpoint: (0, 60),
            ticks: 0,
            impact_cooldown: 0,
            thrusting: false,
            braking: false,
            note: "Read the route. Launch when ready.",
        };
        game.reset_contract();
        game
    }
    /// Current local phase, excluding shared pause/help.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// One-based current contract number.
    pub fn contract(&self) -> usize {
        self.index + 1
    }
    /// Current career score; a recovery rolls back the failed contract.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Remaining fuel units, always between zero and the installed tank capacity.
    pub fn fuel(&self) -> f32 {
        self.fuel
    }
    /// Position in metres on the local chart; y increases downward.
    pub fn position(&self) -> Vec2 {
        self.pos
    }
    /// Horizontal and downward velocity in metres per second.
    pub fn velocity(&self) -> Vec2 {
        self.velocity
    }
    /// Number of successful deliveries, excluding an uncompleted current parcel.
    pub fn delivered(&self) -> usize {
        self.stamps.iter().filter(|&&s| s > 0).count()
    }
    /// Career result. Wrecks with remaining permits are recoverable, not terminal.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Certified => Outcome::Won,
            Phase::Grounded => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    pub(crate) fn planet(&self) -> &'static Planet {
        &PLANETS[self.index]
    }
    pub(crate) fn capacity(&self) -> f32 {
        120. + 30. * f32::from(self.upgrades[1])
    }
    pub(crate) fn engine(&self) -> f32 {
        (112. + 14. * f32::from(self.upgrades[0])) / if self.loaded { 1.22 } else { 1. }
    }
    pub(crate) fn limits(&self) -> (f32, f32, f32) {
        (
            10. + 3. * f32::from(self.upgrades[2]),
            18. + 3. * f32::from(self.upgrades[2]),
            0.24 + 0.06 * f32::from(self.upgrades[2]),
        )
    }
    pub(crate) fn pad(&self, i: usize) -> Vec2 {
        Vec2::new(80. + 160. * i as f32, self.planet().terrain[2 + i * 4])
    }
    pub(crate) fn role_pad(&self, role: Role) -> usize {
        self.planet()
            .roles
            .iter()
            .position(|&r| r == role)
            .expect("every authored planet has each pad role")
    }
    pub(crate) fn wind(&self) -> f32 {
        self.planet().wind + self.planet().gust * (self.ticks as f32 / 170.).sin()
    }
    pub(crate) fn ground(&self, x: f32) -> f32 {
        for i in 0..4 {
            let p = self.pad(i);
            if (x - p.x).abs() <= PAD_HALF {
                return p.y;
            }
        }
        let t = (x / 40.).clamp(0., 16.);
        let i = (t as usize).min(15);
        let f = t - i as f32;
        self.planet().terrain[i] * (1. - f) + self.planet().terrain[i + 1] * f
    }
    pub(crate) fn upgrade_cost(&self, i: usize) -> u32 {
        [110, 90, 80][i] + u32::from(self.upgrades[i]) * 60
    }
    fn reset_contract(&mut self) {
        let start = self.role_pad(Role::Depot);
        self.pos = self.pad(start) - Vec2::new(0., FEET);
        self.velocity = Vec2::ZERO;
        self.angle = 0.;
        self.fuel = self.capacity();
        self.hull = 100.;
        self.cargo = 100.;
        self.loaded = false;
        self.docked = Some(start);
        self.last_pad = start;
        self.reserves = self.planet().roles.map(|r| match r {
            Role::Depot => 75.,
            Role::Pickup => 28.,
            Role::Outpost => 65.,
            Role::Delivery => 0.,
        });
        self.repaired = [false; 4];
        self.beacon[self.index] = false;
        self.ticks = 0;
        self.impact_cooldown = 0;
        self.thrusting = false;
        self.braking = false;
        self.phase = Phase::Briefing;
        self.note = "Blue depot -> yellow pickup -> green destination. X services a pad.";
    }
    fn wreck(&mut self, note: &'static str) -> Cue {
        self.phase = if self.permits > 0 {
            Phase::Wreck
        } else {
            Phase::Grounded
        };
        self.note = note;
        self.thrusting = false;
        self.braking = false;
        Cue::Bad
    }
    fn buy(&mut self, i: usize) -> Cue {
        if i >= 3 || self.upgrades[i] >= 2 || self.credits < self.upgrade_cost(i) {
            return Cue::None;
        }
        self.credits -= self.upgrade_cost(i);
        self.upgrades[i] += 1;
        self.note = "Refit installed for all later contracts.";
        Cue::Good
    }
    fn service(&mut self) -> Cue {
        let Some(pad) = self.docked else {
            return Cue::None;
        };
        let role = self.planet().roles[pad];
        let mut changed = false;
        let fuel = (self.capacity() - self.fuel).min(self.reserves[pad]);
        if fuel > 0.01 {
            self.fuel += fuel;
            self.reserves[pad] -= fuel;
            changed = true;
            self.note = "Finite pad reserves transferred to the tank.";
        }
        if role == Role::Outpost && !self.repaired[pad] && self.hull < 100. {
            self.hull = (self.hull + 30.).min(100.);
            self.repaired[pad] = true;
            changed = true;
            self.note = "Outpost crew repaired the hull. This repair kit is now spent.";
        }
        if role == Role::Pickup && !self.loaded {
            self.loaded = true;
            self.score += 100;
            changed = true;
            self.note = "Parcel secured. Extra mass reduces thrust: climb before crossing.";
        }
        if role == Role::Delivery && self.loaded {
            let stamps = if self.cargo >= 90. && self.hull >= 80. {
                3
            } else if self.cargo >= 60. && self.hull >= 40. {
                2
            } else {
                1
            };
            self.stamps[self.index] = stamps;
            let seconds = self.planet().deadline.saturating_sub(self.ticks / 60);
            self.score += u32::from(stamps) * 500
                + (self.cargo * 8. + self.hull * 4. + self.fuel * 4.) as u32
                + seconds * 3;
            self.credits += 120 + u32::from(stamps) * 20;
            self.phase = if self.index == 7 {
                Phase::Certified
            } else {
                Phase::Debrief
            };
            self.thrusting = false;
            self.braking = false;
            self.note = "Delivered. Another community has heard from home.";
            return if self.index == 7 { Cue::Win } else { Cue::Good };
        }
        if changed { Cue::Good } else { Cue::None }
    }
    fn damage(&mut self, speed: f32, hard: bool) -> Cue {
        if self.impact_cooldown > 0 {
            return Cue::None;
        }
        let damage = (if hard { 14. } else { 8. }) + (speed - 10.).max(0.) * 1.3;
        let protection = 1. - f32::from(self.upgrades[2]) * 0.15;
        self.hull = (self.hull - damage * protection).max(0.);
        if self.loaded {
            self.cargo = (self.cargo - damage * 0.75).max(0.);
        }
        self.impact_cooldown = 45;
        self.note = "Impact! Slow both axes and level the ship before landing.";
        if self.hull <= 0. {
            self.wreck("Hull destroyed. A recovery permit can restart this contract.")
        } else if self.cargo <= 0. {
            self.wreck("The parcel broke apart. Its replacement needs a recovery permit.")
        } else {
            Cue::Bad
        }
    }
    /// Advance exactly one 1/60-second tick. Menus wait for input; terminal states are inert.
    pub fn step(&mut self, input: &Input) -> Cue {
        match self.phase {
            Phase::Certified | Phase::Grounded => return Cue::None,
            Phase::Briefing => {
                if input.primary
                    || (input.click
                        && (132. ..=640.).contains(&input.pointer.x)
                        && (486. ..=527.).contains(&input.pointer.y))
                {
                    self.phase = Phase::Flight;
                    self.note = "Launch with Up or Space. Climb above the ridge before tilting.";
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Wreck => {
                if input.primary && self.permits > 0 {
                    self.permits -= 1;
                    self.score = self.checkpoint.0;
                    self.credits = self.checkpoint.1;
                    self.reset_contract();
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Debrief => {
                if let Some(slot) = input.slot.filter(|s| (1..=3).contains(s)) {
                    return self.buy(slot - 1);
                }
                if input.click && (132. ..=640.).contains(&input.pointer.x) {
                    for i in 0..3 {
                        let y = 282. + i as f32 * 58.;
                        if (y..y + 50.).contains(&input.pointer.y) {
                            return self.buy(i);
                        }
                    }
                }
                if input.primary
                    || (input.click
                        && (132. ..=640.).contains(&input.pointer.x)
                        && (486. ..=527.).contains(&input.pointer.y))
                {
                    self.index += 1;
                    self.checkpoint = (self.score, self.credits);
                    self.reset_contract();
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Flight => {}
        }
        self.ticks += 1;
        if self.ticks >= self.planet().deadline * 60 {
            return self.wreck("The storm closed the route before the parcel arrived.");
        }
        if input.tertiary {
            return self.wreck("Recovery requested. This contract's progress will be rolled back.");
        }
        self.impact_cooldown = self.impact_cooldown.saturating_sub(1);
        let was_thrust = self.thrusting;
        let was_brake = self.braking;
        self.thrusting = false;
        self.braking = false;
        let mut cue = if input.secondary {
            self.service()
        } else {
            Cue::None
        };
        if self.phase != Phase::Flight {
            return cue;
        }
        let want_thrust = input.up || input.primary_down;
        if self.docked.is_some() {
            if !want_thrust || input.down || self.fuel <= 0. {
                return cue;
            }
            self.docked = None;
        }
        let turn = i32::from(input.right) - i32::from(input.left);
        self.angle = (self.angle + turn as f32 * 1.65 * TICK_DT).clamp(-MAX_TILT, MAX_TILT);
        if input.down && self.fuel > 0. {
            let fraction = (self.fuel / (1.9 * TICK_DT)).min(1.);
            self.fuel = (self.fuel - 1.9 * TICK_DT).max(0.);
            self.velocity =
                approach_vec2(self.velocity, Vec2::new(0., 8.), 90. * fraction, TICK_DT)
                    .expect("finite bounded flight values");
            self.angle -= self.angle.clamp(-2.2 * TICK_DT, 2.2 * TICK_DT);
            self.braking = true;
            if !was_brake {
                cue = Cue::Action;
            }
        } else if want_thrust && self.fuel > 0. {
            let burn = 3.8 - f32::from(self.upgrades[0]) * 0.3;
            let fraction = (self.fuel / (burn * TICK_DT)).min(1.);
            self.fuel = (self.fuel - burn * TICK_DT).max(0.);
            self.velocity +=
                Vec2::new(self.angle.sin(), -self.angle.cos()) * self.engine() * fraction * TICK_DT;
            self.thrusting = true;
            if !was_thrust {
                cue = Cue::Action;
            }
        }
        self.velocity += Vec2::new(self.wind(), self.planet().gravity) * TICK_DT;
        self.velocity *= 1. - self.planet().drag * TICK_DT;
        self.velocity.x = self.velocity.x.clamp(-180., 180.);
        self.velocity.y = self.velocity.y.clamp(-160., 180.);
        self.pos += self.velocity * TICK_DT;
        if self.pos.x < HALF_BODY || self.pos.x > 640. - HALF_BODY || self.pos.y < 10. {
            let impact = self.velocity.length();
            self.pos.x = self.pos.x.clamp(HALF_BODY, 640. - HALF_BODY);
            self.pos.y = self.pos.y.max(10.);
            self.velocity *= -0.2;
            cue = self.damage(impact, true);
            if self.phase != Phase::Flight {
                return cue;
            }
        }
        let surface = self
            .ground(self.pos.x - HALF_BODY)
            .min(self.ground(self.pos.x))
            .min(self.ground(self.pos.x + HALF_BODY));
        if self.pos.y + FEET >= surface {
            let pad = (0..4).find(|&i| {
                let p = self.pad(i);
                (self.pos.x - p.x).abs() <= PAD_HALF - HALF_BODY && (p.y - surface).abs() < 0.1
            });
            let limits = self.limits();
            if let Some(pad) = pad.filter(|_| {
                self.velocity.y >= 0.
                    && self.velocity.y <= limits.1
                    && self.velocity.x.abs() <= limits.0
                    && self.angle.abs() <= limits.2
            }) {
                self.pos.y = self.pad(pad).y - FEET;
                self.velocity = Vec2::ZERO;
                self.angle = 0.;
                self.docked = Some(pad);
                self.last_pad = pad;
                self.thrusting = false;
                self.braking = false;
                self.note = match self.planet().roles[pad] {
                    Role::Pickup => {
                        "Safe landing. Tap X to load the parcel and draw available fuel."
                    }
                    Role::Delivery => "Destination reached. X delivers a carried parcel.",
                    Role::Outpost => "Outpost reached. X uses its finite fuel and one repair kit.",
                    Role::Depot => "Back at the depot. X draws any remaining reserve fuel.",
                };
                cue = Cue::Good;
            } else {
                let impact = self.velocity.length();
                self.pos.y = surface - FEET - 0.1;
                self.velocity.y = -self.velocity.y.abs() * 0.25 - 9.;
                self.velocity.x *= 0.6;
                cue = self.damage(impact, pad.is_none());
                if self.phase != Phase::Flight {
                    return cue;
                }
            }
        }
        if !self.beacon[self.index]
            && self.pos.distance(Vec2::from_array(self.planet().beacon)) < 18.
        {
            self.beacon[self.index] = true;
            self.score += 250;
            self.credits += 20;
            self.fuel = (self.fuel + 15.).min(self.capacity());
            self.note = "Survey beacon logged: +250 score, +20 credits, +15 fuel.";
            cue = Cue::Good;
        }
        cue
    }
    /// Generate ordinary steering/service inputs for a reproducible demonstration.
    /// This reads the same chart and instruments available to a human; it changes no state.
    pub fn pilot(&self, tick: u64) -> Input {
        match self.phase {
            Phase::Briefing | Phase::Wreck => {
                return Input {
                    primary: tick.is_multiple_of(30),
                    ..Input::default()
                };
            }
            Phase::Debrief => {
                if !tick.is_multiple_of(30) {
                    return Input::default();
                }
                for i in [0, 1, 2] {
                    if self.upgrades[i] < 2 && self.credits >= self.upgrade_cost(i) {
                        return Input {
                            slot: Some(i + 1),
                            ..Input::default()
                        };
                    }
                }
                return Input {
                    primary: true,
                    ..Input::default()
                };
            }
            Phase::Certified | Phase::Grounded => return Input::default(),
            Phase::Flight => {}
        }
        let destination = self.role_pad(if self.loaded {
            Role::Delivery
        } else {
            Role::Pickup
        });
        if let Some(at) = self.docked {
            if at == destination {
                return Input {
                    secondary: true,
                    ..Input::default()
                };
            }
            if self.reserves[at] > 0.1 && self.fuel < self.capacity() - 0.1 {
                return Input {
                    secondary: true,
                    ..Input::default()
                };
            }
        }
        let target = self.pad(destination);
        let dx = target.x - self.pos.x;
        let launch_column = (self.pos.x - self.pad(self.last_pad).x).abs() < 38.
            && self.pos.y > 94.
            && self.last_pad != destination;
        let target_y = if dx.abs() > 32. || launch_column {
            65.
        } else {
            target.y - FEET + 1.
        };
        let desired_vx = if launch_column {
            0.
        } else {
            (dx * 0.72).clamp(-42., 42.)
        };
        let desired_ax = (desired_vx - self.velocity.x) * 1.6 - self.wind();
        let desired_angle = desired_ax.atan2(self.planet().gravity).clamp(-0.9, 0.9);
        let desired_vy = ((target_y - self.pos.y) * 0.8).clamp(-45., 36.);
        let brake = self.pos.y > target.y - 55.
            && dx.abs() < PAD_HALF - HALF_BODY - 1.
            && self.velocity.x.abs() < 15.
            && self.docked.is_none();
        Input {
            left: self.angle > desired_angle + 0.012,
            right: self.angle < desired_angle - 0.012,
            up: self.velocity.y > desired_vy - 0.2 || self.docked.is_some(),
            down: brake,
            ..Input::default()
        }
    }
    /// Plain-language career result for the host's paginated ending screen.
    pub fn summary(&self) -> String {
        let stamps: u32 = self.stamps.iter().map(|&x| u32::from(x)).sum();
        let beacons = self.beacon.iter().filter(|&&x| x).count();
        if self.outcome() == Outcome::Won {
            format!(
                "Courier certified. All 8 communities received their parcels. {stamps}/24 stamps, {beacons}/8 survey beacons, {} credits and {} recovery permits remain. The Dawn Archive preserves your route. Career score {}.",
                self.credits, self.permits, self.score
            )
        } else {
            format!(
                "{} of 8 parcels delivered. {stamps} stamps and {beacons} survey beacons. {} Career score {}. Begin a new career to try another route and refit strategy.",
                self.delivered(),
                self.note,
                self.score
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn launched() -> Game {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        game
    }
    fn airborne() -> Game {
        let mut game = launched();
        game.docked = None;
        game.pos = Vec2::new(150., 80.);
        game
    }
    fn over_pad(i: usize) -> Game {
        let mut game = airborne();
        game.pos = game.pad(i) - Vec2::new(0., FEET + 0.05);
        game.velocity = Vec2::new(0., 10.);
        game
    }
    #[test]
    fn legal_pilot_delivers_the_complete_eight_world_campaign() {
        let mut game = Game::new();
        let mut replay = Game::new();
        let mut visited = [false; 8];
        let mut ended = None;
        for tick in 0..180_000 {
            visited[game.index] = true;
            let input = game.pilot(tick);
            game.step(&input);
            replay.step(&input);
            assert_eq!(game, replay, "replay tick {tick}");
            if game.outcome() != Outcome::Playing {
                ended = Some(tick);
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won, "{game:?}");
        assert!(visited.into_iter().all(|v| v));
        assert_eq!(game.delivered(), 8);
        assert_eq!(game.permits, 3);
        assert!(game.stamps.iter().all(|&s| s >= 2));
        eprintln!(
            "Legal courier victory tick {}, score {}, stamps {:?}, permits {}",
            ended.unwrap(),
            game.score,
            game.stamps,
            game.permits
        );
        let frozen = game.clone();
        for _ in 0..30 {
            assert_eq!(
                game.step(&Input {
                    primary: true,
                    secondary: true,
                    tertiary: true,
                    left: true,
                    right: true,
                    up: true,
                    down: true,
                    slot: Some(1),
                    click: true,
                    pointer: Vec2::ZERO,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn briefing_waits_and_rejects_flight_and_wrong_pointer_commands() {
        let mut game = Game::new();
        let before = game.clone();
        for _ in 0..50 {
            game.step(&Input {
                up: true,
                secondary: true,
                tertiary: true,
                slot: Some(1),
                click: true,
                pointer: Vec2::new(0., 0.),
                ..Input::default()
            });
        }
        assert_eq!(game, before);
        game.step(&Input {
            click: true,
            pointer: Vec2::new(200., 500.),
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Flight);
        assert_eq!(game.ticks, 0);
    }
    #[test]
    fn thrust_rotates_acceleration_and_loaded_mass_reduces_lift() {
        let mut empty = airborne();
        let mut loaded = empty.clone();
        loaded.loaded = true;
        let command = Input {
            right: true,
            up: true,
            ..Input::default()
        };
        empty.step(&command);
        loaded.step(&command);
        assert!(empty.angle > 0. && empty.velocity.x > 0. && empty.velocity.y < 0.);
        assert!(loaded.velocity.y > empty.velocity.y);
        assert!(empty.fuel < 120.);
        assert_eq!(empty.fuel, loaded.fuel);
        assert!(empty.thrusting);
        assert!(!empty.braking);
    }
    #[test]
    fn coast_keeps_inertia_and_integrates_planet_gravity_and_wind() {
        let mut game = airborne();
        game.index = 1;
        game.velocity = Vec2::new(20., 0.);
        let x = game.pos.x;
        game.step(&Input::default());
        assert!(game.pos.x > x);
        assert!(game.velocity.x > 20.);
        assert!(game.velocity.y > 0.);
        assert_eq!(game.fuel, 120.);
        let falling = game.velocity.y;
        game.step(&Input::default());
        assert!(game.velocity.y > falling);
    }
    #[test]
    fn retro_brake_spends_fuel_reduces_speed_and_levels_attitude() {
        let mut game = airborne();
        game.velocity = Vec2::new(40., 60.);
        game.angle = 0.8;
        let initial = game.velocity.length();
        game.step(&Input {
            down: true,
            up: true,
            ..Input::default()
        });
        assert!(game.velocity.length() < initial);
        assert!(game.angle < 0.8);
        assert!(game.fuel < 120.);
        assert!(game.braking);
        assert!(!game.thrusting);
    }
    #[test]
    fn empty_tank_cannot_thrust_or_retro_brake() {
        let mut game = airborne();
        game.fuel = 0.;
        game.velocity = Vec2::new(10., 10.);
        let mut coast = game.clone();
        game.step(&Input {
            up: true,
            down: true,
            ..Input::default()
        });
        coast.step(&Input::default());
        assert_eq!(game, coast);
        assert!(!game.thrusting && !game.braking);
        assert_eq!(game.fuel, 0.);
    }
    #[test]
    fn safe_touchdown_requires_pad_width_speed_and_attitude() {
        let mut good = over_pad(1);
        good.velocity.x = 3.;
        good.angle = 0.1;
        assert_eq!(good.step(&Input::default()), Cue::Good);
        assert_eq!(good.docked, Some(1));
        assert_eq!(good.velocity, Vec2::ZERO);
        assert_eq!(good.hull, 100.);
        let mut tilted = over_pad(1);
        tilted.angle = 0.8;
        tilted.loaded = true;
        assert_eq!(tilted.step(&Input::default()), Cue::Bad);
        assert!(tilted.docked.is_none());
        assert!(tilted.hull < 100. && tilted.cargo < 100.);
        let mut fast = over_pad(1);
        fast.velocity.y = 60.;
        fast.step(&Input::default());
        assert!(fast.hull < 50.);
        assert!(fast.docked.is_none());
        let mut edge = over_pad(1);
        edge.pos.x += PAD_HALF - 2.;
        edge.step(&Input::default());
        assert!(edge.docked.is_none());
    }
    #[test]
    fn gear_refit_broadens_real_touchdown_limits() {
        let mut bare = over_pad(1);
        bare.velocity = Vec2::new(14., 22.);
        bare.angle = 0.30;
        let mut geared = bare.clone();
        geared.upgrades[2] = 2;
        bare.step(&Input::default());
        geared.step(&Input::default());
        assert!(bare.docked.is_none());
        assert_eq!(geared.docked, Some(1));
        assert_eq!(geared.hull, 100.);
    }
    #[test]
    fn rock_and_border_impacts_damage_the_ship_and_fragile_cargo() {
        let mut game = airborne();
        game.loaded = true;
        game.pos = Vec2::new(175., game.ground(175.) - FEET);
        game.velocity = Vec2::new(0., 40.);
        game.step(&Input::default());
        assert!(game.hull < 100. && game.cargo < 100.);
        assert!(game.velocity.y < 0.);
        assert!(game.docked.is_none());
        let after = game.hull;
        assert_eq!(game.damage(30., true), Cue::None);
        assert_eq!(game.hull, after);
        let mut edge = airborne();
        edge.pos.x = 7.;
        edge.velocity.x = -30.;
        edge.step(&Input::default());
        assert!(edge.hull < 100.);
        assert!(edge.pos.x >= HALF_BODY);
    }
    #[test]
    fn service_requires_a_landing_and_parcel_loading_is_not_repeatable() {
        let mut flying = airborne();
        let mut neutral = flying.clone();
        flying.step(&Input {
            secondary: true,
            ..Input::default()
        });
        neutral.step(&Input::default());
        assert_eq!(flying, neutral);
        let mut game = over_pad(1);
        game.step(&Input::default());
        game.fuel = 50.;
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert!(game.loaded);
        assert_eq!(game.fuel, 78.);
        assert_eq!(game.score, 100);
        assert_eq!(game.reserves[1], 0.);
        let before = (game.fuel, game.score, game.credits, game.cargo);
        for _ in 0..20 {
            assert_eq!(
                game.step(&Input {
                    secondary: true,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(before, (game.fuel, game.score, game.credits, game.cargo));
    }
    #[test]
    fn outpost_supplies_and_repair_kit_are_finite() {
        let mut game = over_pad(2);
        game.step(&Input::default());
        game.fuel = 0.;
        game.hull = 35.;
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.fuel, 65.);
        assert_eq!(game.hull, 65.);
        assert!(game.repaired[2]);
        for _ in 0..10 {
            game.step(&Input {
                secondary: true,
                ..Input::default()
            });
        }
        assert_eq!(game.fuel, 65.);
        assert_eq!(game.hull, 65.);
        assert_eq!(game.score, 0);
    }
    #[test]
    fn destination_demands_cargo_and_awards_delivery_once() {
        let mut game = over_pad(3);
        game.step(&Input::default());
        assert_eq!(
            game.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.delivered(), 0);
        game.loaded = true;
        game.cargo = 70.;
        game.hull = 70.;
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Debrief);
        assert_eq!(game.stamps[0], 2);
        assert_eq!(game.delivered(), 1);
        let before = game.clone();
        for _ in 0..10 {
            game.step(&Input {
                secondary: true,
                ..Input::default()
            });
        }
        assert_eq!(game, before);
    }
    #[test]
    fn beacon_reward_is_once_and_recovery_rolls_it_back() {
        let mut game = airborne();
        game.pos = Vec2::from_array(game.planet().beacon);
        game.fuel = 50.;
        game.step(&Input::default());
        assert_eq!(game.score, 250);
        assert_eq!(game.credits, 80);
        assert_eq!(game.fuel, 65.);
        game.step(&Input::default());
        assert_eq!(game.score, 250);
        assert_eq!(game.credits, 80);
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Wreck);
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Briefing);
        assert_eq!(game.score, 0);
        assert_eq!(game.credits, 60);
        assert!(!game.beacon[0]);
        assert_eq!(game.permits, 2);
    }
    #[test]
    fn deadline_and_three_recovery_permits_lead_to_a_frozen_loss() {
        let mut game = launched();
        game.ticks = game.planet().deadline * 60 - 1;
        game.step(&Input::default());
        assert_eq!(game.phase, Phase::Wreck);
        for remaining in (0..3).rev() {
            game.step(&Input {
                primary: true,
                ..Input::default()
            });
            assert_eq!(game.permits, remaining);
            assert_eq!(game.phase, Phase::Briefing);
            game.step(&Input {
                primary: true,
                ..Input::default()
            });
            game.step(&Input {
                tertiary: true,
                ..Input::default()
            });
        }
        assert_eq!(game.outcome(), Outcome::Lost);
        let frozen = game.clone();
        game.step(&Input {
            primary: true,
            secondary: true,
            tertiary: true,
            up: true,
            ..Input::default()
        });
        assert_eq!(game, frozen);
        assert_eq!(Game::new().permits, 3);
    }
    #[test]
    fn refits_are_paid_bounded_and_have_physical_effects() {
        let mut game = Game::new();
        game.phase = Phase::Debrief;
        game.credits = 500;
        let old_engine = game.engine();
        game.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        assert_eq!(game.credits, 390);
        assert!(game.engine() > old_engine);
        game.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        assert_eq!(game.upgrades[0], 2);
        assert_eq!(game.credits, 220);
        let full = game.clone();
        game.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        assert_eq!(game, full);
        game.credits = 1;
        let poor = game.clone();
        game.step(&Input {
            slot: Some(2),
            ..Input::default()
        });
        assert_eq!(game, poor);
        game.credits = 100;
        game.step(&Input {
            click: true,
            pointer: Vec2::new(150., 360.),
            ..Input::default()
        });
        assert_eq!(game.upgrades[1], 1);
        assert_eq!(game.capacity(), 150.);
        game.step(&Input {
            click: true,
            pointer: Vec2::new(150., 505.),
            ..Input::default()
        });
        assert_eq!(game.index, 1);
        assert_eq!(game.phase, Phase::Briefing);
        assert_eq!(game.fuel, 150.);
    }
    #[test]
    fn all_authored_worlds_have_distinct_roles_clear_beacons_and_flat_pads() {
        for (index, planet) in PLANETS.iter().enumerate() {
            let mut game = Game::new();
            game.index = index;
            for role in [Role::Depot, Role::Pickup, Role::Delivery, Role::Outpost] {
                assert_eq!(planet.roles.iter().filter(|&&r| r == role).count(), 1);
            }
            for i in 0..4 {
                let pad = game.pad(i);
                for dx in [-24., 0., 24.] {
                    assert_eq!(game.ground(pad.x + dx), pad.y);
                }
            }
            assert!(planet.beacon[1] + 40. < game.ground(planet.beacon[0]));
            assert!(planet.gravity > 0. && planet.deadline >= 180);
        }
    }
}
