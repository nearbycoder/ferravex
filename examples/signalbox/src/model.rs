//! Deterministic railway rules. No drawing, platform input, storage, or clocks.
use crate::content::{SHIFTS, Shift, Side};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_math::{Vec2, vec2};

const MAX_TRAINS: usize = 12;
const TICKS_PER_SECOND: u32 = 60;
/// Capacity of one tick's presentation event list; extra events are dropped.
const MAX_EVENTS: usize = 16;
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
/// The signal each switchboard control operates (`None` for the points).
pub(crate) const CONTROL_SIGNAL: [Option<usize>; 6] =
    [Some(0), None, Some(1), Some(2), None, Some(3)];
/// The points each switchboard control operates (`None` for the signals).
pub(crate) const CONTROL_POINTS: [Option<usize>; 6] = [None, Some(0), None, None, Some(1), None];
/// The control that operates each signal lamp.
pub(crate) const SIGNAL_CONTROL: [usize; 4] = [0, 2, 3, 5];
/// Map lamps sit beside their guarded node: `(node, vertical offset)`.
const LAMPS: [(usize, f32); 4] = [(1, -30.), (3, -30.), (4, 30.), (6, 30.)];

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

/// Why the line stopped. Kept for the incident card and its effects; the
/// rules never read it back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Incident {
    /// `train` reached `station` instead of its destination.
    WrongPlatform { train: usize, station: usize },
    /// `train` left the railway at yard `node` without a delivery.
    LeftLine { train: usize, node: usize },
    /// Trains `a` and `b` collided at `at`.
    Collision { a: usize, b: usize, at: Vec2 },
    /// The closing bell rang with `waiting` services undelivered.
    Bell { waiting: usize },
}

/// Something one tick accepted, recorded only for presentation effects.
/// Rules never read these; the list is cleared at the start of every tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Event {
    /// The briefing was accepted and the railway clock started.
    Started,
    /// A retry reopened the current shift.
    Retried,
    /// The railway clock resumed (`true`) or stopped for planning (`false`).
    Clock(bool),
    /// The dispatcher switched a signal lamp.
    Signal { signal: usize, green: bool },
    /// The dispatcher moved a set of points to `route`.
    Points { point: usize, route: usize },
    /// Points refused to move because a train is too close.
    Locked { point: usize },
    /// A bridge signal refused to clear during an inspection.
    Closed { signal: usize },
    /// A train passed a green signal, which returned to red.
    Passed { signal: usize },
    /// A train entered the railway from its yard.
    Entered { train: usize },
    /// A train missed its due time.
    Late { train: usize },
    /// A train reached its platform; `late` is in ticks.
    Delivered {
        train: usize,
        late: u32,
        earned: u32,
    },
    /// The bridge inspection began (`open: false`) or ended.
    Bridge { open: bool },
    /// A safety incident stopped the line.
    Incident(Incident),
    /// Every service is home; `medal` is one to three stars.
    Cleared { medal: u8 },
}

/// Bounded, allocation-free list of the events one tick accepted.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Events {
    items: [Event; MAX_EVENTS],
    len: usize,
}

