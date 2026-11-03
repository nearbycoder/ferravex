//! Pure fixed-tick rhythm rules, authored chart expansion and concert progression.
//!
//! ```
//! use voltage_choir::Game;
//! use ferravex_arcade::{Input, Outcome};
//! let mut concert = Game::new();
//! assert_eq!(concert.track(), 1);
//! concert.step(&Input { primary: true, ..Input::default() });
//! for tick in 0..300 {
//!     let command = concert.autoplay(tick);
//!     concert.step(&command);
//! }
//! assert_eq!(concert.outcome(), Outcome::Playing);
//! assert!(concert.score() > 0);
//! ```
use crate::content::{MAX_NOTES, TRACK_COUNT, TRACKS};
use ferravex_arcade::{Cue, Input, Outcome};

pub(crate) const PERFECT: u32 = 2;
pub(crate) const GOOD: u32 = 4;
pub(crate) const WINDOW: u32 = 7;
const START_CROWD: i32 = 80;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Judgment {
    #[default]
    Pending,
    Perfect,
    Good,
    Okay,
    Miss,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Note {
    pub tick: u32,
    pub lane: usize,
    pub result: Judgment,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Briefing,
    CountIn,
    Playing,
    Results,
    Retry,
    Won,
    Lost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flash {
    None,
    Perfect,
    Good,
    Okay,
    Miss,
    Stray,
}
impl Flash {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Perfect => "Perfect",
            Self::Good => "Good",
            Self::Okay => "Okay",
            Self::Miss => "Miss",
            Self::Stray => "Empty tap",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stats {
    pub perfect: u32,
    pub good: u32,
    pub okay: u32,
    pub misses: u32,
    pub stray: u32,
    pub score: u32,
    pub max_combo: u32,
}
impl Stats {
    pub fn judged(self) -> u32 {
        self.perfect + self.good + self.okay + self.misses
    }
    pub fn accuracy(self) -> u32 {
        if self.judged() == 0 {
            1000
        } else {
            (self.perfect * 1000 + self.good * 850 + self.okay * 600) / self.judged()
        }
    }
    pub fn grade(self) -> &'static str {
        match self.accuracy() {
            1000 if self.stray == 0 => "S+",
            970.. => "S",
            900.. => "A",
            800.. => "B",
            700.. => "C",
            _ => "D",
        }
    }
}

/// One bounded six-track concert. Use [`Self::step`] once per actual 60 Hz tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) track: usize,
    pub(crate) phase: Phase,
    pub(crate) clock: u32,
    pub(crate) notes: [Note; MAX_NOTES],
    pub(crate) len: usize,
    pub(crate) crowd: i32,
    pub(crate) combo: u32,
    pub(crate) charge: u32,
    pub(crate) encore_until: u32,
    pub(crate) retries: u8,
    pub(crate) attempt: Stats,
    pub(crate) results: [Option<Stats>; TRACK_COUNT],
    pub(crate) flashes: [Flash; 4],
    pub(crate) flash_ticks: [u8; 4],
    pub(crate) last_offset: i32,
    pub(crate) notice: &'static str,
    pub(crate) notes_on: [Option<u8>; 4],
    pub(crate) stage_ticks: u32,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Prepare the opening song. A primary edge begins its four-beat count-in.
    pub fn new() -> Self {
        let mut game = Self {
            track: 0,
            phase: Phase::Briefing,
            clock: 0,
            notes: [Note::default(); MAX_NOTES],
            len: 0,
            crowd: START_CROWD,
            combo: 0,
            charge: 0,
            encore_until: 0,
            retries: 3,
            attempt: Stats::default(),
            results: [None; TRACK_COUNT],
            flashes: [Flash::None; 4],
            flash_ticks: [0; 4],
            last_offset: 0,
            notice: "Ready to start.",
            notes_on: [None; 4],
            stage_ticks: 0,
        };
        game.load_chart();
        game
    }
    /// Current song, numbered 1 through 6.
    pub fn track(&self) -> usize {
        self.track + 1
    }
    /// Number of successfully completed songs.
    pub fn completed_tracks(&self) -> usize {
        self.results.iter().filter(|r| r.is_some()).count()
    }
    /// Accepted song scores plus the current unbanked attempt, without duplicates.
    pub fn score(&self) -> u32 {
        let total = self.results.iter().flatten().map(|r| r.score).sum::<u32>();
        if self.results[self.track].is_some() {
            total
        } else {
            total + self.attempt.score
        }
    }
    /// Concert outcome. A failed song offers bounded retries before terminal loss.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Won => Outcome::Won,
            Phase::Lost => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    /// Current song accuracy in thousandths. Pending notes do not count yet.
    pub fn accuracy(&self) -> u32 {
        self.attempt.accuracy()
    }
    /// Remaining song-retry tickets; starting a new concert restores three.
    pub fn retries(&self) -> u8 {
        self.retries
    }
    /// Musical note-ons produced by the most recently accepted tick, as MIDI pitches.
    /// The host reads this once after `step`; rendering must never replay it.
    pub fn music_notes(&self) -> [Option<u8>; 4] {
        self.notes_on
    }
    fn load_chart(&mut self) {
        let track = &TRACKS[self.track];
        self.len = 0;
        self.notes.fill(Note::default());
        for subdivision in 0..track.bars() * 8 {
            let mask = track.mask(subdivision);
            for lane in 0..4 {
                if mask & (1 << lane) != 0 {
                    assert!(self.len < MAX_NOTES, "authored chart capacity");
                    self.notes[self.len] = Note {
                        tick: subdivision * (track.beat_ticks / 2),
                        lane,
                        result: Judgment::Pending,
                    };
                    self.len += 1;
                }
            }
        }
    }
    fn begin(&mut self) {
        self.phase = Phase::CountIn;
        self.clock = 0;
        self.crowd = START_CROWD;
        self.combo = 0;
        self.charge = 0;
        self.encore_until = 0;
        self.attempt = Stats::default();
        self.flashes = [Flash::None; 4];
        self.flash_ticks = [0; 4];
        self.last_offset = 0;
        self.notes_on = [None; 4];
        self.stage_ticks = 0;
        self.notice = "Four beats. Watch the notes approach the line.";
        self.load_chart();
    }
    fn retry(&mut self) -> Cue {
        if self.retries == 0 {
            self.phase = Phase::Lost;
            self.notice = "No retry tickets remain. The city waits for another concert.";
            return Cue::Bad;
        }
        self.retries -= 1;
        self.begin();
        self.notice = "Fresh take. This song's previous points were discarded.";
        Cue::Action
    }
    fn fail(&mut self, reason: &'static str) -> Cue {
        self.notice = reason;
        self.phase = if self.retries > 0 {
            Phase::Retry
        } else {
            Phase::Lost
        };
        self.stage_ticks = 0;
        Cue::Bad
    }
    pub(crate) fn encore(&self) -> bool {
        self.phase == Phase::Playing && self.clock < self.encore_until
    }
    fn lane_edges(input: &Input) -> [bool; 4] {
        let mut lanes = [
            input.left_pressed,
            input.down_pressed,
            input.up_pressed,
            input.right_pressed,
        ];
        if let Some(slot) = input.slot.filter(|v| (1..=4).contains(v)) {
            lanes[slot - 1] = true;
        }
        if input.click
            && (input.pointer.y >= 160. && input.pointer.y <= 554.)
            && (input.pointer.x >= 180. && input.pointer.x < 660.)
        {
            lanes[((input.pointer.x - 180.) / 120.) as usize] = true;
        }
        lanes
    }
    fn flash(&mut self, lane: usize, flash: Flash) {
        self.flashes[lane] = flash;
        self.flash_ticks[lane] = 22;
    }
    fn strike(&mut self, lane: usize) -> Cue {
        let candidate = self.notes[..self.len]
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                n.lane == lane
                    && n.result == Judgment::Pending
                    && n.tick.abs_diff(self.clock) <= WINDOW
            })
            .min_by_key(|(_, n)| (n.tick.abs_diff(self.clock), n.tick))
            .map(|(i, _)| i);
        let Some(index) = candidate else {
            self.attempt.stray += 1;
            self.combo = 0;
            self.charge = self.charge.saturating_sub(5);
            if !self.encore() {
                self.crowd -= 2;
            }
            self.notice = "Empty tap: wait for a note to reach the line.";
            self.flash(lane, Flash::Stray);
            return Cue::Bad;
        };
        let offset = self.clock as i32 - self.notes[index].tick as i32;
        self.last_offset = offset;
        let (judgment, flash, points, charge) = match offset.unsigned_abs() {
            0..=PERFECT => (Judgment::Perfect, Flash::Perfect, 100, 4),
            3..=GOOD => (Judgment::Good, Flash::Good, 75, 3),
            _ => (Judgment::Okay, Flash::Okay, 45, 1),
        };
        self.notes[index].result = judgment;
        match judgment {
            Judgment::Perfect => self.attempt.perfect += 1,
            Judgment::Good => self.attempt.good += 1,
            Judgment::Okay => self.attempt.okay += 1,
            _ => {}
        }
        self.combo += 1;
        self.attempt.max_combo = self.attempt.max_combo.max(self.combo);
        let multiplier = 1 + (self.combo / 20).min(4);
        self.attempt.score += points * multiplier + if self.encore() { 25 } else { 0 };
        self.charge = (self.charge + charge).min(100);
        if judgment != Judgment::Okay {
            self.crowd = (self.crowd + 1).min(100);
        }
        self.notice = if self.charge == 100 {
            "Encore ready: press X for an eight-beat boost."
        } else {
            ""
        };
        self.flash(lane, flash);
        Cue::Good
    }
    /// Consume one tick's edges. Holding directions, inactive screens and invalid
    /// Encore requests cannot award notes or duplicate completed-song points.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        self.notes_on = [None; 4];
        self.stage_ticks = self.stage_ticks.saturating_add(1);
        for (flash, remaining) in self.flashes.iter_mut().zip(&mut self.flash_ticks) {
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                *flash = Flash::None;
            }
        }
        let accept = input.primary
            || (input.click
                && (240. ..=720.).contains(&input.pointer.x)
                && (491. ..=534.).contains(&input.pointer.y));
        match self.phase {
            Phase::Briefing => {
                if accept {
                    self.begin();
                    return Cue::Action;
                }
                Cue::None
            }
            Phase::Results => {
                if accept {
                    if self.track + 1 == TRACK_COUNT {
                        self.phase = Phase::Won;
                        self.notice = "Every window sings. The city has its voice again.";
                        return Cue::Win;
                    }
                    self.track += 1;
                    self.phase = Phase::Briefing;
                    self.clock = 0;
                    self.stage_ticks = 0;
                    self.attempt = Stats::default();
                    self.notice = "A new district is waiting. Read the song card, then count in.";
                    self.load_chart();
                    return Cue::Action;
                }
                Cue::None
            }
            Phase::Retry => {
                if accept || input.tertiary {
                    return self.retry();
                }
                if input.secondary {
                    self.phase = Phase::Lost;
                    return Cue::Bad;
                }
                Cue::None
            }
            Phase::CountIn => {
                if input.tertiary {
                    return self.retry();
                }
                let beat = TRACKS[self.track].beat_ticks;
                if self.clock.is_multiple_of(beat) {
                    self.notes_on[0] = Some(if self.clock == beat * 3 { 79 } else { 67 });
                }
                self.clock += 1;
                if self.clock == beat * 4 {
                    self.clock = 0;
                    self.phase = Phase::Playing;
                    self.stage_ticks = 0;
                    self.notice = "Play! Tap Left, Down, Up, Right at the bright line.";
                }
                Cue::None
            }
            Phase::Playing => self.play_tick(input),
            Phase::Won | Phase::Lost => Cue::None,
        }
    }
    fn play_tick(&mut self, input: &Input) -> Cue {
        if input.tertiary {
            return self.retry();
        }
        let track = &TRACKS[self.track];
        if self.clock.is_multiple_of(track.beat_ticks / 2) {
            self.notes_on = track.music(self.clock / (track.beat_ticks / 2));
        }
        let mut cue = Cue::None;
        if input.secondary && self.charge == 100 && !self.encore() {
            self.charge = 0;
            self.encore_until = self.clock + track.beat_ticks * 8;
            self.notice = "Encore: bonus points and protected misses for eight beats.";
            cue = Cue::Action;
        }
        for (lane, pressed) in Self::lane_edges(input).into_iter().enumerate() {
            if pressed {
                let result = self.strike(lane);
                if result == Cue::Bad || cue != Cue::Bad {
                    cue = result;
                }
            }
        }
        let shield = self.encore();
        for index in 0..self.len {
            let note = self.notes[index];
            if note.result == Judgment::Pending && self.clock > note.tick + WINDOW {
                self.notes[index].result = Judgment::Miss;
                self.attempt.misses += 1;
                self.combo = 0;
                if !shield {
                    self.crowd -= 10;
                }
                self.flash(note.lane, Flash::Miss);
                self.notice = if shield {
                    "Encore caught the drop, but accuracy still lost a note."
                } else {
                    "Miss. Read ahead and return to the beat."
                };
                cue = Cue::Bad;
            }
        }
        if self.crowd <= 0 {
            self.crowd = 0;
            return self.fail("The crowd went quiet. Spend a retry ticket for another take.");
        }
        self.clock += 1;
        if self.clock > track.length() + track.beat_ticks * 2 {
            if self.attempt.accuracy() < track.target {
                return self.fail("Song ended below its accuracy target. Try a cleaner take.");
            }
            self.attempt.score += self.attempt.accuracy() * 2;
            self.results[self.track] = Some(self.attempt);
            self.phase = Phase::Results;
            self.stage_ticks = 0;
            self.notice = "District restored. Review your grade, then continue.";
            return Cue::Win;
        }
        cue
    }
    /// Read-only perfect-player command generator, using the same direction edges
    /// and Encore action as a human. It never changes clocks, health or judgments.
    pub fn autoplay(&self, tick: u64) -> Input {
        let mut input = Input::default();
        match self.phase {
            Phase::Briefing | Phase::Results | Phase::Retry => {
                input.primary = tick.is_multiple_of(60);
            }
            Phase::Playing => {
                for note in &self.notes[..self.len] {
                    if note.tick == self.clock && note.result == Judgment::Pending {
                        match note.lane {
                            0 => input.left_pressed = true,
                            1 => input.down_pressed = true,
                            2 => input.up_pressed = true,
                            _ => input.right_pressed = true,
                        }
                    }
                }
                input.secondary = self.charge == 100 && !self.encore();
            }
            _ => {}
        }
        input
    }
    pub(crate) fn average_accuracy(&self) -> u32 {
        let (total, count) = self
            .results
            .iter()
            .flatten()
            .fold((0, 0), |(t, n), s| (t + s.accuracy(), n + 1));
        total.checked_div(count).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane_input(mask: u8) -> Input {
        Input {
            left_pressed: mask & 1 != 0,
            down_pressed: mask & 2 != 0,
            up_pressed: mask & 4 != 0,
            right_pressed: mask & 8 != 0,
            ..Input::default()
        }
    }
    fn probe(notes: &[(u32, usize)], clock: u32) -> Game {
        let mut game = Game::new();
        game.phase = Phase::Playing;
        game.clock = clock;
        game.len = notes.len();
        for (i, &(tick, lane)) in notes.iter().enumerate() {
            game.notes[i] = Note {
                tick,
                lane,
                result: Judgment::Pending,
            };
        }
        game
    }
    fn run_to_results(game: &mut Game) {
        for tick in 0..12_000 {
            let input = game.autoplay(tick);
            game.step(&input);
            if game.phase == Phase::Results {
                return;
            }
            assert_eq!(game.outcome(), Outcome::Playing);
        }
        panic!("song did not finish");
    }
    #[test]
    fn authored_charts_are_bounded_sorted_unique_and_have_valid_music() {
        let mut counts = [0; 6];
        for (track, definition) in TRACKS.iter().enumerate() {
            let mut game = Game::new();
            game.track = track;
            game.load_chart();
            counts[track] = game.len;
            assert!(game.len >= 40 && game.len <= MAX_NOTES);
            assert!(definition.beat_ticks.is_multiple_of(2));
            assert!(
                definition
                    .arrangement
                    .iter()
                    .all(|&p| p < definition.patterns.len())
            );
            assert!(
                game.notes[..game.len]
                    .windows(2)
                    .all(|n| (n[0].tick, n[0].lane) < (n[1].tick, n[1].lane))
            );
            assert!(
                game.notes[..game.len]
                    .iter()
                    .all(|n| n.tick < definition.length() && n.lane < 4)
            );
            for subdivision in 0..definition.bars() * 8 {
                assert!(definition.mask(subdivision) <= 15);
                assert!(
                    definition
                        .music(subdivision)
                        .into_iter()
                        .flatten()
                        .all(|m| (36..=96).contains(&m))
                );
            }
            assert_eq!(definition.music(definition.bars() * 8), [None; 4]);
        }
        assert!(counts.windows(2).all(|n| n[1] >= n[0]));
    }
    #[test]
    fn complete_six_track_concert_is_won_by_legal_edges_and_then_immutable() {
        let mut game = Game::new();
        let mut visited = [false; 6];
        let mut music_events = 0;
        for tick in 0..60_000 {
            visited[game.track] = true;
            let command = game.autoplay(tick);
            game.step(&command);
            music_events += game.music_notes().into_iter().flatten().count();
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert!(visited.into_iter().all(|v| v));
        assert_eq!(game.completed_tracks(), 6);
        assert_eq!(game.retries(), 3);
        for result in game.results.into_iter().flatten() {
            assert_eq!(result.accuracy(), 1000);
            assert_eq!(result.misses, 0);
            assert_eq!(result.stray, 0);
            assert_eq!(result.grade(), "S+");
        }
        assert!(game.score() > 200_000);
        assert!(music_events > 1000);
        let ended = game.clone();
        for _ in 0..100 {
            assert_eq!(
                game.step(&Input {
                    primary: true,
                    secondary: true,
                    tertiary: true,
                    ..lane_input(15)
                }),
                Cue::None
            );
        }
        assert_eq!(game, ended);
    }
    #[test]
    fn timing_window_boundaries_are_inclusive_and_symmetric() {
        for (offset, result) in [
            (-7, Judgment::Okay),
            (-5, Judgment::Okay),
            (-4, Judgment::Good),
            (-3, Judgment::Good),
            (-2, Judgment::Perfect),
            (0, Judgment::Perfect),
            (2, Judgment::Perfect),
            (3, Judgment::Good),
            (4, Judgment::Good),
            (5, Judgment::Okay),
            (7, Judgment::Okay),
        ] {
            let mut game = probe(&[(20, 0)], (20 + offset) as u32);
            game.step(&lane_input(1));
            assert_eq!(game.notes[0].result, result, "offset {offset}");
            assert_eq!(game.attempt.stray, 0);
        }
        let mut early = probe(&[(20, 0)], 12);
        early.step(&lane_input(1));
        assert_eq!(early.notes[0].result, Judgment::Pending);
        assert_eq!(early.attempt.stray, 1);
        let mut late = probe(&[(20, 0)], 28);
        late.step(&lane_input(1));
        assert_eq!(late.notes[0].result, Judgment::Miss);
        assert_eq!(late.attempt.stray, 1);
    }
    #[test]
    fn aliases_and_holds_cannot_double_hit_or_repeat_future_notes() {
        let mut game = probe(&[(20, 0), (32, 0)], 20);
        game.step(&Input {
            left_pressed: true,
            slot: Some(1),
            click: true,
            pointer: ferravex_arcade::vec2(220., 488.),
            ..Input::default()
        });
        assert_eq!(game.attempt.perfect, 1);
        for _ in 0..20 {
            game.step(&Input {
                left: true,
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!(game.attempt.perfect, 1);
        assert_eq!(game.attempt.misses, 1);
    }
    #[test]
    fn overlapping_windows_choose_nearest_pending_note_then_earlier_tie() {
        let mut nearer = probe(&[(20, 0), (32, 0)], 27);
        nearer.step(&lane_input(1));
        assert_eq!(nearer.notes[0].result, Judgment::Pending);
        assert_eq!(nearer.notes[1].result, Judgment::Okay);
        let mut tie = probe(&[(20, 0), (32, 0)], 26);
        tie.step(&lane_input(1));
        assert_eq!(tie.notes[0].result, Judgment::Okay);
        assert_eq!(tie.notes[1].result, Judgment::Pending);
    }
    #[test]
    fn simultaneous_chords_judge_each_lane_once_and_missing_half_is_a_miss() {
        let mut chord = probe(&[(20, 0), (20, 3)], 20);
        chord.step(&lane_input(9));
        assert_eq!((chord.attempt.perfect, chord.combo), (2, 2));
        assert_eq!(chord.crowd, 82);
        let mut partial = probe(&[(20, 0), (20, 3)], 20);
        partial.step(&lane_input(1));
        for _ in 0..8 {
            partial.step(&Input::default());
        }
        assert_eq!((partial.attempt.perfect, partial.attempt.misses), (1, 1));
        assert_eq!(partial.combo, 0);
    }
    #[test]
    fn idle_failure_retries_are_bounded_and_loss_cannot_be_escaped_with_game_inputs() {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        let mut attempts = 1;
        for _ in 0..6000 {
            if game.phase == Phase::Retry {
                attempts += 1;
                game.step(&Input {
                    primary: true,
                    ..Input::default()
                });
            } else {
                game.step(&Input::default());
            }
            if game.outcome() == Outcome::Lost {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Lost);
        assert_eq!(attempts, 4);
        assert_eq!(game.retries(), 0);
        let ended = game.clone();
        game.step(&Input {
            primary: true,
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game, ended);
        assert_eq!(Game::new().retries(), 3);
    }
    #[test]
    fn encore_spends_once_expires_and_does_not_repair_accuracy() {
        let mut game = probe(&[(0, 0), (300, 1)], 0);
        game.charge = 100;
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        let deadline = game.encore_until;
        assert_eq!(deadline, 288);
        assert_eq!(game.charge, 0);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.encore_until, deadline);
        while game.clock <= 8 {
            game.step(&Input::default());
        }
        assert_eq!(game.attempt.misses, 1);
        assert_eq!(game.crowd, START_CROWD);
        assert_eq!(game.accuracy(), 0);
        while game.clock <= 308 {
            game.step(&Input::default());
        }
        assert_eq!(game.attempt.misses, 2);
        assert_eq!(game.crowd, START_CROWD - 10);
        assert!(!game.encore());
    }
    #[test]
    fn invalid_encore_and_out_of_range_slots_do_not_award_or_spend() {
        let mut game = probe(&[(80, 0)], 0);
        game.charge = 99;
        game.step(&Input {
            secondary: true,
            slot: Some(usize::MAX),
            click: true,
            pointer: ferravex_arcade::vec2(f32::NAN, 20.),
            ..Input::default()
        });
        assert_eq!(game.charge, 99);
        assert_eq!(game.encore_until, 0);
        assert_eq!(game.attempt.score, 0);
        assert_eq!(game.attempt.stray, 0);
    }
    #[test]
    fn accuracy_threshold_can_fail_without_losing_the_crowd() {
        for (track, expected) in [(0, Phase::Results), (2, Phase::Retry)] {
            let mut game = Game::new();
            game.track = track;
            game.begin();
            for _ in 0..10_000 {
                let mut mask = 0;
                if game.phase == Phase::Playing {
                    for note in &game.notes[..game.len] {
                        if note.tick + 6 == game.clock {
                            mask |= 1 << note.lane;
                        }
                    }
                }
                game.step(&lane_input(mask));
                if matches!(game.phase, Phase::Results | Phase::Retry) {
                    break;
                }
            }
            assert_eq!(game.phase, expected);
            assert_eq!(game.accuracy(), 600);
            assert_eq!(game.crowd, START_CROWD);
        }
    }
    #[test]
    fn results_do_not_reaward_and_retry_discards_only_current_attempt() {
        let mut game = Game::new();
        run_to_results(&mut game);
        let banked = game.score();
        for _ in 0..300 {
            game.step(&Input {
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!(game.score(), banked);
        assert_eq!(game.completed_tracks(), 1);
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        for tick in 0..250 {
            let input = game.autoplay(tick);
            game.step(&input);
        }
        assert!(game.score() > banked);
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.score(), banked);
        assert_eq!(game.completed_tracks(), 1);
        assert_eq!(game.retries(), 2);
        assert!(
            game.notes[..game.len]
                .iter()
                .all(|n| n.result == Judgment::Pending)
        );
    }
    #[test]
    fn tick_music_is_cleared_between_pulses_and_count_in_does_not_judge_lanes() {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        game.step(&lane_input(15));
        assert_eq!(game.music_notes(), [Some(67), None, None, None]);
        assert_eq!(game.attempt.judged(), 0);
        game.step(&lane_input(15));
        assert_eq!(game.music_notes(), [None; 4]);
        assert_eq!(game.attempt.stray, 0);
        let mut playing = probe(&[(0, 0)], 0);
        playing.step(&lane_input(1));
        assert_eq!(playing.music_notes(), [Some(60), Some(48), Some(67), None]);
        playing.step(&Input::default());
        assert_eq!(playing.music_notes(), [None; 4]);
    }
    #[test]
    fn pointer_hits_include_the_visible_key_bed_and_exclude_the_sidebar() {
        let mut game = probe(&[(20, 3)], 20);
        game.step(&Input {
            click: true,
            pointer: ferravex_arcade::vec2(650., 548.),
            ..Input::default()
        });
        assert_eq!(game.notes[0].result, Judgment::Perfect);
        let mut outside = probe(&[(20, 3)], 20);
        outside.step(&Input {
            click: true,
            pointer: ferravex_arcade::vec2(675., 548.),
            ..Input::default()
        });
        assert_eq!(outside.notes[0].result, Judgment::Pending);
        assert_eq!(outside.attempt.stray, 0);
    }
    #[test]
    fn identical_legal_inputs_produce_identical_complete_state() {
        let mut first = Game::new();
        let mut second = Game::new();
        for tick in 0..2000 {
            let input = first.autoplay(tick);
            assert_eq!(first.step(&input), second.step(&input));
            assert_eq!(first, second);
        }
    }
}
