//! Deterministic railway rules. No drawing, platform input, storage, or clocks.
use crate::content::{SHIFTS, Shift, Side};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_math::{Vec2, vec2};

const MAX_TRAINS: usize = 12;
const TICKS_PER_SECOND: u32 = 60;
pub(crate) const EDGES: [(usize, usize); 11] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (4, 5),
    (5, 6),
    (6, 7),
    (2, 8),
    (2, 9),
    (5, 10),
    (5, 11),
];
pub(crate) const CONTROL_NAMES: [&str; 6] = [
    "West gate",
    "West points",
    "West bridge",
    "East bridge",
    "East points",
    "East gate",
];
pub(crate) const ROUTE_NAMES: [&str; 3] = ["Straight", "North", "South"];
pub(crate) const STATION_NAMES: [&str; 4] =
    ["Abbey (A)", "Orchard (B)", "Quarry (C)", "Harbour (D)"];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum TrainState {
    #[default]
    Waiting,
    At(usize),
    Moving {
        from: usize,
        to: usize,
        progress: u32,
        duration: u32,
    },
    Delivered,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Train {
    pub state: TrainState,
    pub delivered_at: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Briefing,
    Working,
    Cleared,
    Incident,
    Won,
    Lost,
}

/// A complete eight-shift railway career, advanced by one fixed 60 Hz tick.
///
/// Signals start red. Start the briefing with Space/Enter (`primary`), then
/// manipulate controls through edge-triggered input. `bot_input` produces legal
/// controls suitable for a reproducible headless run; it has no privileged path.
///
/// ```
/// use signalbox::Game;
/// use ferravex_arcade::{Input, Outcome};
/// let mut game = Game::new();
/// game.tick(&Input { primary: true, ..Input::default() });
/// for tick in 0..120 {
///     let input = game.bot_input(tick);
///     game.tick(&input);
/// }
/// assert_eq!(game.outcome(), Outcome::Playing);
/// ```
pub struct Game {
    pub(crate) shift: usize,
    pub(crate) phase: Phase,
    pub(crate) dispatching: bool,
    pub(crate) time: u32,
    pub(crate) trains: [Train; MAX_TRAINS],
    pub(crate) points: [usize; 2],
    pub(crate) signals: [bool; 4],
    pub(crate) selected: usize,
    pub(crate) score: u32,
    pub(crate) provisional: u32,
    pub(crate) delays: u32,
    pub(crate) medals: [u8; 8],
    pub(crate) retries: u8,
    pub(crate) incidents: u8,
    pub(crate) message: String,
    pub(crate) message_ticks: u32,
    pub(crate) delivered_total: u32,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// Begin a new career at the first briefing with three shift retries.
    pub fn new() -> Self {
        Self {
            shift: 0,
            phase: Phase::Briefing,
            dispatching: false,
            time: 0,
            trains: [Train::default(); MAX_TRAINS],
            points: [0; 2],
            signals: [false; 4],
            selected: 1,
            score: 0,
            provisional: 0,
            delays: 0,
            medals: [0; 8],
            retries: 3,
            incidents: 0,
            message: String::new(),
            message_ticks: 0,
            delivered_total: 0,
        }
    }

    pub(crate) fn schedule(&self) -> &'static Shift {
        &SHIFTS[self.shift]
    }

    pub(crate) fn nodes(&self) -> [Vec2; 12] {
        let s = self.schedule();
        [
            vec2(48., s.west_y),
            vec2(135., s.west_y),
            vec2(246., s.west_y),
            vec2(355., s.bridge_y),
            vec2(444., s.bridge_y),
            vec2(553., s.east_y),
            vec2(647., s.east_y),
            vec2(704., s.east_y),
            vec2(246., 180.),
            vec2(246., 443.),
            vec2(553., 180.),
            vec2(553., 443.),
        ]
    }

    pub(crate) fn position(&self, index: usize) -> Option<Vec2> {
        let nodes = self.nodes();
        match self.trains[index].state {
            TrainState::At(n) => Some(nodes[n]),
            TrainState::Moving {
                from,
                to,
                progress,
                duration,
            } => Some(nodes[from].lerp(nodes[to], progress as f32 / duration as f32)),
            _ => None,
        }
    }

    pub(crate) fn direction(&self, index: usize) -> Vec2 {
        let nodes = self.nodes();
        if let TrainState::Moving { from, to, .. } = self.trains[index].state {
            (nodes[to] - nodes[from]).normalize_or_zero()
        } else if self.schedule().trains[index].side == Side::West {
            Vec2::X
        } else {
            -Vec2::X
        }
    }

    pub(crate) fn bridge_closed(&self) -> bool {
        self.schedule().closure.is_some_and(|(start, end)| {
            (start * TICKS_PER_SECOND..end * TICKS_PER_SECOND).contains(&self.time)
        })
    }

    pub(crate) fn delivered(&self) -> usize {
        self.trains[..self.schedule().trains.len()]
            .iter()
            .filter(|t| t.state == TrainState::Delivered)
            .count()
    }

    pub(crate) fn control_center(control: usize) -> Vec2 {
        vec2(77. + control as f32 * 112., 539.)
    }

    fn note(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.message_ticks = 240;
    }

    fn reset_shift(&mut self) {
        self.phase = Phase::Briefing;
        self.dispatching = false;
        self.time = 0;
        self.trains = [Train::default(); MAX_TRAINS];
        self.points = [0; 2];
        self.signals = [false; 4];
        self.provisional = 0;
        self.delays = 0;
        self.selected = 1;
        self.message.clear();
        self.message_ticks = 0;
    }

    fn retry(&mut self) -> Cue {
        if self.retries == 0 {
            self.phase = Phase::Lost;
            self.note("Your safety certificate has been withdrawn.");
            return Cue::Bad;
        }
        self.retries -= 1;
        self.reset_shift();
        Cue::Action
    }

    fn incident(&mut self, message: String) -> Cue {
        self.incidents = self.incidents.saturating_add(1);
        self.phase = if self.retries == 0 {
            Phase::Lost
        } else {
            Phase::Incident
        };
        self.dispatching = false;
        self.note(message);
        Cue::Bad
    }

    fn points_locked(&self, point: usize) -> bool {
        let at = self.nodes()[if point == 0 { 2 } else { 5 }];
        (0..self.schedule().trains.len()).any(|i| {
            self.position(i)
                .is_some_and(|p| p.distance_squared(at) < 38. * 38.)
        })
    }

    fn operate(&mut self, control: usize, backwards: bool) -> Cue {
        self.selected = control;
        if let Some(point) = match control {
            1 => Some(0),
            4 => Some(1),
            _ => None,
        } {
            if self.points_locked(point) {
                self.note("Points locked: wait until the nearby train is clear.");
                return Cue::None;
            }
            self.points[point] = (self.points[point] + if backwards { 2 } else { 1 }) % 3;
            self.note(format!(
                "{} → {}",
                CONTROL_NAMES[control], ROUTE_NAMES[self.points[point]]
            ));
        } else {
            let signal = match control {
                0 => 0,
                2 => 1,
                3 => 2,
                5 => 3,
                _ => return Cue::None,
            };
            if matches!(signal, 1 | 2) && self.bridge_closed() && !self.signals[signal] {
                self.note("Bridge inspection: both entrances remain red.");
                return Cue::None;
            }
            self.signals[signal] = !self.signals[signal];
            self.note(format!(
                "{} {}",
                CONTROL_NAMES[control],
                if self.signals[signal] {
                    "green: admits one service"
                } else {
                    "red: hold"
                }
            ));
        }
        Cue::Action
    }

    /// Advance one 1/60-second simulation tick and return accepted-event feedback.
    /// Terminal careers are immutable; planning stops railway time but permits
    /// point and signal changes. Input edges are consumed by the shared host.
    pub fn tick(&mut self, input: &Input) -> Cue {
        if matches!(self.phase, Phase::Won | Phase::Lost) {
            return Cue::None;
        }
        match self.phase {
            Phase::Briefing => {
                if input.primary || (input.click && Self::main_button(input.pointer)) {
                    self.phase = Phase::Working;
                    self.dispatching = true;
                    self.note("Set points, then clear a signal. X freezes time for planning.");
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Cleared => {
                if input.primary || (input.click && Self::main_button(input.pointer)) {
                    self.shift += 1;
                    self.reset_shift();
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Incident => {
                if input.tertiary || (input.click && Self::main_button(input.pointer)) {
                    return self.retry();
                }
                return Cue::None;
            }
            _ => {}
        }
        let mut cue = Cue::None;
        if input.tertiary {
            return self.retry();
        }
        if input.secondary
            || (input.click
                && (738. ..932.).contains(&input.pointer.x)
                && (506. ..558.).contains(&input.pointer.y))
        {
            self.dispatching = !self.dispatching;
            cue = Cue::Action;
        }
        if input.left_pressed {
            self.selected = (self.selected + 5) % 6;
        }
        if input.right_pressed {
            self.selected = (self.selected + 1) % 6;
        }
        if let Some(slot) = input.slot.filter(|s| (1..=5).contains(s)) {
            self.selected = slot - 1;
        }
        if input.primary || input.up_pressed || input.down_pressed {
            cue = self.operate(self.selected, input.down_pressed);
        }
        if input.click {
            for control in 0..6 {
                let at = Self::control_center(control);
                if (at.x - 52. ..at.x + 52.).contains(&input.pointer.x)
                    && (515. ..565.).contains(&input.pointer.y)
                {
                    cue = self.operate(control, false);
                    break;
                }
            }
        }
        if self.dispatching {
            let event = self.advance_railway();
            if event != Cue::None {
                cue = event;
            }
        }
        cue
    }

    fn main_button(pointer: Vec2) -> bool {
        (100. ..694.).contains(&pointer.x) && (442. ..490.).contains(&pointer.y)
    }

    fn advance_railway(&mut self) -> Cue {
        self.time += 1;
        self.message_ticks = self.message_ticks.saturating_sub(1);
        let schedule = self.schedule();
        if self.bridge_closed() {
            self.signals[1] = false;
            self.signals[2] = false;
        }
        let mut cue = Cue::None;
        for i in 0..schedule.trains.len() {
            let plan = schedule.trains[i];
            if self.trains[i].state == TrainState::Waiting && self.time >= plan.release * 60 {
                let (source, gate) = if plan.side == Side::West {
                    (0, 1)
                } else {
                    (7, 6)
                };
                let yard_busy = self.trains.iter().any(|train| match train.state {
                    TrainState::At(n) => n == source || n == gate,
                    TrainState::Moving { from, to, .. } => {
                        from == source || to == source || from == gate
                    }
                    _ => false,
                });
                if !yard_busy {
                    self.trains[i].state = TrainState::At(source);
                }
            }
            match self.trains[i].state {
                TrainState::Moving {
                    from,
                    to,
                    progress,
                    duration,
                } => {
                    self.trains[i].state = if progress + 1 >= duration {
                        TrainState::At(to)
                    } else {
                        TrainState::Moving {
                            from,
                            to,
                            progress: progress + 1,
                            duration,
                        }
                    };
                }
                TrainState::At(node) => {
                    if node >= 8 {
                        if node - 8 != plan.destination {
                            return self.incident(format!(
                                "{} reached {} instead of {}. Check both points before release.",
                                plan.name,
                                STATION_NAMES[node - 8],
                                STATION_NAMES[plan.destination]
                            ));
                        }
                        let late = self.time.saturating_sub(plan.due * 60);
                        self.delays += late;
                        self.provisional += 200 + 180_u32.saturating_sub(late / 12);
                        self.trains[i].state = TrainState::Delivered;
                        self.trains[i].delivered_at = Some(self.time);
                        self.note(format!(
                            "{} delivered to {} {}",
                            plan.name,
                            STATION_NAMES[plan.destination],
                            if late == 0 {
                                "on time".to_owned()
                            } else {
                                format!("({}s late)", late.div_ceil(60))
                            }
                        ));
                        cue = Cue::Good;
                        continue;
                    }
                    let eastbound = plan.side == Side::West;
                    if (node == 0 && !eastbound) || (node == 7 && eastbound) {
                        return self.incident(format!("{} left the railway without reaching {}. Route it north or south at its station.", plan.name, STATION_NAMES[plan.destination]));
                    }
                    let signal = match (node, eastbound) {
                        (1, true) => Some(0),
                        (3, true) => Some(1),
                        (4, false) => Some(2),
                        (6, false) => Some(3),
                        _ => None,
                    };
                    if let Some(signal) = signal {
                        if !self.signals[signal] {
                            continue;
                        }
                        self.signals[signal] = false;
                    }
                    let next = match node {
                        2 => match self.points[0] {
                            1 => 8,
                            2 => 9,
                            _ => {
                                if eastbound {
                                    3
                                } else {
                                    1
                                }
                            }
                        },
                        5 => match self.points[1] {
                            1 => 10,
                            2 => 11,
                            _ => {
                                if eastbound {
                                    6
                                } else {
                                    4
                                }
                            }
                        },
                        _ => {
                            if eastbound {
                                node + 1
                            } else {
                                node - 1
                            }
                        }
                    };
                    let nodes = self.nodes();
                    let duration = (nodes[node].distance(nodes[next])
                        / (plan.service.speed() * schedule.speed))
                        .ceil() as u32;
                    self.trains[i].state = TrainState::Moving {
                        from: node,
                        to: next,
                        progress: 0,
                        duration: duration.max(1),
                    };
                }
                _ => {}
            }
        }
        for a in 0..schedule.trains.len() {
            if let Some(pa) = self.position(a) {
                for b in (a + 1)..schedule.trains.len() {
                    if self
                        .position(b)
                        .is_some_and(|pb| pa.distance_squared(pb) < 20. * 20.)
                    {
                        return self.incident(format!("{} and {} collided. Keep opposing or following trains separated; signals do not check the route for you.", schedule.trains[a].name, schedule.trains[b].name));
                    }
                }
            }
        }
        if self.delivered() == schedule.trains.len() {
            let medal = if self.delays == 0 {
                3
            } else if self.delays <= 45 * 60 {
                2
            } else {
                1
            };
            self.medals[self.shift] = medal;
            self.score += self.provisional + 300 + u32::from(medal) * 150;
            self.delivered_total += schedule.trains.len() as u32;
            self.phase = if self.shift + 1 == SHIFTS.len() {
                Phase::Won
            } else {
                Phase::Cleared
            };
            self.dispatching = false;
            Cue::Win
        } else if self.time >= schedule.closing * 60 {
            self.incident("The end-of-shift bell rang with trains still undelivered. Plan routes first, then keep the railway moving.".to_owned())
        } else {
            cue
        }
    }

    /// Read the career result without advancing time.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Won => Outcome::Won,
            Phase::Lost => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }

    /// A conservative dispatcher using only the same button and pointer actions
    /// as a player. It sends services in timetable order, so faster human routing
    /// can earn a better punctuality grade. The tick argument is intentionally
    /// unused: decisions read the actual railway, not hidden playback events.
    pub fn bot_input(&self, _tick: u64) -> Input {
        let mut input = Input::default();
        match self.phase {
            Phase::Briefing | Phase::Cleared => {
                input.primary = true;
                return input;
            }
            Phase::Incident => {
                input.tertiary = true;
                return input;
            }
            Phase::Won | Phase::Lost => return input,
            Phase::Working => {}
        }
        if !self.dispatching {
            input.secondary = true;
            return input;
        }
        let Some(index) = (0..self.schedule().trains.len())
            .find(|i| self.trains[*i].state != TrainState::Delivered)
        else {
            return input;
        };
        let plan = self.schedule().trains[index];
        let desired = if plan.destination < 2 {
            [plan.destination + 1, 0]
        } else {
            [0, plan.destination - 1]
        };
        for (point, control) in [(0, 1), (1, 4)] {
            if self.points[point] != desired[point] && !self.points_locked(point) {
                input.click = true;
                input.pointer = Self::control_center(control);
                return input;
            }
        }
        let state = self.trains[index].state;
        let gate = if plan.side == Side::West { 0 } else { 3 };
        let near_origin = match state {
            TrainState::At(n) => {
                if plan.side == Side::West {
                    n <= 1
                } else {
                    n == 6 || n == 7
                }
            }
            TrainState::Moving { from, .. } => from == if plan.side == Side::West { 0 } else { 7 },
            _ => false,
        };
        if near_origin && !self.signals[gate] {
            input.click = true;
            input.pointer = Self::control_center(if gate == 0 { 0 } else { 5 });
            return input;
        }
        let westbound = plan.side == Side::East;
        let bridge_signal = if westbound { 2 } else { 1 };
        let before_bridge = match state {
            TrainState::At(n) => n == if westbound { 4 } else { 3 },
            TrainState::Moving { from, to, .. } => {
                (from, to) == if westbound { (5, 4) } else { (2, 3) }
            }
            _ => false,
        };
        if before_bridge && !self.signals[bridge_signal] && !self.bridge_closed() {
            input.click = true;
            input.pointer = Self::control_center(if westbound { 3 } else { 2 });
        }
        input
    }

    pub(crate) fn rank(&self) -> &'static str {
        match self.medals.iter().map(|m| u32::from(*m)).sum::<u32>() {
            24 => "Master of the Line",
            20..=23 => "Chief Dispatcher",
            15..=19 => "Senior Signalkeeper",
            _ => "Keeper of the Brass Key",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press_primary(game: &mut Game) {
        game.tick(&Input {
            primary: true,
            ..Input::default()
        });
    }
    fn click(game: &mut Game, control: usize) -> Cue {
        game.tick(&Input {
            click: true,
            pointer: Game::control_center(control),
            ..Input::default()
        })
    }
    fn wait(game: &mut Game, count: u32) {
        for _ in 0..count {
            game.tick(&Input::default());
        }
    }
    fn wrong_platform(game: &mut Game) {
        press_primary(game);
        click(game, 1);
        click(game, 1); // W1 needs A, select B.
        click(game, 0);
        for _ in 0..1000 {
            game.tick(&Input::default());
            if game.phase != Phase::Working {
                break;
            }
        }
    }

    #[test]
    fn all_eight_authored_shifts_are_winnable_through_player_controls() {
        let mut game = Game::new();
        let mut last_score = 0;
        for tick in 0..120_000 {
            game.tick(&game.bot_input(tick));
            assert!(game.score >= last_score);
            last_score = game.score;
            assert_eq!(
                game.incidents,
                0,
                "shift {}: {}",
                game.shift + 1,
                game.message
            );
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(
            game.outcome(),
            Outcome::Won,
            "shift {} tick {}: {}",
            game.shift,
            game.time,
            game.message
        );
        assert_eq!(game.medals.len(), 8);
        assert!(game.medals.iter().all(|medal| *medal > 0));
        assert_eq!(game.delivered_total, 61);
        assert!(game.score > 20_000);
        let finished = (game.time, game.score, game.delivered_total, game.medals);
        for _ in 0..60 {
            assert_eq!(
                game.tick(&Input {
                    primary: true,
                    tertiary: true,
                    click: true,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(
            (game.time, game.score, game.delivered_total, game.medals),
            finished
        );
    }

    #[test]
    fn red_signal_holds_train_and_held_button_is_not_an_edge() {
        let mut game = Game::new();
        press_primary(&mut game);
        for _ in 0..600 {
            game.tick(&Input {
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!(game.trains[0].state, TrainState::At(1));
        assert_eq!(game.points, [0; 2]);
        assert_eq!(game.delivered(), 0);
        assert_eq!(game.signals, [false; 4]);
    }

    #[test]
    fn planning_freezes_time_but_accepts_route_changes() {
        let mut game = Game::new();
        press_primary(&mut game);
        wait(&mut game, 150);
        game.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        let before = (game.time, game.trains);
        click(&mut game, 1);
        wait(&mut game, 600);
        assert_eq!(game.points, [1, 0]);
        assert_eq!((game.time, game.trains), before);
        game.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.time, before.0 + 1);
    }

    #[test]
    fn wrong_station_is_an_incident_not_a_delivery_or_score() {
        let mut game = Game::new();
        wrong_platform(&mut game);
        assert_eq!(game.phase, Phase::Incident);
        assert!(game.message.contains("instead of"));
        assert_eq!(game.delivered(), 0);
        assert_eq!(game.score, 0);
        assert_eq!(game.provisional, 0);
    }

    #[test]
    fn opposing_trains_collide_on_the_shared_bridge() {
        let mut game = Game::new();
        press_primary(&mut game);
        click(&mut game, 0); // W1 goes straight and waits at west bridge.
        wait(&mut game, 600);
        click(&mut game, 5); // E1 goes straight and waits at east bridge.
        wait(&mut game, 600);
        assert_eq!(game.trains[0].state, TrainState::At(3));
        assert_eq!(game.trains[1].state, TrainState::At(4));
        click(&mut game, 2);
        click(&mut game, 3);
        wait(&mut game, 120);
        assert_eq!(game.phase, Phase::Incident);
        assert!(game.message.contains("collided"));
    }

    #[test]
    fn points_reject_changes_under_a_train_and_signals_reset_after_release() {
        let mut game = Game::new();
        press_primary(&mut game);
        click(&mut game, 1);
        click(&mut game, 0);
        for _ in 0..500 {
            if game.points_locked(0) {
                break;
            }
            game.tick(&Input::default());
        }
        assert!(game.points_locked(0));
        assert!(!game.signals[0]);
        assert_eq!(click(&mut game, 1), Cue::None);
        assert_eq!(game.points[0], 1);
        assert!(game.message.contains("locked"));
    }

    #[test]
    fn retries_are_bounded_and_a_lost_career_is_immutable() {
        let mut game = Game::new();
        for attempt in 0..4 {
            wrong_platform(&mut game);
            if attempt < 3 {
                assert_eq!(game.phase, Phase::Incident);
                game.tick(&Input {
                    tertiary: true,
                    ..Input::default()
                });
                assert_eq!(game.phase, Phase::Briefing);
                assert_eq!(game.time, 0);
                assert_eq!(game.score, 0);
                assert_eq!(game.provisional, 0);
                assert_eq!(game.points, [0; 2]);
            }
        }
        assert_eq!(game.outcome(), Outcome::Lost);
        let before = (game.time, game.trains, game.score, game.incidents);
        for _ in 0..120 {
            assert_eq!(
                game.tick(&Input {
                    primary: true,
                    tertiary: true,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!((game.time, game.trains, game.score, game.incidents), before);
    }

    #[test]
    fn end_of_shift_deadline_and_invalid_slots_are_safe() {
        let mut game = Game::new();
        press_primary(&mut game);
        for _ in 0..game.schedule().closing * 60 + 100 {
            game.tick(&Input {
                slot: Some(99),
                ..Input::default()
            });
        }
        assert_eq!(game.phase, Phase::Incident);
        assert!(game.message.contains("end-of-shift"));
        assert_eq!(game.delivered(), 0);
        assert_eq!(game.selected, 1);
    }

    #[test]
    fn bridge_inspection_rejects_opening_without_spending_a_retry() {
        let mut game = Game::new();
        for tick in 0..30_000 {
            if game.shift == 3 && game.phase == Phase::Working {
                break;
            }
            game.tick(&game.bot_input(tick));
        }
        assert_eq!(game.shift, 3);
        wait(&mut game, 18 * 60);
        assert!(game.bridge_closed());
        assert_eq!(click(&mut game, 2), Cue::None);
        assert_eq!(click(&mut game, 3), Cue::None);
        assert!(!game.signals[1] && !game.signals[2]);
        assert_eq!(game.retries, 3);
    }

    #[test]
    fn a_completed_shift_is_scored_once_and_waiting_does_not_farm_it() {
        let mut game = Game::new();
        for tick in 0..8000 {
            game.tick(&game.bot_input(tick));
            if game.phase == Phase::Cleared {
                break;
            }
        }
        assert_eq!(game.phase, Phase::Cleared);
        let scored = game.score;
        let delivered = game.delivered_total;
        wait(&mut game, 600);
        assert_eq!(game.score, scored);
        assert_eq!(game.delivered_total, delivered);
        press_primary(&mut game);
        assert_eq!(game.shift, 1);
        assert_eq!(game.provisional, 0);
        assert_eq!(game.score, scored);
    }
}