impl Events {
    const fn new() -> Self {
        Self {
            items: [Event::Started; MAX_EVENTS],
            len: 0,
        }
    }
    fn clear(&mut self) {
        self.len = 0;
    }
    fn push(&mut self, event: Event) {
        if self.len < MAX_EVENTS {
            self.items[self.len] = event;
            self.len += 1;
        }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Event> {
        self.items[..self.len].iter()
    }
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
    /// Plain-language account of the latest action, for status readers.
    pub(crate) message: String,
    pub(crate) delivered_total: u32,
    /// The incident holding the line, if any.
    pub(crate) incident: Option<Incident>,
    /// Presentation events accepted by the most recent tick.
    pub(crate) events: Events,
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
            delivered_total: 0,
            incident: None,
            events: Events::new(),
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

    /// The switchboard control under `pointer`, if any.
    pub(crate) fn control_at(pointer: Vec2) -> Option<usize> {
        (0..6).find(|&control| {
            let at = Self::control_center(control);
            (at.x - 52. ..at.x + 52.).contains(&pointer.x) && (515. ..565.).contains(&pointer.y)
        })
    }

    /// Map position of a signal lamp: west gate, west bridge, east bridge,
    /// east gate.
    pub(crate) fn lamp(&self, signal: usize) -> Vec2 {
        let (node, dy) = LAMPS[signal.min(3)];
        self.nodes()[node] + vec2(0., dy)
    }

    /// Map position of a set of points: west, east.
    pub(crate) fn junction(&self, point: usize) -> Vec2 {
        self.nodes()[if point == 0 { 2 } else { 5 }]
    }

    /// The control whose lamp or points are drawn on the map under `pointer`.
    pub(crate) fn map_control_at(&self, pointer: Vec2) -> Option<usize> {
        (0..4)
            .find(|&signal| pointer.distance_squared(self.lamp(signal)) < 18. * 18.)
            .map(|signal| SIGNAL_CONTROL[signal])
            .or_else(|| {
                [(0, 1), (1, 4)]
                    .into_iter()
                    .find(|&(point, _)| pointer.distance_squared(self.junction(point)) < 20. * 20.)
                    .map(|(_, control)| control)
            })
    }

    /// The run/plan clock button.
    pub(crate) fn clock_button(pointer: Vec2) -> bool {
        (738. ..932.).contains(&pointer.x) && (506. ..558.).contains(&pointer.y)
    }

    /// The briefing, shift-cleared and incident cards' action button.
    pub(crate) fn main_button(pointer: Vec2) -> bool {
        (100. ..694.).contains(&pointer.x) && (442. ..490.).contains(&pointer.y)
    }

    /// The node after `node` for a train travelling east (`true`) or west
    /// with the given point settings. Callers stop at stations and exits.
    pub(crate) fn next_node(points: [usize; 2], node: usize, eastbound: bool) -> usize {
        match node {
            2 => match points[0] {
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
            5 => match points[1] {
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
                    node.saturating_sub(1)
                }
            }
        }
    }

    /// The signal guarding `node` for a train travelling in that direction.
    pub(crate) fn signal_at(node: usize, eastbound: bool) -> Option<usize> {
        match (node, eastbound) {
            (1, true) => Some(0),
            (3, true) => Some(1),
            (4, false) => Some(2),
            (6, false) => Some(3),
            _ => None,
        }
    }

    /// Point settings that route a service to `destination` from either yard.
    pub(crate) fn desired_points(destination: usize) -> [usize; 2] {
        if destination < 2 {
            [destination + 1, 0]
        } else {
            [0, destination - 1]
        }
    }

    fn note(&mut self, message: impl Into<String>) {
        self.message = message.into();
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
        self.incident = None;
    }

    fn retry(&mut self) -> Cue {
        if self.retries == 0 {
            self.phase = Phase::Lost;
            self.note("Your safety certificate has been withdrawn.");
            return Cue::Bad;
        }
        self.retries -= 1;
        self.reset_shift();
        self.events.push(Event::Retried);
        Cue::Action
    }

    fn incident(&mut self, incident: Incident, message: String) -> Cue {
        self.incidents = self.incidents.saturating_add(1);
        self.phase = if self.retries == 0 {
            Phase::Lost
        } else {
            Phase::Incident
        };
        self.dispatching = false;
        self.incident = Some(incident);
        self.events.push(Event::Incident(incident));
        self.note(message);
        Cue::Bad
    }

    pub(crate) fn points_locked(&self, point: usize) -> bool {
        let at = self.junction(point);
        (0..self.schedule().trains.len()).any(|i| {
            self.position(i)
                .is_some_and(|p| p.distance_squared(at) < 38. * 38.)
        })
    }

    fn operate(&mut self, control: usize, backwards: bool) -> Cue {
        self.selected = control;
        if let Some(point) = CONTROL_POINTS.get(control).copied().flatten() {
            if self.points_locked(point) {
                self.note("Points locked: wait until the nearby train is clear.");
                self.events.push(Event::Locked { point });
                return Cue::None;
            }
            self.points[point] = (self.points[point] + if backwards { 2 } else { 1 }) % 3;
            self.note(format!(
                "{} → {}",
                CONTROL_NAMES[control], ROUTE_NAMES[self.points[point]]
            ));
            self.events.push(Event::Points {
                point,
                route: self.points[point],
            });
        } else {
            let Some(signal) = CONTROL_SIGNAL.get(control).copied().flatten() else {
                return Cue::None;
            };
            if matches!(signal, 1 | 2) && self.bridge_closed() && !self.signals[signal] {
                self.note("Bridge inspection: both entrances remain red.");
                self.events.push(Event::Closed { signal });
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
            self.events.push(Event::Signal {
                signal,
                green: self.signals[signal],
            });
        }
        Cue::Action
    }

    /// Advance one 1/60-second simulation tick and return accepted-event feedback.
    /// Terminal careers are immutable; planning stops railway time but permits
    /// point and signal changes. Input edges are consumed by the shared host.
    /// Clicking a lamp or a set of points on the map operates it exactly like
    /// its switchboard control.
    pub fn tick(&mut self, input: &Input) -> Cue {
        self.events.clear();
        if matches!(self.phase, Phase::Won | Phase::Lost) {
            return Cue::None;
        }
        match self.phase {
            Phase::Briefing => {
                if input.primary || (input.click && Self::main_button(input.pointer)) {
                    self.phase = Phase::Working;
                    self.dispatching = true;
                    self.note("Set points, then clear a signal. X freezes time for planning.");
                    self.events.push(Event::Started);
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
        if input.secondary || (input.click && Self::clock_button(input.pointer)) {
            self.dispatching = !self.dispatching;
            self.events.push(Event::Clock(self.dispatching));
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
        if input.click
            && let Some(control) =
                Self::control_at(input.pointer).or_else(|| self.map_control_at(input.pointer))
        {
            cue = self.operate(control, false);
        }
        if self.dispatching {
            let event = self.advance_railway();
            if event != Cue::None {
                cue = event;
            }
        }
        cue
    }

    fn advance_railway(&mut self) -> Cue {
        self.time += 1;
        let schedule = self.schedule();
        if let Some((start, end)) = schedule.closure {
            if self.time == start * TICKS_PER_SECOND {
                self.events.push(Event::Bridge { open: false });
            } else if self.time == end * TICKS_PER_SECOND {
                self.events.push(Event::Bridge { open: true });
            }
        }
        if self.bridge_closed() {
            self.signals[1] = false;
            self.signals[2] = false;
        }
        let mut cue = Cue::None;
        for i in 0..schedule.trains.len() {
            let plan = schedule.trains[i];
            if self.time == plan.due * TICKS_PER_SECOND + 1
                && self.trains[i].state != TrainState::Delivered
            {
                self.events.push(Event::Late { train: i });
            }
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
                    self.events.push(Event::Entered { train: i });
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
                            return self.incident(
                                Incident::WrongPlatform {
                                    train: i,
                                    station: node - 8,
                                },
                                format!(
                                    "{} reached {} instead of {}. Check both points before release.",
                                    plan.name,
                                    STATION_NAMES[node - 8],
                                    STATION_NAMES[plan.destination]
                                ),
                            );
                        }
                        let late = self.time.saturating_sub(plan.due * 60);
                        let earned = 200 + 180_u32.saturating_sub(late / 12);
                        self.delays += late;
                        self.provisional += earned;
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
                        self.events.push(Event::Delivered {
                            train: i,
                            late,
                            earned,
                        });
                        cue = Cue::Good;
                        continue;
                    }
                    let eastbound = plan.side == Side::West;
                    if (node == 0 && !eastbound) || (node == 7 && eastbound) {
                        return self.incident(
                            Incident::LeftLine { train: i, node },
                            format!(
                                "{} left the railway without reaching {}. Route it north or south at its station.",
                                plan.name, STATION_NAMES[plan.destination]
                            ),
                        );
                    }
                    if let Some(signal) = Self::signal_at(node, eastbound) {
                        if !self.signals[signal] {
                            continue;
                        }
                        self.signals[signal] = false;
                        self.events.push(Event::Passed { signal });
                    }
                    let next = Self::next_node(self.points, node, eastbound);
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
                    if let Some(pb) = self
                        .position(b)
                        .filter(|pb| pa.distance_squared(*pb) < 20. * 20.)
                    {
                        return self.incident(
                            Incident::Collision {
                                a,
                                b,
                                at: pa.lerp(pb, 0.5),
                            },
                            format!(
                                "{} and {} collided. Keep opposing or following trains separated; signals do not check the route for you.",
                                schedule.trains[a].name, schedule.trains[b].name
                            ),
                        );
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
            self.events.push(Event::Cleared { medal });
            Cue::Win
        } else if self.time >= schedule.closing * 60 {
            let waiting = schedule.trains.len() - self.delivered();
            self.incident(
                Incident::Bell { waiting },
                "The end-of-shift bell rang with trains still undelivered. Plan routes first, then keep the railway moving.".to_owned(),
            )
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
        let desired = Self::desired_points(plan.destination);
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
        match self.stars() {
            24 => "Master of the Line",
            20..=23 => "Chief Dispatcher",
            15..=19 => "Senior Signalkeeper",
            _ => "Keeper of the Brass Key",
        }
    }

    /// Punctuality stars banked across the career (up to 24).
    pub(crate) fn stars(&self) -> u32 {
        self.medals.iter().map(|m| u32::from(*m)).sum()
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
    fn click_at(game: &mut Game, pointer: Vec2) -> Cue {
        game.tick(&Input {
            click: true,
            pointer,
            ..Input::default()
        })
    }
    fn click_lamp(game: &mut Game, signal: usize) -> Cue {
        let at = game.lamp(signal);
        click_at(game, at)
    }
    fn click_points(game: &mut Game, point: usize) -> Cue {
        let at = game.junction(point);
        click_at(game, at)
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
        assert_eq!(
            game.incident,
            Some(Incident::WrongPlatform {
                train: 0,
                station: 1
            })
        );
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
        let Some(Incident::Collision { a, b, at }) = game.incident else {
            panic!("expected a collision, got {:?}", game.incident);
        };
        assert_eq!((a, b), (0, 1));
        assert!(
            (355. ..=444.).contains(&at.x),
            "collision on the bridge: {at:?}"
        );
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
        assert!(game.events.iter().any(|e| *e == Event::Locked { point: 0 }));
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
                assert_eq!(game.incident, None);
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
        assert_eq!(game.incident, Some(Incident::Bell { waiting: 4 }));
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
        assert!(
            game.events
                .iter()
                .any(|e| *e == Event::Closed { signal: 1 })
        );
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

    #[test]
    fn map_lamps_and_points_operate_exactly_like_their_switchboard_controls() {
        let mut game = Game::new();
        press_primary(&mut game);
        // Points on the map cycle forward like a switchboard click.
        assert_eq!(click_points(&mut game, 0), Cue::Action);
        assert_eq!(game.points, [1, 0]);
        assert_eq!(game.selected, 1);
        let near = game.junction(1) + vec2(6., -5.);
        assert_eq!(click_at(&mut game, near), Cue::Action);
        assert_eq!(game.points, [1, 1]);
        assert_eq!(game.selected, 4);
        // Each lamp toggles its own signal and selects its control.
        for (signal, control) in SIGNAL_CONTROL.into_iter().enumerate() {
            assert_eq!(click_lamp(&mut game, signal), Cue::Action);
            assert!(game.signals[signal], "signal {signal}");
            assert_eq!(game.selected, control);
            let green = Event::Signal {
                signal,
                green: true,
            };
            assert!(game.events.iter().any(|e| *e == green));
            click_lamp(&mut game, signal);
            assert!(!game.signals[signal]);
        }
        // Empty map, stations and the river do nothing.
        let before = (game.points, game.signals, game.selected);
        for pointer in [vec2(400., 200.), game.nodes()[8], vec2(600., 470.)] {
            assert_eq!(click_at(&mut game, pointer), Cue::None);
            assert_eq!((game.points, game.signals, game.selected), before);
        }
        // Map clicks share the same refusals: locked points stay put.
        let mut game = Game::new();
        press_primary(&mut game);
        click_points(&mut game, 0);
        click_lamp(&mut game, 0);
        for _ in 0..500 {
            if game.points_locked(0) {
                break;
            }
            game.tick(&Input::default());
        }
        assert!(game.points_locked(0));
        assert_eq!(click_points(&mut game, 0), Cue::None);
        assert_eq!(game.points[0], 1);
        // The bot still plays only through the switchboard.
        for tick in 0..2000 {
            let input = game.bot_input(tick);
            if input.click {
                assert!(Game::control_at(input.pointer).is_some());
            }
            game.tick(&input);
        }
    }

    #[test]
    fn events_record_accepted_actions_and_reset_every_tick() {
        let mut game = Game::new();
        press_primary(&mut game);
        assert_eq!(
            game.events.iter().copied().collect::<Vec<_>>(),
            [Event::Started]
        );
        // The next tick starts a fresh list: W1 enters, nothing else happened.
        game.tick(&Input::default());
        assert_eq!(
            game.events.iter().copied().collect::<Vec<_>>(),
            [Event::Entered { train: 0 }]
        );
        game.tick(&Input::default());
        assert_eq!(game.events.iter().count(), 0);
        click(&mut game, 1);
        assert!(
            game.events
                .iter()
                .any(|e| *e == Event::Points { point: 0, route: 1 })
        );
        game.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        assert!(game.events.iter().any(|e| *e == Event::Clock(false)));
        game.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        // Follow the first delivery and its events through ordinary play.
        let mut delivered = None;
        for tick in 0..4000 {
            game.tick(&game.bot_input(tick));
            if let Some(Event::Delivered {
                train,
                late,
                earned,
            }) = game
                .events
                .iter()
                .copied()
                .find(|e| matches!(e, Event::Delivered { .. }))
            {
                delivered = Some((train, late, earned));
                break;
            }
        }
        let (train, late, earned) = delivered.expect("the bot delivers W1");
        assert_eq!((train, late), (0, 0));
        assert_eq!(earned, game.provisional);
    }

    #[test]
    fn route_helpers_match_the_rules() {
        // West-origin services to A/B turn at the west points.
        assert_eq!(Game::desired_points(0), [1, 0]);
        assert_eq!(Game::desired_points(3), [0, 2]);
        assert_eq!(Game::next_node([1, 0], 2, true), 8);
        assert_eq!(Game::next_node([0, 2], 5, true), 11);
        assert_eq!(Game::next_node([0, 0], 5, false), 4);
        assert_eq!(Game::next_node([0, 0], 0, false), 0);
        assert_eq!(Game::signal_at(1, true), Some(0));
        assert_eq!(Game::signal_at(1, false), None);
        assert_eq!(Game::signal_at(4, false), Some(2));
    }
}
