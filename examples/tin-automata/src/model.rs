//! Pure programming rules for twelve factory workbenches.
//!
//! ```
//! use tin_automata::{Game, Phase};
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! game.step(&Input { primary: true, ..Input::default() }); // append Forward
//! assert_eq!(game.program_len(), 1);
//! game.step(&Input { slot: Some(2), ..Input::default() }); // execute one instruction
//! assert_eq!(game.phase(), Phase::Paused);
//! assert_eq!(game.executed_steps(), 1);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{ROOMS, Room};
use crate::layout::{self, Button};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_play::navigation::Cell;

pub(crate) const COLS: u32 = 9;
pub(crate) const ROWS: u32 = 7;
pub(crate) const CAPACITY: usize = 32;
const HISTORY: usize = 32;
const STEP_LIMIT: u16 = 160;
pub(crate) const CADENCE: u16 = 18;
/// Presentation events one update can record.
const EVENTS: usize = 8;
/// Visited cells remembered for the robot's trail.
const PATH: usize = 48;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Op {
    #[default]
    Forward,
    Left,
    Right,
    Grab,
    Drop,
    Repeat2,
    Repeat3,
}
impl Op {
    pub(crate) const ALL: [Self; 7] = [
        Self::Forward,
        Self::Left,
        Self::Right,
        Self::Grab,
        Self::Drop,
        Self::Repeat2,
        Self::Repeat3,
    ];
    /// One-word palette caption; the repeat count is drawn in its glyph.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Forward => "Forward",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Grab => "Grab",
            Self::Drop => "Drop",
            Self::Repeat2 | Self::Repeat3 => "Repeat",
        }
    }
    pub(crate) fn code(self) -> char {
        match self {
            Self::Forward => 'F',
            Self::Left => 'L',
            Self::Right => 'R',
            Self::Grab => 'G',
            Self::Drop => 'D',
            Self::Repeat2 => '2',
            Self::Repeat3 => '3',
        }
    }
    fn from_code(code: u8) -> Self {
        match code {
            b'F' => Self::Forward,
            b'L' => Self::Left,
            b'R' => Self::Right,
            b'G' => Self::Grab,
            b'D' => Self::Drop,
            b'2' => Self::Repeat2,
            b'3' => Self::Repeat3,
            _ => panic!("authored instruction"),
        }
    }
}

/// Why an attempt stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FaultKind {
    Edge,
    Wall,
    Pit,
    GateA,
    GateB,
    ArmsFull,
    NoParcel,
    EmptyArms,
    Occupied,
    WrongDock,
    StepLimit,
    TapeEnded,
    NoPrevious,
}
impl FaultKind {
    /// Status-reader explanation.
    fn message(self) -> &'static str {
        match self {
            Self::Edge => "The robot reached the workbench edge. C edits the tape.",
            Self::Wall => "A wall blocks Forward. C returns to the editor.",
            Self::Pit => "The route enters a pit. C restores the workbench.",
            Self::GateA => "Gate A is closed. Step on switch a first.",
            Self::GateB => "Gate B is closed. Step on switch b first.",
            Self::ArmsFull => "Arms already full. Drop the carried parcel first.",
            Self::NoParcel => "There is no parcel on this tile. Move onto one before Grab.",
            Self::EmptyArms => "Empty arms cannot Drop. Grab a parcel first.",
            Self::Occupied => "Another parcel occupies this tile. Drop elsewhere.",
            Self::WrongDock => "Wrong numbered dock. Carry the parcel to its matching number.",
            Self::StepLimit => "Execution budget exhausted. C edits a shorter program.",
            Self::TapeEnded => "Tape ended before every delivery. C returns to the editor.",
            Self::NoPrevious => "A repeat needs an earlier simple instruction.",
        }
    }
    /// One- or two-word popup shown where the fault happened.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Edge => "Edge",
            Self::Wall => "Wall",
            Self::Pit => "Pit",
            Self::GateA => "Gate A",
            Self::GateB => "Gate B",
            Self::ArmsFull => "Arms full",
            Self::NoParcel => "No parcel",
            Self::EmptyArms => "Empty arms",
            Self::Occupied => "Occupied",
            Self::WrongDock => "Wrong dock",
            Self::StepLimit => "Too long",
            Self::TapeEnded => "Tape end",
            Self::NoPrevious => "Repeat what?",
        }
    }
    /// The robot tried to move into the fault cell.
    pub(crate) fn blocked_move(self) -> bool {
        matches!(
            self,
            Self::Edge | Self::Wall | Self::Pit | Self::GateA | Self::GateB
        )
    }
}

/// Where and why the current attempt faulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fault {
    pub kind: FaultKind,
    /// The blocked target for moves, otherwise the robot's tile.
    pub at: Cell,
    /// The tape cell that was executing (the append cell when the tape ran out).
    pub pc: Option<usize>,
}

/// Something an update accepted or refused, kept for that update only so the
/// presentation can react where it happened. Rules never read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Wrote {
        index: usize,
        op: Op,
    },
    Deleted {
        index: usize,
    },
    Undid,
    Cleared {
        len: usize,
    },
    Cursor {
        index: usize,
    },
    Palette {
        index: usize,
    },
    /// A write was refused because the tape budget is full.
    TapeFull {
        index: usize,
    },
    /// A locked instruction tile was clicked.
    Locked {
        index: usize,
    },
    /// Running was refused because the tape is empty.
    EmptyRun,
    Started,
    Paused {
        paused: bool,
    },
    Moved {
        from: Cell,
        to: Cell,
    },
    Turned {
        at: Cell,
    },
    Repeat {
        index: usize,
        times: u8,
    },
    Switch {
        at: Cell,
        gate: u8,
    },
    Grabbed {
        at: Cell,
        parcel: usize,
    },
    Dropped {
        at: Cell,
        parcel: usize,
        delivered: bool,
    },
    Fault(Fault),
    Solved {
        stars: u8,
    },
    Stopped,
    NextRoom,
    Hint {
        open: bool,
    },
}

