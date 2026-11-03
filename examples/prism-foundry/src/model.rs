//! Pure, bounded optical puzzle rules. A tick consumes only the supplied command.
//!
//! ```
//! use ferravex_arcade::{Input, Outcome};
//! use prism_foundry::Game;
//! let mut game = Game::new();
//! assert_eq!(game.chamber(), 1);
//! game.tick(&Input { primary: true, ..Input::default() });
//! assert_eq!(game.rotations(), 1);
//! game.tick(&Input { secondary: true, ..Input::default() });
//! assert_eq!(game.rotations(), 0);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{CHAMBERS, Chamber};
use ferravex_arcade::{Cue, Input, Outcome, Vec2};

pub(crate) const COLS: usize = 11;
pub(crate) const ROWS: usize = 7;
pub(crate) const CELLS: usize = COLS * ROWS;
pub(crate) const MAX_PIECES: usize = 24;
const MAX_STATES: usize = CELLS * 4 * 3;
const HISTORY: usize = 128;
pub(crate) const BOARD_X: f32 = 28.;
pub(crate) const BOARD_Y: f32 = 160.;
pub(crate) const TILE: f32 = 52.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Emitter { direction: u8, color: u8 },
    Mirror,
    Splitter,
    Filter,
    Receiver(u8),
    Wall,
}
impl Kind {
    pub(crate) fn dial_count(self) -> u8 {
        match self {
            Self::Mirror | Self::Splitter => 2,
            Self::Filter => 3,
            _ => 0,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Piece {
    pub x: u8,
    pub y: u8,
    pub kind: Kind,
    pub start: u8,
    pub solution: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Editing,
    Cleared,
    Finished,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Trace {
    /// Outgoing half-edges; neighboring cell receives the matching incoming half.
    pub edges: [[u8; 4]; CELLS],
    pub received: [u8; MAX_PIECES],
    pub states: usize,
    pub looped: bool,
}
#[derive(Clone, Copy, Debug, Default)]
struct Ray {
    x: u8,
    y: u8,
    dir: u8,
    color: u8,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Change {
    piece: usize,
    before: u8,
}

/// One twelve-chamber campaign. Construction and updates do not access a renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) level: usize,
    pub(crate) phase: Phase,
    pub(crate) dials: [u8; MAX_PIECES],
    pub(crate) cursor: (u8, u8),
    pub(crate) trace: Trace,
    history: [Change; HISTORY],
    history_len: usize,
    moves: u32,
    pub(crate) resets: u16,
    pub(crate) mistakes: u16,
    pub(crate) hinted: bool,
    pub(crate) stars: [u8; 12],
    score: u32,
    pub(crate) age: u32,
    pub(crate) phase_age: u32,
    pub(crate) notice: &'static str,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// Begin a fresh campaign in the first chamber, with no score or solved locks.
    pub fn new() -> Self {
        let mut game = Self {
            level: 0,
            phase: Phase::Editing,
            dials: [0; MAX_PIECES],
            cursor: (0, 0),
            trace: Trace {
                edges: [[0; 4]; CELLS],
                received: [0; MAX_PIECES],
                states: 0,
                looped: false,
            },
            history: [Change::default(); HISTORY],
            history_len: 0,
            moves: 0,
            resets: 0,
            mistakes: 0,
            hinted: false,
            stars: [0; 12],
            score: 0,
            age: 0,
            phase_age: 0,
            notice: "Follow the beam. Click a dial or use arrows and Space.",
        };
        game.load();
        game
    }
    pub(crate) fn content(&self) -> &'static Chamber {
        &CHAMBERS[self.level]
    }
    fn load(&mut self) {
        self.dials = [0; MAX_PIECES];
        for (i, piece) in self.content().pieces.iter().enumerate() {
            self.dials[i] = piece.start;
        }
        if let Some(p) = self
            .content()
            .pieces
            .iter()
            .find(|p| p.kind.dial_count() > 0)
        {
            self.cursor = (p.x, p.y);
        }
        self.phase = Phase::Editing;
        self.phase_age = 0;
        self.history_len = 0;
        self.moves = 0;
        self.resets = 0;
        self.mistakes = 0;
        self.hinted = false;
        self.notice = "Match every lock, then press 5 to seal this chamber.";
        self.retrace();
    }
    /// Current chamber, numbered 1 through 12.
    pub fn chamber(&self) -> usize {
        self.level + 1
    }
    /// Number of effective rotations in the current arrangement; undo removes one.
    pub fn rotations(&self) -> u32 {
        self.moves
    }
    /// Cumulative score, awarded once when each complete chamber is sealed.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Won after the twelfth seal. Puzzles allow unlimited local retries, not a loss screen.
    pub fn outcome(&self) -> Outcome {
        if self.phase == Phase::Finished {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub(crate) fn par(&self) -> u32 {
        self.content()
            .pieces
            .iter()
            .map(|p| {
                let n = p.kind.dial_count();
                if n == 0 {
                    0
                } else {
                    u32::from((p.solution + n - p.start) % n)
                }
            })
            .sum()
    }
    pub(crate) fn selected(&self) -> Option<usize> {
        self.content()
            .pieces
            .iter()
            .position(|p| (p.x, p.y) == self.cursor)
    }
    pub(crate) fn locks(&self) -> (usize, usize) {
        let mut lit = 0;
        let mut total = 0;
        for (i, piece) in self.content().pieces.iter().enumerate() {
            if let Kind::Receiver(target) = piece.kind {
                total += 1;
                lit += usize::from(self.trace.received[i] == target);
            }
        }
        (lit, total)
    }
    fn retrace(&mut self) {
        self.trace = trace(self.content().pieces, &self.dials);
    }
    fn rotate(&mut self, index: usize) -> Cue {
        let p = self.content().pieces[index];
        let count = p.kind.dial_count();
        if count == 0 {
            return Cue::None;
        }
        if self.history_len == HISTORY {
            self.history.copy_within(1..HISTORY, 0);
            self.history_len -= 1;
        }
        self.history[self.history_len] = Change {
            piece: index,
            before: self.dials[index],
        };
        self.history_len += 1;
        self.dials[index] = (self.dials[index] + 1) % count;
        self.moves = self.moves.saturating_add(1);
        self.retrace();
        self.notice = if self.locks().0 == self.locks().1 {
            "All locks match! Press 5 to seal the chamber."
        } else {
            "Trace the branches. Only exact colors light a lock."
        };
        Cue::Action
    }
    fn seal(&mut self) -> Cue {
        let (lit, total) = self.locks();
        if lit != total {
            self.mistakes = self.mistakes.saturating_add(1);
            self.notice = "Seal rejected: every lock must match its exact color.";
            return Cue::Bad;
        }
        let stars =
            if self.moves <= self.par() && !self.hinted && self.resets == 0 && self.mistakes == 0 {
                3
            } else if self.moves <= self.par() * 2 && self.resets <= 2 {
                2
            } else {
                1
            };
        self.stars[self.level] = stars;
        self.score += 1000 + u32::from(stars) * 250;
        self.phase_age = 0;
        if self.level == CHAMBERS.len() - 1 {
            self.phase = Phase::Finished;
            self.notice = "The Daybreak Engine is awake. Light returns to the city.";
            Cue::Win
        } else {
            self.phase = Phase::Cleared;
            self.notice = "Chamber sealed. Space opens the next chamber.";
            Cue::Good
        }
    }
    /// Execute one 60 Hz input snapshot. Only pressed edges rotate, undo or seal.
    /// Terminal calls are inert. X undoes up to 128 retained turns; C resets locally.
    pub fn tick(&mut self, input: &Input) -> Cue {
        if self.phase == Phase::Finished {
            return Cue::None;
        }
        self.age = self.age.saturating_add(1);
        self.phase_age = self.phase_age.saturating_add(1);
        let button = input.click
            && input.pointer.x >= 622.
            && input.pointer.x < 932.
            && input.pointer.y >= 510.
            && input.pointer.y < 552.;
        if self.phase == Phase::Cleared {
            if input.primary || input.slot == Some(5) || button {
                self.level += 1;
                self.load();
                return Cue::Good;
            }
            return Cue::None;
        }
        if input.secondary {
            if self.history_len == 0 {
                return Cue::None;
            }
            self.history_len -= 1;
            let change = self.history[self.history_len];
            self.dials[change.piece] = change.before;
            let piece = self.content().pieces[change.piece];
            self.cursor = (piece.x, piece.y);
            self.moves = self.moves.saturating_sub(1);
            self.retrace();
            self.notice = "Last turn undone. The previous light path is restored.";
            return Cue::Action;
        }
        if input.tertiary {
            if self.moves == 0 {
                return Cue::None;
            }
            for (i, piece) in self.content().pieces.iter().enumerate() {
                self.dials[i] = piece.start;
            }
            self.moves = 0;
            self.history_len = 0;
            self.resets = self.resets.saturating_add(1);
            self.retrace();
            self.notice = "Chamber reset. Prior hints and seal attempts still count.";
            return Cue::Action;
        }
        if input.slot == Some(4) {
            if self.hinted {
                return Cue::None;
            }
            self.hinted = true;
            self.notice = "Engineer hint unlocked; this chamber can earn two stars.";
            return Cue::Action;
        }
        if input.slot == Some(5) || button {
            return self.seal();
        }
        let previous = self.cursor;
        if input.left_pressed {
            self.cursor.0 = self.cursor.0.saturating_sub(1);
        }
        if input.right_pressed {
            self.cursor.0 = (self.cursor.0 + 1).min(COLS as u8 - 1);
        }
        if input.up_pressed {
            self.cursor.1 = self.cursor.1.saturating_sub(1);
        }
        if input.down_pressed {
            self.cursor.1 = (self.cursor.1 + 1).min(ROWS as u8 - 1);
        }
        if input.click {
            let x = (input.pointer.x - BOARD_X) / TILE;
            let y = (input.pointer.y - BOARD_Y) / TILE;
            if x >= 0. && x < COLS as f32 && y >= 0. && y < ROWS as f32 {
                self.cursor = (x as u8, y as u8);
                return self.selected().map_or(Cue::None, |i| self.rotate(i));
            }
        }
        if input.primary {
            return self.selected().map_or(Cue::None, |i| self.rotate(i));
        }
        if previous != self.cursor {
            Cue::Action
        } else {
            Cue::None
        }
    }
    /// Read-only demonstration controller. Sends normal pointer rotations and seal keys.
    /// The reference arrangement is authored content, never applied directly to game state.
    pub fn pilot(&self, tick: u64) -> Input {
        if self.phase == Phase::Finished || !tick.is_multiple_of(30) {
            return Input::default();
        }
        if self.phase == Phase::Cleared {
            return Input {
                primary: self.phase_age >= 120,
                ..Input::default()
            };
        }
        for (i, p) in self.content().pieces.iter().enumerate() {
            if p.kind.dial_count() > 0 && self.dials[i] != p.solution {
                return Input {
                    click: true,
                    pointer: Vec2::new(
                        BOARD_X + (f32::from(p.x) + 0.5) * TILE,
                        BOARD_Y + (f32::from(p.y) + 0.5) * TILE,
                    ),
                    ..Input::default()
                };
            }
        }
        Input {
            slot: Some(5),
            ..Input::default()
        }
    }
}

fn reflect(direction: u8, dial: u8) -> u8 {
    if dial == 0 {
        [3, 2, 1, 0][usize::from(direction)]
    } else {
        [1, 0, 3, 2][usize::from(direction)]
    }
}
fn trace(pieces: &[Piece], dials: &[u8; MAX_PIECES]) -> Trace {
    let mut result = Trace {
        edges: [[0; 4]; CELLS],
        received: [0; MAX_PIECES],
        states: 0,
        looped: false,
    };
    let mut occupancy = [None; CELLS];
    for (i, p) in pieces.iter().enumerate() {
        occupancy[usize::from(p.y) * COLS + usize::from(p.x)] = Some(i);
    }
    let mut queue = [Ray::default(); MAX_STATES];
    let mut seen = [[0u8; 4]; CELLS];
    let mut tail = 0;
    let mut head = 0;
    // Admission deduplicates color channels before enqueue: at most 77 * 4 * 3 states.
    let mut send =
        |ray: Ray, queue: &mut [Ray; MAX_STATES], tail: &mut usize, result: &mut Trace| {
            let at = usize::from(ray.y) * COLS + usize::from(ray.x);
            let fresh = ray.color & !seen[at][usize::from(ray.dir)];
            if fresh != ray.color {
                result.looped = true;
            }
            if fresh == 0 {
                return;
            }
            seen[at][usize::from(ray.dir)] |= fresh;
            queue[*tail] = Ray {
                color: fresh,
                ..ray
            };
            *tail += 1;
        };
    for p in pieces {
        if let Kind::Emitter { direction, color } = p.kind {
            send(
                Ray {
                    x: p.x,
                    y: p.y,
                    dir: direction,
                    color,
                },
                &mut queue,
                &mut tail,
                &mut result,
            );
        }
    }
    while head < tail {
        let ray = queue[head];
        head += 1;
        result.states += 1;
        result.edges[usize::from(ray.y) * COLS + usize::from(ray.x)][usize::from(ray.dir)] |=
            ray.color;
        let (dx, dy) = [(1, 0), (0, 1), (-1, 0), (0, -1)][usize::from(ray.dir)];
        let x = i16::from(ray.x) + dx;
        let y = i16::from(ray.y) + dy;
        if x < 0 || y < 0 || x >= COLS as i16 || y >= ROWS as i16 {
            continue;
        }
        let next = Ray {
            x: x as u8,
            y: y as u8,
            ..ray
        };
        if let Some(i) = occupancy[y as usize * COLS + x as usize] {
            match pieces[i].kind {
                Kind::Emitter { .. } | Kind::Wall => {}
                Kind::Receiver(_) => result.received[i] |= ray.color,
                Kind::Mirror => send(
                    Ray {
                        dir: reflect(ray.dir, dials[i]),
                        ..next
                    },
                    &mut queue,
                    &mut tail,
                    &mut result,
                ),
                Kind::Splitter => {
                    send(next, &mut queue, &mut tail, &mut result);
                    send(
                        Ray {
                            dir: reflect(ray.dir, dials[i]),
                            ..next
                        },
                        &mut queue,
                        &mut tail,
                        &mut result,
                    );
                }
                Kind::Filter => {
                    let color = ray.color & (1 << dials[i]);
                    if color != 0 {
                        send(Ray { color, ..next }, &mut queue, &mut tail, &mut result);
                    }
                }
            }
        } else {
            send(next, &mut queue, &mut tail, &mut result);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use Kind::*;
    fn piece(x: u8, y: u8, kind: Kind, start: u8) -> Piece {
        Piece {
            x,
            y,
            kind,
            start,
            solution: start,
        }
    }
    fn command(slot: usize) -> Input {
        Input {
            slot: Some(slot),
            ..Input::default()
        }
    }
    fn click(p: &Piece) -> Input {
        Input {
            click: true,
            pointer: Vec2::new(
                BOARD_X + (f32::from(p.x) + 0.5) * TILE,
                BOARD_Y + (f32::from(p.y) + 0.5) * TILE,
            ),
            ..Input::default()
        }
    }
    fn solve_current(game: &mut Game) {
        let pieces = game.content().pieces;
        for (i, p) in pieces.iter().enumerate() {
            for _ in 0..3 {
                if p.kind.dial_count() == 0 || game.dials[i] == p.solution {
                    break;
                }
                assert_eq!(game.tick(&click(p)), Cue::Action);
            }
        }
        assert_eq!(
            game.locks().0,
            game.locks().1,
            "{}: {:?}",
            game.content().name,
            game.trace.received
        );
    }
    #[test]
    fn all_twelve_chambers_complete_by_ordinary_controls() {
        let mut game = Game::new();
        let mut mirror = Game::new();
        let mut visited = [false; 12];
        let mut ended = None;
        for tick in 0..20_000 {
            visited[game.level] = true;
            let input = game.pilot(tick);
            game.tick(&input);
            mirror.tick(&input);
            assert_eq!(game, mirror, "replay diverged at {tick}");
            assert!(game.trace.states <= MAX_STATES);
            if game.outcome() == Outcome::Won {
                ended = Some(tick);
                break;
            }
        }
        assert_eq!(
            game.outcome(),
            Outcome::Won,
            "{}: {:?}",
            game.content().name,
            game.trace.received
        );
        assert!(visited.into_iter().all(|v| v));
        assert_eq!(game.stars, [3; 12]);
        assert_eq!(game.score(), 21_000);
        eprintln!(
            "Legal campaign won at tick {} with {} points",
            ended.unwrap(),
            game.score()
        );
        let frozen = game.clone();
        for slot in 1..=5 {
            assert_eq!(
                game.tick(&Input {
                    primary: true,
                    secondary: true,
                    tertiary: true,
                    click: true,
                    pointer: Vec2::ZERO,
                    slot: Some(slot),
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn every_authored_chamber_starts_unsolved_and_has_a_legal_par_solution() {
        let mut game = Game::new();
        for level in 0..12 {
            assert_eq!(game.level, level);
            assert!(
                game.locks().0 < game.locks().1,
                "initially solved: {}",
                game.content().name
            );
            assert!(game.content().pieces.len() <= MAX_PIECES);
            let mut occupied = [false; CELLS];
            for p in game.content().pieces {
                assert!(usize::from(p.x) < COLS && usize::from(p.y) < ROWS);
                let at = usize::from(p.y) * COLS + usize::from(p.x);
                assert!(
                    !occupied[at],
                    "overlapping devices in {}",
                    game.content().name
                );
                occupied[at] = true;
            }
            solve_current(&mut game);
            assert_eq!(game.rotations(), game.par());
            game.tick(&command(5));
            assert_eq!(game.stars[level], 3);
            if level < 11 {
                game.tick(&Input {
                    primary: true,
                    ..Input::default()
                });
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
    }
    #[test]
    fn mirrors_obey_both_reflection_tables_and_are_reversible() {
        assert_eq!(
            (0..4).map(|d| reflect(d, 0)).collect::<Vec<_>>(),
            vec![3, 2, 1, 0]
        );
        assert_eq!(
            (0..4).map(|d| reflect(d, 1)).collect::<Vec<_>>(),
            vec![1, 0, 3, 2]
        );
        for dial in 0..2 {
            for d in 0..4 {
                assert_eq!(reflect((reflect(d, dial) + 2) % 4, dial), (d + 2) % 4);
            }
        }
    }
    #[test]
    fn splitter_powers_straight_and_reflected_branches() {
        let pieces = [
            piece(
                0,
                3,
                Emitter {
                    direction: 0,
                    color: 1,
                },
                0,
            ),
            piece(3, 3, Splitter, 0),
            piece(8, 3, Receiver(1), 0),
            piece(3, 0, Receiver(1), 0),
        ];
        let output = trace(&pieces, &[0; MAX_PIECES]);
        assert_eq!(output.received[2], 1);
        assert_eq!(output.received[3], 1);
    }
    #[test]
    fn filters_remove_channels_and_incompatible_filters_absorb() {
        let pieces = [
            piece(
                0,
                3,
                Emitter {
                    direction: 0,
                    color: 7,
                },
                0,
            ),
            piece(2, 3, Filter, 0),
            piece(4, 3, Filter, 0),
            piece(8, 3, Receiver(1), 0),
        ];
        let mut dials = [0; MAX_PIECES];
        assert_eq!(trace(&pieces, &dials).received[3], 1);
        dials[2] = 2;
        assert_eq!(trace(&pieces, &dials).received[3], 0);
        dials[1] = 2;
        assert_eq!(trace(&pieces, &dials).received[3], 4);
    }
    #[test]
    fn lock_color_is_the_union_of_arriving_channels_including_contamination() {
        let pieces = [
            piece(
                0,
                3,
                Emitter {
                    direction: 0,
                    color: 1,
                },
                0,
            ),
            piece(
                3,
                0,
                Emitter {
                    direction: 1,
                    color: 2,
                },
                0,
            ),
            piece(3, 3, Receiver(3), 0),
            piece(
                8,
                3,
                Emitter {
                    direction: 2,
                    color: 4,
                },
                0,
            ),
        ];
        assert_eq!(trace(&pieces[..3], &[0; MAX_PIECES]).received[2], 3);
        assert_eq!(trace(&pieces, &[0; MAX_PIECES]).received[2], 7);
    }
    #[test]
    fn closed_splitter_loop_terminates_and_keeps_its_escape_beam() {
        let pieces = [
            piece(
                0,
                1,
                Emitter {
                    direction: 0,
                    color: 7,
                },
                0,
            ),
            piece(2, 1, Splitter, 0),
            piece(5, 1, Mirror, 1),
            piece(5, 4, Mirror, 0),
            piece(2, 4, Mirror, 1),
            piece(2, 0, Receiver(7), 0),
        ];
        let mut dials = [0; MAX_PIECES];
        dials[2] = 1;
        dials[4] = 1;
        let output = trace(&pieces, &dials);
        assert!(output.looped);
        assert!(output.states < 100);
        assert_eq!(output.received[5], 7);
        assert_eq!(output, trace(&pieces, &dials));
    }
    #[test]
    fn walls_sources_and_receivers_absorb_incoming_beams() {
        for blocker in [
            Wall,
            Emitter {
                direction: 3,
                color: 2,
            },
            Receiver(1),
        ] {
            let pieces = [
                piece(
                    0,
                    3,
                    Emitter {
                        direction: 0,
                        color: 1,
                    },
                    0,
                ),
                piece(3, 3, blocker, 0),
                piece(8, 3, Receiver(1), 0),
            ];
            assert_eq!(trace(&pieces, &[0; MAX_PIECES]).received[2], 0);
        }
    }
    #[test]
    fn undo_restores_arrangement_and_reset_keeps_hint_and_attempt_costs() {
        let mut game = Game::new();
        let dials = game.dials;
        let trace = game.trace.clone();
        game.tick(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(game.rotations(), 1);
        game.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.dials, dials);
        assert_eq!(game.trace, trace);
        assert_eq!(game.rotations(), 0);
        game.tick(&command(4));
        game.tick(&command(5));
        game.tick(&Input {
            primary: true,
            ..Input::default()
        });
        game.tick(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.dials, dials);
        assert_eq!(game.resets, 1);
        assert_eq!(game.mistakes, 1);
        assert!(game.hinted);
        assert_eq!(
            game.tick(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(
            game.tick(&Input {
                tertiary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.resets, 1);
    }
    #[test]
    fn bounded_undo_rejects_underflow_and_never_farms_points() {
        let mut game = Game::new();
        for _ in 0..300 {
            game.tick(&Input {
                primary: true,
                ..Input::default()
            });
        }
        assert_eq!(game.history_len, HISTORY);
        assert_eq!(game.rotations(), 300);
        for _ in 0..HISTORY {
            game.tick(&Input {
                secondary: true,
                ..Input::default()
            });
        }
        assert_eq!(game.rotations(), 172);
        assert_eq!(game.history_len, 0);
        assert_eq!(
            game.tick(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.score(), 0);
    }
    #[test]
    fn failed_seals_do_not_award_progress_and_cleared_chambers_award_once() {
        let mut game = Game::new();
        for _ in 0..3 {
            assert_eq!(game.tick(&command(5)), Cue::Bad);
        }
        assert_eq!(game.score(), 0);
        assert_eq!(game.chamber(), 1);
        solve_current(&mut game);
        assert_eq!(game.tick(&command(5)), Cue::Good);
        assert_eq!(game.stars[0], 2);
        assert_eq!(game.score(), 1500);
        let dials = game.dials;
        for _ in 0..10 {
            game.tick(&Input {
                secondary: true,
                tertiary: true,
                ..Input::default()
            });
        }
        assert_eq!(game.dials, dials);
        assert_eq!(game.score(), 1500);
        game.tick(&command(5));
        assert_eq!(game.chamber(), 2);
        assert_eq!(game.score(), 1500);
    }
    #[test]
    fn hint_cap_and_long_solutions_produce_distinct_medals() {
        let mut game = Game::new();
        game.tick(&command(4));
        solve_current(&mut game);
        game.tick(&command(5));
        assert_eq!(game.stars[0], 2);
        let mut game = Game::new();
        for _ in 0..10 {
            game.tick(&Input {
                primary: true,
                ..Input::default()
            });
        }
        solve_current(&mut game);
        game.tick(&command(5));
        assert_eq!(game.stars[0], 1);
    }
    #[test]
    fn pointer_and_keyboard_rotate_the_same_device_and_holds_do_not_repeat() {
        let mut keyboard = Game::new();
        let mut pointer = Game::new();
        let device = pointer.content().pieces[pointer.selected().unwrap()];
        keyboard.tick(&Input {
            primary: true,
            ..Input::default()
        });
        pointer.tick(&click(&device));
        assert_eq!(keyboard, pointer);
        let before = keyboard.dials;
        for _ in 0..60 {
            keyboard.tick(&Input {
                primary_down: true,
                left: true,
                ..Input::default()
            });
        }
        assert_eq!(keyboard.dials, before);
        assert_eq!(keyboard.rotations(), 1);
        assert_eq!(keyboard.cursor, (device.x, device.y));
    }
    #[test]
    fn invalid_pointer_locations_and_non_devices_cannot_spend_turns() {
        let mut game = Game::new();
        let initial = game.dials;
        for point in [
            Vec2::new(-1., 200.),
            Vec2::new(900., 100.),
            Vec2::new(f32::NAN, 200.),
            Vec2::new(BOARD_X + 10., BOARD_Y + 10.),
        ] {
            assert_eq!(
                game.tick(&Input {
                    click: true,
                    pointer: point,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(
            game.tick(&Input {
                primary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.dials, initial);
        assert_eq!(game.rotations(), 0);
        assert_eq!(game.score(), 0);
    }
}