/// Presentation memory: this update's events, the motion of the latest
/// executed instruction, the attempt's trail and its fault. Rules never read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Look {
    events: [Option<Event>; EVENTS],
    /// Where the robot stood, and faced, before the latest executed instruction.
    pub from: Cell,
    pub from_facing: u8,
    /// Updates since that instruction (saturating).
    pub age: u16,
    pub fault: Option<Fault>,
    path: [Cell; PATH],
    path_len: usize,
}
impl Look {
    fn new() -> Self {
        let origin = Cell { x: 0, y: 0 };
        Self {
            events: [None; EVENTS],
            from: origin,
            from_facing: 1,
            age: u16::MAX,
            fault: None,
            path: [origin; PATH],
            path_len: 0,
        }
    }
    fn tick(&mut self) {
        self.events = [None; EVENTS];
        self.age = self.age.saturating_add(1);
    }
    fn push(&mut self, event: Event) {
        if let Some(slot) = self.events.iter_mut().find(|e| e.is_none()) {
            *slot = Some(event);
        }
    }
    /// Events accepted or refused by the latest update.
    pub(crate) fn events(&self) -> impl Iterator<Item = Event> + '_ {
        self.events.iter().flatten().copied()
    }
    fn restart(&mut self, at: Cell, facing: u8) {
        self.from = at;
        self.from_facing = facing;
        self.age = u16::MAX;
        self.fault = None;
        self.path[0] = at;
        self.path_len = 1;
    }
    fn visit(&mut self, cell: Cell) {
        if self.path_len == PATH {
            self.path.rotate_left(1);
            self.path_len -= 1;
        }
        self.path[self.path_len] = cell;
        self.path_len += 1;
    }
    /// Cells visited by this attempt, oldest first, ending at the robot.
    pub(crate) fn path(&self) -> &[Cell] {
        &self.path[..self.path_len]
    }
}

/// Local programming phases; the shared host owns whole-campaign pause and retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Editing,
    Running,
    Paused,
    Fault,
    Solved,
    Complete,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tape {
    pub ops: [Op; CAPACITY],
    pub len: usize,
    pub cursor: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Parcel {
    pub pos: Cell,
    pub present: bool,
    pub delivered: bool,
}
impl Default for Parcel {
    fn default() -> Self {
        Self {
            pos: Cell { x: 0, y: 0 },
            present: false,
            delivered: false,
        }
    }
}

/// Points banked once for a solved room.
pub(crate) fn points(medal: u8, steps: u16) -> u32 {
    u32::from(medal) * 500 + 500 + 100_u32.saturating_sub(u32::from(steps))
}

/// A complete factory restoration. All changes pass through [`Game::step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) level: usize,
    pub(crate) phase: Phase,
    pub(crate) tape: Tape,
    pub(crate) palette: usize,
    history: [Tape; HISTORY],
    history_len: usize,
    pub(crate) robot: Cell,
    pub(crate) facing: u8,
    pub(crate) parcels: [Parcel; 3],
    pub(crate) carrying: Option<usize>,
    pub(crate) switches: u8,
    pub(crate) pc: usize,
    pub(crate) last_pc: Option<usize>,
    previous: Option<Op>,
    pub(crate) repeats: u8,
    pub(crate) steps: u16,
    pub(crate) timer: u16,
    pub(crate) attempts: u16,
    pub(crate) failures: u16,
    pub(crate) hinted: bool,
    pub(crate) show_hint: bool,
    pub(crate) stars: [u8; 12],
    score: u32,
    /// Last accepted action in words, published to status readers only.
    pub(crate) notice: &'static str,
    pub(crate) solved_ticks: u16,
    pub(crate) look: Look,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin the factory campaign with an empty tape and all rooms locked.
    pub fn new() -> Self {
        let mut game = Self {
            level: 0,
            phase: Phase::Editing,
            tape: Tape::default(),
            palette: 0,
            history: [Tape::default(); HISTORY],
            history_len: 0,
            robot: Cell { x: 0, y: 0 },
            facing: 1,
            parcels: [Parcel::default(); 3],
            carrying: None,
            switches: 0,
            pc: 0,
            last_pc: None,
            previous: None,
            repeats: 0,
            steps: 0,
            timer: 0,
            attempts: 0,
            failures: 0,
            hinted: false,
            show_hint: false,
            stars: [0; 12],
            score: 0,
            notice: "Choose an instruction, then Space to write it.",
            solved_ticks: 0,
            look: Look::new(),
        };
        game.reset_board();
        game
    }
    /// Current room number, including the completed final room.
    pub fn room(&self) -> usize {
        self.level + 1
    }
    /// Current editing/execution state.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// Occupied instruction cells, never larger than the room's budget.
    pub fn program_len(&self) -> usize {
        self.tape.len
    }
    /// Instructions and expanded repeat steps executed in the present attempt.
    pub fn executed_steps(&self) -> u16 {
        self.steps
    }
    /// Banked score from completed workbenches.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Puzzle retries are unlimited; finishing all rooms is the terminal outcome.
    pub fn outcome(&self) -> Outcome {
        if self.phase == Phase::Complete {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub(crate) fn content(&self) -> &'static Room {
        &ROOMS[self.level]
    }
    /// Three-star program length.
    pub(crate) fn par(&self) -> usize {
        self.content().solution.len()
    }
    pub(crate) fn can_undo(&self) -> bool {
        self.history_len > 0
    }
    /// Stars a solve would earn now with a tape of `len` instructions.
    pub(crate) fn medal(&self, len: usize) -> u8 {
        let par = self.par();
        let medal = if len <= par && self.failures == 0 && !self.hinted {
            3
        } else if len <= par + 4 && self.failures <= 2 {
            2
        } else {
            1
        };
        if self.hinted { medal.min(2) } else { medal }
    }
    pub(crate) fn unlocked(&self) -> usize {
        if self.level < 4 {
            5
        } else if self.level == 4 {
            6
        } else {
            7
        }
    }
    pub(crate) fn tile(&self, cell: Cell) -> u8 {
        if cell.x >= COLS || cell.y >= ROWS {
            b'#'
        } else {
            self.content().map[cell.y as usize].as_bytes()[cell.x as usize]
        }
    }
    pub(crate) fn goals(&self) -> usize {
        self.parcels.iter().filter(|p| p.present).count()
    }
    pub(crate) fn delivered(&self) -> usize {
        self.parcels.iter().filter(|p| p.delivered).count()
    }
    fn reset_board(&mut self) {
        self.robot = Cell {
            x: self.content().start.0,
            y: self.content().start.1,
        };
        self.facing = self.content().facing;
        self.parcels = [Parcel::default(); 3];
        for y in 0..ROWS {
            for x in 0..COLS {
                let c = self.tile(Cell { x, y });
                if (b'1'..=b'3').contains(&c) {
                    self.parcels[(c - b'1') as usize] = Parcel {
                        pos: Cell { x, y },
                        present: true,
                        delivered: false,
                    };
                }
            }
        }
        self.carrying = None;
        self.switches = 0;
        self.pc = 0;
        self.last_pc = None;
        self.previous = None;
        self.repeats = 0;
        self.steps = 0;
        self.timer = 0;
        self.look.restart(self.robot, self.facing);
    }
    fn remember(&mut self) {
        if self.history_len == HISTORY {
            self.history.rotate_left(1);
            self.history_len -= 1;
        }
        self.history[self.history_len] = self.tape;
        self.history_len += 1;
    }
    fn write(&mut self) -> Cue {
        let append = self.tape.cursor == self.tape.len;
        if append && self.tape.len == self.content().budget {
            self.notice = "Tape budget reached. Replace a cell, delete, or compress with repeats.";
            self.look.push(Event::TapeFull {
                index: self.tape.len.saturating_sub(1),
            });
            return Cue::Bad;
        }
        let op = Op::ALL[self.palette];
        if !append && self.tape.ops[self.tape.cursor] == op {
            return Cue::None;
        }
        self.remember();
        let index = self.tape.cursor;
        self.tape.ops[index] = op;
        if append {
            self.tape.len += 1;
        }
        self.tape.cursor = (index + 1).min(self.tape.len);
        self.notice = "Instruction written. 1 runs; 2 executes one step.";
        self.look.push(Event::Wrote { index, op });
        Cue::Action
    }
    fn delete(&mut self) -> Cue {
        if self.tape.len == 0 {
            return Cue::None;
        }
        self.remember();
        let at = self.tape.cursor.min(self.tape.len - 1);
        self.tape.ops.copy_within(at + 1..self.tape.len, at);
        self.tape.len -= 1;
        self.tape.ops[self.tape.len] = Op::Forward;
        self.tape.cursor = at;
        self.notice = "Instruction removed. 3 undoes an edit.";
        self.look.push(Event::Deleted { index: at });
        Cue::Action
    }
    fn begin(&mut self, running: bool) -> Cue {
        if self.tape.len == 0 {
            self.notice = "Write at least one instruction before running.";
            self.look.push(Event::EmptyRun);
            return Cue::Bad;
        }
        self.reset_board();
        self.attempts = self.attempts.saturating_add(1);
        self.show_hint = false;
        self.phase = if running {
            Phase::Running
        } else {
            Phase::Paused
        };
        self.notice = "Follow the bright instruction marker. C returns to the editor.";
        self.look.push(Event::Started);
        if running { Cue::Action } else { self.execute() }
    }
    fn fault(&mut self, kind: FaultKind, at: Cell, pc: Option<usize>) -> Cue {
        self.phase = Phase::Fault;
        self.failures = self.failures.saturating_add(1);
        self.notice = kind.message();
        let fault = Fault { kind, at, pc };
        self.look.fault = Some(fault);
        self.look.push(Event::Fault(fault));
        Cue::Bad
    }
    fn simple(&mut self, op: Op) -> Result<(), (FaultKind, Cell)> {
        let here = self.robot;
        match op {
            Op::Forward => {
                let (dx, dy) = [(0, -1), (1, 0), (0, 1), (-1, 0)][self.facing as usize];
                let x = self.robot.x as i32 + dx;
                let y = self.robot.y as i32 + dy;
                if x < 0 || y < 0 || x >= COLS as i32 || y >= ROWS as i32 {
                    return Err((FaultKind::Edge, here));
                }
                let next = Cell {
                    x: x as u32,
                    y: y as u32,
                };
                match self.tile(next) {
                    b'#' => return Err((FaultKind::Wall, next)),
                    b'~' => return Err((FaultKind::Pit, next)),
                    b'A' if self.switches & 1 == 0 => return Err((FaultKind::GateA, next)),
                    b'B' if self.switches & 2 == 0 => return Err((FaultKind::GateB, next)),
                    switch @ (b'a' | b'b') => {
                        let bit = 1 << (switch - b'a');
                        if self.switches & bit == 0 {
                            self.look.push(Event::Switch {
                                at: next,
                                gate: switch.to_ascii_uppercase(),
                            });
                        }
                        self.switches |= bit;
                    }
                    _ => {}
                }
                self.robot = next;
                self.look.visit(next);
                self.look.push(Event::Moved {
                    from: here,
                    to: next,
                });
            }
            Op::Left => {
                self.facing = (self.facing + 3) % 4;
                self.look.push(Event::Turned { at: here });
            }
            Op::Right => {
                self.facing = (self.facing + 1) % 4;
                self.look.push(Event::Turned { at: here });
            }
            Op::Grab => {
                if self.carrying.is_some() {
                    return Err((FaultKind::ArmsFull, here));
                }
                self.carrying = self
                    .parcels
                    .iter()
                    .position(|p| p.present && !p.delivered && p.pos == self.robot);
                let Some(parcel) = self.carrying else {
                    return Err((FaultKind::NoParcel, here));
                };
                self.look.push(Event::Grabbed { at: here, parcel });
            }
            Op::Drop => {
                let Some(i) = self.carrying else {
                    return Err((FaultKind::EmptyArms, here));
                };
                if self
                    .parcels
                    .iter()
                    .enumerate()
                    .any(|(j, p)| j != i && p.present && !p.delivered && p.pos == self.robot)
                {
                    return Err((FaultKind::Occupied, here));
                }
                let tile = self.tile(self.robot);
                if (b'x'..=b'z').contains(&tile) && (tile - b'x') as usize != i {
                    return Err((FaultKind::WrongDock, here));
                }
                self.parcels[i].pos = self.robot;
                self.parcels[i].delivered = tile == b'x' + i as u8;
                self.carrying = None;
                self.look.push(Event::Dropped {
                    at: here,
                    parcel: i,
                    delivered: self.parcels[i].delivered,
                });
            }
            Op::Repeat2 | Op::Repeat3 => unreachable!("repeat is control flow"),
        }
        Ok(())
    }
    fn execute(&mut self) -> Cue {
        if self.steps >= STEP_LIMIT {
            return self.fault(FaultKind::StepLimit, self.robot, self.last_pc);
        }
        if self.pc >= self.tape.len && self.repeats == 0 {
            return self.fault(FaultKind::TapeEnded, self.robot, Some(self.tape.len));
        }
        self.steps += 1;
        self.look.from = self.robot;
        self.look.from_facing = self.facing;
        self.look.age = 0;
        let op = if self.repeats > 0 {
            self.repeats -= 1;
            self.previous.expect("repeat has previous action")
        } else {
            self.last_pc = Some(self.pc);
            let op = self.tape.ops[self.pc];
            self.pc += 1;
            op
        };
        if matches!(op, Op::Repeat2 | Op::Repeat3) {
            if self.previous.is_none() {
                return self.fault(FaultKind::NoPrevious, self.robot, self.last_pc);
            }
            self.repeats = if op == Op::Repeat2 { 2 } else { 3 };
            self.notice = "Repeat expands into separate, visible robot steps.";
            self.look.push(Event::Repeat {
                index: self.last_pc.unwrap_or_default(),
                times: self.repeats,
            });
            return Cue::Action;
        }
        if let Err((kind, at)) = self.simple(op) {
            return self.fault(kind, at, self.last_pc);
        }
        self.previous = Some(op);
        self.notice = match op {
            Op::Forward => "Forward accepted.",
            Op::Left | Op::Right => "Facing changed; position is unchanged.",
            Op::Grab => "Parcel secured. Find its numbered delivery dock.",
            Op::Drop => "Parcel placed. Matching docks count as deliveries.",
            _ => unreachable!(),
        };
        if self.delivered() == self.goals() {
            let medal = self.medal(self.tape.len);
            self.stars[self.level] = medal;
            self.score += points(medal, self.steps);
            self.solved_ticks = 0;
            self.phase = if self.level == 11 {
                Phase::Complete
            } else {
                Phase::Solved
            };
            self.notice = "All parcels delivered. The next workbench is ready.";
            self.look.push(Event::Solved { stars: medal });
            return if self.phase == Phase::Complete {
                Cue::Win
            } else {
                Cue::Good
            };
        }
        Cue::Action
    }
    fn stop(&mut self) -> Cue {
        if self.phase == Phase::Editing {
            return Cue::None;
        }
        self.reset_board();
        self.phase = Phase::Editing;
        self.notice = "Workbench restored. Your program is kept for editing.";
        self.look.push(Event::Stopped);
        Cue::Action
    }
    fn next(&mut self) -> Cue {
        self.level += 1;
        self.phase = Phase::Editing;
        self.tape = Tape::default();
        self.palette = 0;
        self.history_len = 0;
        self.attempts = 0;
        self.failures = 0;
        self.hinted = false;
        self.show_hint = false;
        self.solved_ticks = 0;
        self.reset_board();
        self.notice = "A new workbench. Write, test and refine your route.";
        self.look.push(Event::NextRoom);
        Cue::Good
    }
    /// Cells the room's reference program visits from the restored board.
    /// The hint draws this route; it simulates a copy and changes nothing.
    pub(crate) fn reference_route(&self) -> Self {
        let mut sim = self.clone();
        sim.reset_board();
        sim.tape = Tape::default();
        for code in self.content().solution.bytes() {
            sim.tape.ops[sim.tape.len] = Op::from_code(code);
            sim.tape.len += 1;
        }
        sim.phase = Phase::Running;
        while sim.phase == Phase::Running {
            sim.execute();
        }
        sim
    }
    /// Apply exactly one fixed 60 Hz tick. All controls are ordinary player actions.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.phase == Phase::Complete {
            return Cue::None;
        }
        self.look.tick();
        if self.phase == Phase::Solved {
            self.solved_ticks = self.solved_ticks.saturating_add(1);
            return if input.primary || (input.click && layout::NEXT.contains(input.pointer)) {
                self.next()
            } else {
                Cue::None
            };
        }
        // A click on a drawn button acts exactly like its key.
        let button = if input.click {
            Button::at(input.pointer)
        } else {
            None
        };
        if input.tertiary || button == Some(Button::Stop) {
            return self.stop();
        }
        let slot = button.and_then(Button::slot).or(input.slot);
        if slot == Some(5) {
            self.hinted = true;
            self.show_hint = !self.show_hint;
            if self.phase == Phase::Running {
                self.phase = Phase::Paused;
            }
            self.look.push(Event::Hint {
                open: self.show_hint,
            });
            return Cue::Action;
        }
        match self.phase {
            Phase::Fault => {
                return if input.primary {
                    self.stop()
                } else {
                    Cue::None
                };
            }
            Phase::Running | Phase::Paused => {
                if slot == Some(1) {
                    self.phase = if self.phase == Phase::Running {
                        Phase::Paused
                    } else {
                        Phase::Running
                    };
                    self.timer = 0;
                    self.look.push(Event::Paused {
                        paused: self.phase == Phase::Paused,
                    });
                    return Cue::Action;
                }
                if slot == Some(2) {
                    self.phase = Phase::Paused;
                    self.show_hint = false;
                    return self.execute();
                }
                if self.phase == Phase::Running {
                    self.timer += 1;
                    if self.timer >= CADENCE {
                        self.timer = 0;
                        return self.execute();
                    }
                }
                return Cue::None;
            }
            _ => {}
        }
        match slot {
            Some(1) => return self.begin(true),
            Some(2) => return self.begin(false),
            Some(3) if self.history_len > 0 => {
                self.history_len -= 1;
                self.tape = self.history[self.history_len];
                self.notice = "Previous tape edit restored.";
                self.look.push(Event::Undid);
                return Cue::Action;
            }
            Some(4) if self.tape.len > 0 => {
                let len = self.tape.len;
                self.remember();
                self.tape = Tape::default();
                self.notice = "Tape cleared. Undo can recover it.";
                self.look.push(Event::Cleared { len });
                return Cue::Action;
            }
            _ => {}
        }
        if input.click {
            let p = input.pointer;
            if let Some(index) = layout::tape_at(p)
                && index <= self.tape.len
            {
                self.tape.cursor = index;
                self.look.push(Event::Cursor { index });
                return Cue::Action;
            }
            // A palette tile chooses its instruction and writes it, as
            // Left/Right followed by Space would.
            if let Some(index) = layout::palette_at(p) {
                if index < self.unlocked() {
                    self.palette = index;
                    return self.write();
                }
                self.look.push(Event::Locked { index });
                return Cue::None;
            }
        }
        if input.left_pressed {
            self.palette = (self.palette + self.unlocked() - 1) % self.unlocked();
            self.look.push(Event::Palette {
                index: self.palette,
            });
            return Cue::Action;
        }
        if input.right_pressed {
            self.palette = (self.palette + 1) % self.unlocked();
            self.look.push(Event::Palette {
                index: self.palette,
            });
            return Cue::Action;
        }
        if input.up_pressed && self.tape.cursor > 0 {
            self.tape.cursor -= 1;
            self.look.push(Event::Cursor {
                index: self.tape.cursor,
            });
            return Cue::Action;
        }
        if input.down_pressed && self.tape.cursor < self.tape.len {
            self.tape.cursor += 1;
            self.look.push(Event::Cursor {
                index: self.tape.cursor,
            });
            return Cue::Action;
        }
        if input.secondary {
            return self.delete();
        }
        if input.primary {
            return self.write();
        }
        Cue::None
    }
    /// Read-only demonstration controller. It enters and executes a normal tape.
    pub fn pilot(&self, tick: u64) -> Input {
        if !tick.is_multiple_of(6) {
            return Input::default();
        }
        match self.phase {
            Phase::Editing => {
                let program = self.content().solution.as_bytes();
                if self.tape.len > program.len()
                    || self.tape.ops[..self.tape.len]
                        .iter()
                        .zip(program)
                        .any(|(op, b)| op.code() as u8 != *b)
                {
                    return Input {
                        slot: Some(4),
                        ..Input::default()
                    };
                }
                if self.tape.len == program.len() {
                    return Input {
                        slot: Some(1),
                        ..Input::default()
                    };
                }
                if self.tape.cursor != self.tape.len {
                    return Input {
                        down_pressed: true,
                        ..Input::default()
                    };
                }
                let desired = Op::from_code(program[self.tape.len]);
                if Op::ALL[self.palette] != desired {
                    Input {
                        right_pressed: true,
                        ..Input::default()
                    }
                } else {
                    Input {
                        primary: true,
                        ..Input::default()
                    }
                }
            }
            Phase::Paused => Input {
                slot: Some(1),
                ..Input::default()
            },
            Phase::Fault => Input {
                tertiary: true,
                ..Input::default()
            },
            Phase::Solved if self.solved_ticks >= 60 => Input {
                primary: true,
                ..Input::default()
            },
            _ => Input::default(),
        }
    }
    /// Stars earned across all rooms.
    pub(crate) fn total_stars(&self) -> u32 {
        self.stars.iter().map(|&s| u32::from(s)).sum()
    }
    /// Rooms restored so far.
    pub(crate) fn restored(&self) -> usize {
        self.stars.iter().filter(|&&s| s > 0).count()
    }
    /// One-sentence campaign report for the ending and the headless CLI.
    pub fn summary(&self) -> String {
        format!(
            "{} of 12 workbenches restored with {} of 36 stars and {} points.",
            self.restored(),
            self.total_stars(),
            self.score
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_arcade::vec2;
    fn button(slot: usize) -> Input {
        Input {
            slot: Some(slot),
            ..Input::default()
        }
    }
    fn press() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    fn write(game: &mut Game, code: u8) {
        let op = Op::from_code(code);
        for _ in 0..7 {
            if Op::ALL[game.palette] == op {
                game.step(&press());
                return;
            }
            game.step(&Input {
                right_pressed: true,
                ..Input::default()
            });
        }
        panic!("locked instruction {op:?}");
    }
    fn enter(game: &mut Game, program: &str) {
        for code in program.bytes() {
            write(game, code);
        }
    }
    fn solve(game: &mut Game) {
        let program = game.content().solution;
        enter(game, program);
        game.step(&button(1));
        for _ in 0..4_000 {
            if !matches!(game.phase, Phase::Running | Phase::Paused) {
                break;
            }
            game.step(&Input::default());
        }
        assert!(
            matches!(game.phase, Phase::Solved | Phase::Complete),
            "room {} {:?} {}",
            game.room(),
            game.phase,
            game.notice
        );
    }
    #[test]
    fn full_factory_is_restored_with_only_normal_inputs_and_exact_replay() {
        let mut game = Game::new();
        let mut replay = Game::new();
        let mut visited = [false; 12];
        let mut finish = None;
        for tick in 0..40_000 {
            visited[game.level] = true;
            let input = game.pilot(tick);
            game.step(&input);
            replay.step(&input);
            assert_eq!(game, replay, "input replay diverged at {tick}");
            assert!(game.tape.len <= game.content().budget && game.tape.len <= CAPACITY);
            assert!(game.steps <= STEP_LIMIT);
            if game.outcome() != Outcome::Playing {
                finish = Some(tick);
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won, "{}", game.summary());
        assert!(visited.into_iter().all(|v| v));
        assert_eq!(game.stars, [3; 12]);
        assert_eq!(game.score(), 24_974);
        assert_eq!(finish, Some(8_988));
        let frozen = game.clone();
        for slot in 1..=5 {
            assert_eq!(
                game.step(&Input {
                    slot: Some(slot),
                    primary: true,
                    secondary: true,
                    tertiary: true,
                    left_pressed: true,
                    click: true,
                    pointer: vec2(550., 540.),
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn every_authored_room_has_distinct_valid_map_and_legal_program() {
        let mut game = Game::new();
        let mut names = std::collections::BTreeSet::new();
        let mut layouts = std::collections::BTreeSet::new();
        let mut total_parcels = 0;
        for room in &ROOMS {
            assert!(names.insert(room.name));
            assert!(layouts.insert(room.map));
            assert_eq!(room.map.len(), ROWS as usize);
            for row in room.map {
                assert_eq!(row.len(), COLS as usize);
                assert!(row.bytes().all(|b| b"#.~ABab123xyz".contains(&b)));
            }
            assert!(room.solution.len() <= room.budget && room.budget <= CAPACITY);
            assert!(game.goals() > 0);
            total_parcels += game.goals();
            for i in 0..game.goals() {
                assert_eq!(
                    room.map
                        .iter()
                        .flat_map(|r| r.bytes())
                        .filter(|&b| b == b'x' + i as u8)
                        .count(),
                    1
                );
            }
            solve(&mut game);
            assert_eq!(game.stars[game.level], 3);
            assert_eq!(game.delivered(), game.goals());
            if game.phase == Phase::Solved {
                game.step(&press());
            }
        }
        assert_eq!(total_parcels, 17);
        assert_eq!(game.outcome(), Outcome::Won);
    }
    #[test]
    fn editor_replaces_deletes_clears_and_undoes_without_touching_world() {
        let mut game = Game::new();
        let start = game.robot;
        enter(&mut game, "FLR");
        assert_eq!(game.tape.len, 3);
        game.step(&Input {
            up_pressed: true,
            ..Input::default()
        });
        write(&mut game, b'G');
        assert_eq!(game.tape.ops[..3], [Op::Forward, Op::Left, Op::Grab]);
        game.step(&button(3));
        assert_eq!(game.tape.ops[..3], [Op::Forward, Op::Left, Op::Right]);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.tape.len, 2);
        game.step(&button(3));
        assert_eq!(game.tape.len, 3);
        game.step(&button(4));
        assert_eq!(game.tape.len, 0);
        game.step(&button(3));
        assert_eq!(game.tape.len, 3);
        assert_eq!(game.robot, start);
        assert_eq!(game.steps, 0);
        assert_eq!(game.score(), 0);
    }
    #[test]
    fn tape_budget_and_history_are_bounded_and_empty_run_cannot_farm() {
        let mut game = Game::new();
        for _ in 0..100 {
            game.step(&button(1));
        }
        assert_eq!(game.phase, Phase::Editing);
        assert_eq!(game.attempts, 0);
        assert_eq!(game.score, 0);
        for _ in 0..40 {
            game.step(&press());
        }
        assert_eq!(game.tape.len, 10);
        assert_eq!(game.tape.cursor, 10);
        let tape = game.tape;
        game.step(&press());
        assert_eq!(game.tape, tape);
        for _ in 0..80 {
            game.step(&Input {
                secondary: true,
                ..Input::default()
            });
            game.step(&press());
        }
        assert_eq!(game.history_len, HISTORY);
        for _ in 0..100 {
            game.step(&button(3));
        }
        assert_eq!(game.history_len, 0);
        assert!(game.tape.len <= 10);
    }
    #[test]
    fn movement_uses_facing_and_turns_never_translate() {
        let mut game = Game::new();
        enter(&mut game, "LFRF");
        game.step(&button(2));
        assert_eq!(game.robot, Cell { x: 1, y: 3 });
        assert_eq!(game.facing, 0);
        game.step(&button(2));
        assert_eq!(game.robot, Cell { x: 1, y: 2 });
        game.step(&button(2));
        assert_eq!(game.facing, 1);
        assert_eq!(game.robot, Cell { x: 1, y: 2 });
        game.step(&button(2));
        assert_eq!(game.robot, Cell { x: 2, y: 2 });
    }
    #[test]
    fn running_cadence_pause_step_and_stop_preserve_tape() {
        let mut game = Game::new();
        enter(&mut game, "FFFF");
        game.step(&button(1));
        for _ in 0..17 {
            game.step(&Input::default());
        }
        assert_eq!(game.steps, 0);
        game.step(&Input::default());
        assert_eq!(game.steps, 1);
        let moved = game.robot;
        game.step(&button(1));
        assert_eq!(game.phase, Phase::Paused);
        for _ in 0..100 {
            game.step(&Input::default());
        }
        assert_eq!(game.robot, moved);
        assert_eq!(game.steps, 1);
        game.step(&button(2));
        assert_eq!(game.steps, 2);
        assert_eq!(game.phase, Phase::Paused);
        let tape = game.tape;
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Editing);
        assert_eq!(game.tape, tape);
        assert_eq!(game.robot, Cell { x: 1, y: 3 });
        assert_eq!(game.steps, 0);
        assert_eq!(game.failures, 0);
    }
    #[test]
    fn holds_and_edit_commands_during_execution_do_not_write_or_move() {
        let mut game = Game::new();
        for _ in 0..60 {
            game.step(&Input {
                right: true,
                down: true,
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!(game.tape.len, 0);
        assert_eq!(game.palette, 0);
        enter(&mut game, "FF");
        game.step(&button(2));
        let snapshot = (game.tape, game.robot, game.palette);
        game.step(&Input {
            primary: true,
            secondary: true,
            left_pressed: true,
            slot: Some(4),
            ..Input::default()
        });
        assert_eq!((game.tape, game.robot, game.palette), snapshot);
    }
    #[test]
    fn faults_are_atomic_count_once_and_retry_restores_world() {
        let mut game = Game::new();
        enter(&mut game, "LFFF");
        for _ in 0..4 {
            game.step(&button(2));
        }
        assert_eq!(game.phase, Phase::Fault);
        assert_eq!(game.robot, Cell { x: 1, y: 1 });
        assert_eq!(game.failures, 1);
        for _ in 0..90 {
            game.step(&button(2));
        }
        assert_eq!(game.failures, 1);
        game.step(&press());
        assert_eq!(game.phase, Phase::Editing);
        assert_eq!(game.robot, Cell { x: 1, y: 3 });
        assert_eq!(game.tape.len, 4);
        assert_eq!(game.failures, 1);
        game.step(&button(2));
        assert_eq!(game.attempts, 2);
    }
    #[test]
    fn grab_drop_and_wrong_docks_respect_capacity_and_identity() {
        let mut game = Game::new();
        enter(&mut game, "FGG");
        game.step(&button(2));
        game.step(&button(2));
        assert_eq!(game.carrying, Some(0));
        game.step(&button(2));
        assert_eq!(game.phase, Phase::Fault);
        assert_eq!(game.carrying, Some(0));
        let mut game = Game::new();
        game.level = 8;
        game.reset_board();
        game.robot = Cell { x: 6, y: 5 };
        game.carrying = Some(0);
        assert!(game.simple(Op::Drop).is_err());
        assert_eq!(game.carrying, Some(0));
        assert_eq!(game.delivered(), 0);
        game.robot = Cell { x: 6, y: 1 };
        assert!(game.simple(Op::Drop).is_ok());
        assert_eq!(game.carrying, None);
        assert!(game.parcels[0].delivered);
        assert!(game.simple(Op::Grab).is_err());
    }
    #[test]
    fn non_dock_drop_can_be_recovered_and_occupied_drop_is_rejected() {
        let mut game = Game::new();
        enter(&mut game, "FGFDG");
        for _ in 0..5 {
            game.step(&button(2));
        }
        assert_eq!(game.carrying, Some(0));
        assert_eq!(game.delivered(), 0);
        assert_eq!(game.parcels[0].pos, Cell { x: 3, y: 3 });
        game.parcels[1] = Parcel {
            pos: game.robot,
            present: true,
            delivered: false,
        };
        assert!(game.simple(Op::Drop).is_err());
        assert_eq!(game.carrying, Some(0));
    }
    #[test]
    fn switches_latch_open_and_stop_resets_gate_state() {
        let mut game = Game::new();
        game.level = 6;
        game.reset_board();
        enter(&mut game, "FGFF");
        for _ in 0..4 {
            game.step(&button(2));
        }
        assert_eq!(game.switches, 1);
        assert_eq!(game.robot, Cell { x: 4, y: 3 });
        assert!(game.simple(Op::Forward).is_ok());
        assert_eq!(game.robot, Cell { x: 5, y: 3 });
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.switches, 0);
        assert_eq!(game.carrying, None);
        game.robot = Cell { x: 4, y: 3 };
        assert!(game.simple(Op::Forward).is_err());
        assert_eq!(game.robot, Cell { x: 4, y: 3 });
    }
    #[test]
    fn pits_and_closed_b_gate_reject_movement_without_translation() {
        let mut game = Game::new();
        game.level = 9;
        game.reset_board();
        game.robot = Cell { x: 6, y: 4 };
        game.facing = 0;
        assert!(game.simple(Op::Forward).is_err());
        assert_eq!(game.robot, Cell { x: 6, y: 4 });
        game.switches = 2;
        assert!(game.simple(Op::Forward).is_ok());
        game.robot = Cell { x: 6, y: 4 };
        game.facing = 3;
        assert!(game.simple(Op::Forward).is_err());
        assert_eq!(game.robot, Cell { x: 6, y: 4 });
    }
    #[test]
    fn repeats_expand_individually_and_never_repeat_a_repeat_instruction() {
        let mut game = Game::new();
        game.level = 5;
        game.reset_board();
        enter(&mut game, "R32");
        game.step(&button(2));
        assert_eq!(game.facing, 2);
        game.step(&button(2));
        assert_eq!(game.repeats, 3);
        assert_eq!(game.facing, 2);
        game.step(&button(2));
        assert_eq!(game.repeats, 2);
        assert_eq!(game.facing, 3);
        game.step(&button(2));
        game.step(&button(2));
        assert_eq!(game.facing, 1);
        game.step(&button(2));
        assert_eq!(game.repeats, 2);
        game.step(&button(2));
        game.step(&button(2));
        assert_eq!(game.facing, 3);
        assert_eq!(game.steps, 8);
        game.step(&button(2));
        assert_eq!(game.phase, Phase::Fault);
        assert!(game.notice.contains("Tape ended"));
        let mut game = Game::new();
        game.level = 5;
        game.reset_board();
        enter(&mut game, "3");
        game.step(&button(2));
        assert_eq!(game.phase, Phase::Fault);
        assert_eq!(game.steps, 1);
    }
    #[test]
    fn repeat_unlocks_and_hard_execution_budget_are_enforced() {
        let mut game = Game::new();
        for _ in 0..5 {
            game.step(&Input {
                right_pressed: true,
                ..Input::default()
            });
        }
        assert_eq!(game.palette, 0);
        game.level = 4;
        assert_eq!(game.unlocked(), 6);
        game.level = 5;
        assert_eq!(game.unlocked(), 7);
        game.reset_board();
        enter(&mut game, "R3");
        game.step(&button(2));
        game.steps = STEP_LIMIT;
        let before = game.robot;
        game.step(&button(2));
        assert_eq!(game.phase, Phase::Fault);
        assert_eq!(game.steps, STEP_LIMIT);
        assert_eq!(game.robot, before);
    }
    #[test]
    fn hints_pause_execution_and_medal_penalties_survive_editor_undo() {
        let mut game = Game::new();
        enter(&mut game, ROOMS[0].solution);
        game.step(&button(1));
        game.step(&button(5));
        assert_eq!(game.phase, Phase::Paused);
        assert!(game.hinted && game.show_hint);
        game.step(&button(5));
        assert!(!game.show_hint);
        assert!(game.hinted);
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        game.step(&button(3));
        game.step(&button(4));
        solve(&mut game);
        assert_eq!(game.stars[0], 2);
        assert!(game.hinted);
        let score = game.score;
        for _ in 0..100 {
            game.step(&button(1));
        }
        assert_eq!(game.score, score);
    }
    fn click(game: &mut Game, at: ferravex_arcade::Vec2) -> Cue {
        game.step(&Input {
            click: true,
            pointer: at,
            ..Input::default()
        })
    }
    #[test]
    fn pointer_targets_match_drawn_geometry_and_mirror_keys() {
        use crate::layout::{Button, palette, tape};
        let mut game = Game::new();
        enter(&mut game, "FLR");
        click(&mut game, tape(1).center());
        assert_eq!(game.tape.cursor, 1);
        // Clicking empty cells past the append cell does nothing.
        click(&mut game, tape(9).center());
        assert_eq!(game.tape.cursor, 1);
        // A palette tile selects its instruction and writes it like Space.
        assert_eq!(click(&mut game, palette(3).center()), Cue::Action);
        assert_eq!(game.palette, 3);
        assert_eq!(game.tape.ops[..3], [Op::Forward, Op::Grab, Op::Right]);
        assert_eq!(game.tape.cursor, 2);
        click(&mut game, tape(3).center());
        click(&mut game, palette(4).center());
        assert_eq!(game.tape.len, 4);
        assert_eq!(game.tape.ops[3], Op::Drop);
        // Repeats are locked in the first room: nothing changes.
        let before = game.clone();
        assert_eq!(click(&mut game, palette(5).center()), Cue::None);
        assert_eq!((game.tape, game.palette), (before.tape, before.palette));
        // Buttons mirror 2 and C.
        click(&mut game, Button::Step.rect().center());
        assert_eq!(game.phase, Phase::Paused);
        assert_eq!(game.steps, 1);
        assert_eq!(game.robot, Cell { x: 2, y: 3 });
        let tape_before = game.tape;
        assert_eq!(click(&mut game, Button::Stop.rect().center()), Cue::Action);
        assert_eq!(game.phase, Phase::Editing);
        assert_eq!(game.robot, Cell { x: 1, y: 3 });
        assert_eq!(game.tape, tape_before);
        assert_eq!(game.steps, 0);
        click(&mut game, Button::Clear.rect().center());
        assert_eq!(game.tape.len, 0);
        click(&mut game, Button::Undo.rect().center());
        assert_eq!(game.tape, tape_before);
        click(&mut game, Button::Hint.rect().center());
        assert!(game.show_hint && game.hinted);
        let tape = game.tape;
        click(&mut game, vec2(-50., -50.));
        assert_eq!(game.tape, tape);
    }
    #[test]
    fn solved_room_continues_from_the_next_button_or_space() {
        use crate::layout::NEXT;
        let mut game = Game::new();
        solve(&mut game);
        click(&mut game, vec2(NEXT.x - 20., NEXT.y + 10.));
        assert_eq!(game.room(), 1);
        click(&mut game, NEXT.center());
        assert_eq!(game.room(), 2);
        assert_eq!(game.phase, Phase::Editing);
        solve(&mut game);
        game.step(&press());
        assert_eq!(game.room(), 3);
    }
    #[test]
    fn presentation_events_record_accepted_actions_for_one_update() {
        let mut game = Game::new();
        game.step(&press());
        assert_eq!(
            game.look.events().collect::<Vec<_>>(),
            [Event::Wrote {
                index: 0,
                op: Op::Forward
            }]
        );
        game.step(&Input::default());
        assert_eq!(game.look.events().count(), 0);
        enter(&mut game, "G");
        game.step(&button(2));
        let events: Vec<_> = game.look.events().collect();
        assert_eq!(
            events,
            [
                Event::Started,
                Event::Moved {
                    from: Cell { x: 1, y: 3 },
                    to: Cell { x: 2, y: 3 }
                }
            ]
        );
        assert_eq!(game.look.from, Cell { x: 1, y: 3 });
        assert_eq!(game.look.age, 0);
        game.step(&button(2));
        assert_eq!(
            game.look.events().collect::<Vec<_>>(),
            [Event::Grabbed {
                at: Cell { x: 2, y: 3 },
                parcel: 0
            }]
        );
        assert_eq!(game.look.path(), [Cell { x: 1, y: 3 }, Cell { x: 2, y: 3 }]);
        // The tape runs out: the fault names the append cell and the robot.
        game.step(&button(2));
        let fault = Fault {
            kind: FaultKind::TapeEnded,
            at: Cell { x: 2, y: 3 },
            pc: Some(2),
        };
        assert_eq!(
            game.look.events().collect::<Vec<_>>(),
            [Event::Fault(fault)]
        );
        assert_eq!(game.look.fault, Some(fault));
        assert!(game.notice.contains("Tape ended"));
        game.step(&press());
        assert_eq!(game.look.fault, None);
        assert_eq!(game.look.path(), [Cell { x: 1, y: 3 }]);
        // A blocked move faults at the wall it hit, on the executing cell.
        let mut game = Game::new();
        enter(&mut game, "LFFF");
        for _ in 0..4 {
            game.step(&button(2));
        }
        let fault = game.look.fault.expect("wall fault");
        assert_eq!(fault.kind, FaultKind::Wall);
        assert_eq!(fault.at, Cell { x: 1, y: 0 });
        assert_eq!(fault.pc, Some(3));
        assert!(fault.kind.blocked_move());
        // Refusals are recorded without changing the tape.
        let mut game = Game::new();
        for _ in 0..11 {
            game.step(&press());
        }
        assert_eq!(game.tape.len, 10);
        assert_eq!(
            game.look.events().collect::<Vec<_>>(),
            [Event::TapeFull { index: 9 }]
        );
    }
    #[test]
    fn switch_events_name_their_gate_once_and_the_route_hint_is_read_only() {
        let mut game = Game::new();
        game.level = 6;
        game.reset_board();
        enter(&mut game, "FGFF");
        let mut switched = 0;
        for _ in 0..4 {
            game.step(&button(2));
            switched += game
                .look
                .events()
                .filter(|e| matches!(e, Event::Switch { gate: b'A', .. }))
                .count();
        }
        assert_eq!(switched, 1);
        let before = game.clone();
        let route = game.reference_route();
        assert_eq!(game, before);
        assert_eq!(route.phase, Phase::Solved);
        let path = route.look.path();
        assert_eq!(path.first(), Some(&Cell { x: 1, y: 3 }));
        assert_eq!(path.last(), Some(&Cell { x: 7, y: 3 }));
        assert_eq!(path.len(), 7);
    }
    #[test]
    fn medal_forecast_matches_the_awarded_medal() {
        let mut game = Game::new();
        enter(&mut game, "FGFFFFD");
        assert_eq!(game.medal(game.tape.len), 3);
        assert_eq!(game.medal(game.par() + 4), 2);
        assert_eq!(game.medal(game.par() + 5), 1);
        game.step(&button(1));
        for _ in 0..400 {
            game.step(&Input::default());
        }
        assert_eq!(game.phase, Phase::Solved);
        assert_eq!(game.stars[0], 3);
        assert_eq!(game.score(), points(3, 7));
    }
}
