//! Pure, bounded investigation rules. Waiting never spends credibility or time.
//!
//! ```
//! use paper_alibi::Game;
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! game.step(&Input { primary: true, ..Input::default() });
//! assert_eq!(game.evidence_count(), 1);
//! assert_eq!(game.case_number(), 1);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{CASES, Case};
use ferravex_arcade::{Cue, Input, Outcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Places,
    People,
    Notebook,
    Theory,
}
impl View {
    pub fn index(self) -> usize {
        match self {
            Self::Places => 0,
            Self::People => 1,
            Self::Notebook => 2,
            Self::Theory => 3,
        }
    }
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Places,
            1 => Self::People,
            2 => Self::Notebook,
            _ => Self::Theory,
        }
    }
}

/// One command the last executed tick accepted or politely refused, kept for
/// that tick only so the presentation can answer it where it happened. The
/// rules never read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// The view switched to this tab.
    Tab(View),
    /// A row was selected in this view.
    Select(View, usize),
    /// A theory field (0 who, 1 how, 2 why) cycled to this answer.
    Draft(usize, usize),
    /// A new scene record (0..=2) or interview (3..=5) entered the notebook.
    Recorded(usize),
    /// The record was already collected; nothing changed.
    Repeat(usize),
    /// A collected record was pinned to the board.
    Pinned(usize),
    /// A pinned record was taken off the board.
    Unpinned(usize),
    /// Pinning refused: this record is still missing.
    Missing(usize),
    /// Both pins were cleared.
    Cleared,
    /// The authored contradiction was connected.
    Linked,
    /// The pinned records do not contradict each other.
    NoLink,
    /// The contradiction was already connected.
    AlreadyLinked,
    /// This witness signed a response.
    Confessed(usize),
    /// Confronting refused: no contradiction is connected yet.
    ConfrontLocked(usize),
    /// Confronting refused: the contradiction names a different witness.
    WrongWitness(usize),
    /// This witness has already signed.
    AlreadySigned(usize),
    /// Filing refused: records or the confrontation are missing.
    FileLocked,
    /// The accusation was right and the case closed.
    Solved,
    /// A supported but wrong accusation; the remaining credibility points.
    Retracted(u8),
    /// The connect/confront command has no target in this view.
    NoTarget,
    /// The next dossier opened.
    NextCase,
    /// The final edition was printed.
    Published,
}

/// Bounded per-tick presentation events. Cleared at the start of every
/// executed update and ignored by equality, so identical commands still
/// compare as identical games.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Events {
    list: [Option<Event>; 4],
}
impl Events {
    fn clear(&mut self) {
        self.list = [None; 4];
    }
    fn push(&mut self, event: Event) {
        if let Some(slot) = self.list.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(event);
        }
    }
    /// Events in the order the tick accepted them.
    pub fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.list.iter().flatten().copied()
    }
}
impl PartialEq for Events {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// A six-case campaign with four credibility points and no real-time deadline.
/// All commands go through [`Game::step`]; see the module's executable example.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub(crate) case: usize,
    pub(crate) solved: usize,
    pub(crate) view: View,
    pub(crate) selected: [usize; 4],
    pub(crate) known: [bool; 7],
    pub(crate) pins: [Option<usize>; 2],
    pub(crate) linked: bool,
    pub(crate) confronted: bool,
    pub(crate) theory: [usize; 3],
    pub(crate) credibility: u8,
    pub(crate) mistakes: u8,
    pub(crate) score: u32,
    pub(crate) total_notes: u32,
    pub(crate) debrief: bool,
    pub(crate) events: Events,
    result: Outcome,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Start a fresh investigation; no platform or storage access occurs.
    pub fn new() -> Self {
        Self {
            case: 0,
            solved: 0,
            view: View::Places,
            selected: [0; 4],
            known: [false; 7],
            pins: [None; 2],
            linked: false,
            confronted: false,
            theory: [0; 3],
            credibility: 4,
            mistakes: 0,
            score: 0,
            total_notes: 0,
            debrief: false,
            events: Events::default(),
            result: Outcome::Playing,
        }
    }
    /// Current case number, in 1..=6. The final dossier stays readable on ending.
    pub fn case_number(&self) -> usize {
        self.case + 1
    }
    /// Number of accepted case conclusions, in 0..=6.
    pub fn cases_solved(&self) -> usize {
        self.solved
    }
    /// Discovered records in the current case, including its confrontation.
    pub fn evidence_count(&self) -> usize {
        self.known.iter().filter(|&&v| v).count()
    }
    /// Remaining formal-accusation mistakes before the newspaper loses trust.
    pub fn credibility(&self) -> u8 {
        self.credibility
    }
    /// Monotonic score; repeated evidence and invalid actions award nothing.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// A terminal state remains unchanged until the host starts a fresh model.
    pub fn outcome(&self) -> Outcome {
        self.result
    }
    pub(crate) fn case_data(&self) -> &'static Case {
        &CASES[self.case]
    }
    pub(crate) fn note_title(&self, index: usize) -> &'static str {
        let case = self.case_data();
        match index {
            0..=2 => case.places[index].title,
            3..=5 => case.suspects[index - 3].name,
            _ => "Signed confrontation",
        }
    }
    pub(crate) fn note_text(&self, index: usize) -> &'static str {
        let case = self.case_data();
        match index {
            0..=2 => case.places[index].evidence,
            3..=5 => case.suspects[index - 3].statement,
            _ => case.confession,
        }
    }
    pub(crate) fn can_file(&self) -> bool {
        self.confronted && self.known[..6].iter().all(|&known| known)
    }
    pub(crate) fn connected_pair(&self) -> [usize; 2] {
        [0, 3 + self.case_data().culprit]
    }
    /// Contradictions connected so far, including the current unsolved case.
    pub(crate) fn contradictions(&self) -> usize {
        self.solved + usize::from(self.linked && self.solved <= self.case)
    }
    /// The reporter rank earned by the number of retracted accusations.
    pub(crate) fn rank(&self) -> &'static str {
        match self.mistakes {
            0 => "Editor of Record",
            1 => "Trusted Correspondent",
            _ => "Persistent Reporter",
        }
    }
    fn discover(&mut self, index: usize) -> Cue {
        if self.known[index] {
            self.events.push(Event::Repeat(index));
            return Cue::None;
        }
        self.known[index] = true;
        self.total_notes += 1;
        self.score += 25;
        self.events.push(Event::Recorded(index));
        Cue::Good
    }
    fn pin(&mut self) -> Cue {
        let index = self.selected[View::Notebook.index()];
        if !self.known[index] {
            self.events.push(Event::Missing(index));
            return Cue::None;
        }
        let event = if self.pins[0] == Some(index) {
            self.pins[0] = self.pins[1];
            self.pins[1] = None;
            Event::Unpinned(index)
        } else if self.pins[1] == Some(index) {
            self.pins[1] = None;
            Event::Unpinned(index)
        } else if self.pins[0].is_none() {
            self.pins[0] = Some(index);
            Event::Pinned(index)
        } else if self.pins[1].is_none() {
            self.pins[1] = Some(index);
            Event::Pinned(index)
        } else {
            self.pins = [self.pins[1], Some(index)];
            Event::Pinned(index)
        };
        self.events.push(event);
        Cue::Action
    }
    fn connect(&mut self) -> Cue {
        if self.linked {
            self.events.push(Event::AlreadyLinked);
            return Cue::None;
        }
        let expected = self.connected_pair();
        if self.pins == [Some(expected[0]), Some(expected[1])]
            || self.pins == [Some(expected[1]), Some(expected[0])]
        {
            self.linked = true;
            self.score += 100;
            self.events.push(Event::Linked);
            Cue::Good
        } else {
            self.events.push(Event::NoLink);
            Cue::None
        }
    }
    fn confront(&mut self) -> Cue {
        let witness = self.selected[View::People.index()];
        if self.confronted {
            self.events.push(Event::AlreadySigned(witness));
            return Cue::None;
        }
        if !self.linked {
            self.events.push(Event::ConfrontLocked(witness));
            return Cue::None;
        }
        if witness != self.case_data().culprit {
            self.events.push(Event::WrongWitness(witness));
            return Cue::None;
        }
        self.confronted = true;
        self.known[6] = true;
        self.total_notes += 1;
        self.score += 75;
        self.events.push(Event::Confessed(witness));
        Cue::Good
    }
    fn file(&mut self) -> Cue {
        if !self.can_file() {
            self.events.push(Event::FileLocked);
            return Cue::None;
        }
        let case = self.case_data();
        if self.theory == [case.culprit, case.method, case.motive] {
            self.solved += 1;
            self.debrief = true;
            self.score += 500 + u32::from(self.credibility) * 50;
            self.events.push(Event::Solved);
            Cue::Win
        } else {
            self.credibility -= 1;
            self.mistakes += 1;
            self.events.push(Event::Retracted(self.credibility));
            if self.credibility == 0 {
                self.result = Outcome::Lost;
            }
            Cue::Bad
        }
    }
    fn primary(&mut self) -> Cue {
        match self.view {
            View::Places => self.discover(self.selected[0]),
            View::People => self.discover(3 + self.selected[1]),
            View::Notebook => self.pin(),
            View::Theory => self.file(),
        }
    }
    fn tertiary(&mut self) -> Cue {
        match self.view {
            View::Notebook => self.connect(),
            View::People => self.confront(),
            _ => {
                self.events.push(Event::NoTarget);
                Cue::None
            }
        }
    }
    fn secondary(&mut self) -> Cue {
        if self.view == View::Notebook {
            self.pins = [None; 2];
            self.events.push(Event::Cleared);
        } else {
            self.show(View::Notebook);
        }
        Cue::Action
    }
    fn show(&mut self, view: View) {
        self.view = view;
        self.events.push(Event::Tab(view));
    }
    fn select(&mut self, row: usize) {
        self.selected[self.view.index()] = row;
        self.events.push(Event::Select(self.view, row));
    }
    fn next_case(&mut self) -> Cue {
        if self.solved == CASES.len() {
            self.result = Outcome::Won;
            self.debrief = false;
            self.score += 500;
            self.events.push(Event::Published);
            return Cue::Win;
        }
        self.case += 1;
        self.view = View::Places;
        self.selected = [0; 4];
        self.known = [false; 7];
        self.pins = [None; 2];
        self.linked = false;
        self.confronted = false;
        self.theory = [0; 3];
        self.debrief = false;
        self.events.push(Event::NextCase);
        Cue::Good
    }
    fn pointer(&mut self, input: &Input) -> Cue {
        let x = input.pointer.x;
        let y = input.pointer.y;
        if !x.is_finite() || !y.is_finite() {
            return Cue::None;
        }
        if (112. ..146.).contains(&y) && (24. ..936.).contains(&x) {
            self.show(View::from_index(((x - 24.) / 228.) as usize));
            return Cue::Action;
        }
        if self.view == View::Theory {
            if (352. ..918.).contains(&x) && (229. ..448.).contains(&y) {
                let row = ((y - 229.) / 73.) as usize;
                if row < 3 {
                    self.selected[3] = row;
                    self.theory[row] = (self.theory[row] + 1) % 3;
                    self.events.push(Event::Draft(row, self.theory[row]));
                    return Cue::Action;
                }
            }
        } else if (36. ..312.).contains(&x) {
            let (top, height, count) = if self.view == View::Notebook {
                (180., 43., 7)
            } else {
                (188., 86., 3)
            };
            if y >= top && y < top + height * count as f32 {
                self.select(((y - top) / height) as usize);
                return Cue::Action;
            }
        }
        if (505. ..547.).contains(&y) {
            // The notebook's clear-pins button sits under its record list.
            if self.view == View::Notebook && (36. ..312.).contains(&x) {
                return self.secondary();
            }
            if (350. ..570.).contains(&x)
                || (self.view == View::Theory && (350. ..918.).contains(&x))
            {
                return self.primary();
            }
            if (590. ..918.).contains(&x) {
                return if self.view == View::Places {
                    self.secondary()
                } else {
                    self.tertiary()
                };
            }
        }
        Cue::None
    }
    /// Execute one fixed-tick command. Only pressed edges cause game actions.
    /// Invalid interactions may explain themselves but never spend score/resources.
    pub fn step(&mut self, input: &Input) -> Cue {
        self.events.clear();
        if self.result != Outcome::Playing {
            return Cue::None;
        }
        if self.debrief {
            if input.primary
                || (input.click
                    && (350. ..918.).contains(&input.pointer.x)
                    && (505. ..547.).contains(&input.pointer.y))
            {
                return self.next_case();
            }
            return Cue::None;
        }
        if let Some(slot) = input.slot {
            if (1..=4).contains(&slot) {
                self.show(View::from_index(slot - 1));
                return Cue::Action;
            }
            return Cue::None;
        }
        if input.click {
            return self.pointer(input);
        }
        if input.up_pressed || input.down_pressed {
            let count = if self.view == View::Notebook { 7 } else { 3 };
            let selected = self.selected[self.view.index()];
            self.select((selected + if input.up_pressed { count - 1 } else { 1 }) % count);
            return Cue::Action;
        }
        if self.view == View::Theory && (input.left_pressed || input.right_pressed) {
            let field = self.selected[3];
            let selection = &mut self.theory[field];
            *selection = (*selection + if input.left_pressed { 2 } else { 1 }) % 3;
            self.events.push(Event::Draft(field, *selection));
            return Cue::Action;
        }
        if input.primary {
            return self.primary();
        }
        if input.tertiary {
            return self.tertiary();
        }
        if input.secondary {
            return self.secondary();
        }
        Cue::None
    }
    /// Legal scripted investigator input for reproducible smoke sessions.
    /// It gathers every record before using the authored solution, never mutates
    /// the model, and deliberately pauses between actions so the UI can be seen.
    pub fn autoplay(&self, tick: u64) -> Input {
        if !tick.is_multiple_of(24) || self.result != Outcome::Playing {
            return Input::default();
        }
        let primary = Input {
            primary: true,
            ..Input::default()
        };
        if self.debrief {
            return primary;
        }
        let tab = |i| Input {
            slot: Some(i),
            ..Input::default()
        };
        let down = Input {
            down_pressed: true,
            ..Input::default()
        };
        for index in 0..6 {
            if !self.known[index] {
                let view = if index < 3 {
                    View::Places
                } else {
                    View::People
                };
                let row = index % 3;
                if self.view != view {
                    return tab(view.index() + 1);
                }
                if self.selected[view.index()] != row {
                    return down;
                }
                return primary;
            }
        }
        if !self.linked {
            if self.view != View::Notebook {
                return tab(3);
            }
            for index in self.connected_pair() {
                if !self.pins.contains(&Some(index)) {
                    if self.selected[2] != index {
                        return down;
                    }
                    return primary;
                }
            }
            return Input {
                tertiary: true,
                ..Input::default()
            };
        }
        if !self.confronted {
            if self.view != View::People {
                return tab(2);
            }
            if self.selected[1] != self.case_data().culprit {
                return down;
            }
            return Input {
                tertiary: true,
                ..Input::default()
            };
        }
        if self.view != View::Theory {
            return tab(4);
        }
        let case = self.case_data();
        let goal = [case.culprit, case.method, case.motive];
        for (row, &value) in goal.iter().enumerate() {
            if self.theory[row] != value {
                if self.selected[3] != row {
                    return down;
                }
                return Input {
                    right_pressed: true,
                    ..Input::default()
                };
            }
        }
        primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn action(g: &mut Game, input: Input) {
        g.step(&input);
    }
    fn press(g: &mut Game) {
        action(
            g,
            Input {
                primary: true,
                ..Input::default()
            },
        );
    }
    fn tab(g: &mut Game, slot: usize) {
        action(
            g,
            Input {
                slot: Some(slot),
                ..Input::default()
            },
        );
    }
    fn down(g: &mut Game) {
        action(
            g,
            Input {
                down_pressed: true,
                ..Input::default()
            },
        );
    }
    fn prepare_first_case() -> Game {
        let mut g = Game::new();
        for tick in 0..10_000 {
            if g.can_file() {
                break;
            }
            let input = g.autoplay(tick);
            g.step(&input);
        }
        assert!(g.can_file());
        g
    }
    #[test]
    fn full_six_case_campaign_wins_with_only_legal_player_commands() {
        let mut g = Game::new();
        let mut visited = [false; 6];
        for tick in 0..30_000 {
            visited[g.case] = true;
            let input = g.autoplay(tick);
            g.step(&input);
            if g.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(g.outcome(), Outcome::Won);
        assert_eq!(g.solved, 6);
        assert!(visited.into_iter().all(|v| v));
        assert_eq!(g.total_notes, 42);
        assert_eq!(g.credibility, 4);
        assert_eq!(g.score, 6650);
    }
    #[test]
    fn repeat_evidence_and_held_keys_cannot_farm_score() {
        let mut g = Game::new();
        press(&mut g);
        assert_eq!(g.score, 25);
        for _ in 0..100 {
            press(&mut g);
            action(
                &mut g,
                Input {
                    down: true,
                    primary_down: true,
                    ..Input::default()
                },
            );
        }
        assert_eq!(g.score, 25);
        assert_eq!(g.evidence_count(), 1);
        assert_eq!(g.selected[0], 0);
    }
    #[test]
    fn pin_requires_collected_records_and_duplicate_pin_toggles() {
        let mut g = Game::new();
        tab(&mut g, 3);
        press(&mut g);
        assert_eq!(g.pins, [None; 2]);
        tab(&mut g, 1);
        press(&mut g);
        tab(&mut g, 3);
        press(&mut g);
        assert_eq!(g.pins, [Some(0), None]);
        press(&mut g);
        assert_eq!(g.pins, [None; 2]);
        assert_eq!(g.score, 25);
    }
    #[test]
    fn only_the_authored_contradiction_unlocks_confrontation() {
        let mut g = Game::new();
        press(&mut g);
        down(&mut g);
        press(&mut g);
        tab(&mut g, 3);
        press(&mut g);
        down(&mut g);
        press(&mut g);
        action(
            &mut g,
            Input {
                tertiary: true,
                ..Input::default()
            },
        );
        assert!(!g.linked);
        tab(&mut g, 2);
        action(
            &mut g,
            Input {
                tertiary: true,
                ..Input::default()
            },
        );
        assert!(!g.confronted);
        assert_eq!(g.credibility, 4);
        assert_eq!(g.score, 50);
    }
    #[test]
    fn unsupported_formal_accusation_does_not_consume_credibility() {
        let mut g = Game::new();
        tab(&mut g, 4);
        for _ in 0..8 {
            press(&mut g);
        }
        assert_eq!(g.credibility, 4);
        assert_eq!(g.score, 0);
        assert_eq!(g.solved, 0);
    }
    #[test]
    fn four_wrong_supported_accusations_end_the_campaign() {
        let mut g = prepare_first_case();
        tab(&mut g, 4);
        // All-zero draft disagrees with the first case's culprit, method and motive.
        for remaining in (0..4).rev() {
            press(&mut g);
            assert_eq!(g.credibility, remaining);
        }
        assert_eq!(g.outcome(), Outcome::Lost);
        assert_eq!(g.solved, 0);
        let before = g.clone();
        press(&mut g);
        tab(&mut g, 1);
        assert_eq!(g, before);
        assert_eq!(Game::new().credibility, 4);
    }
    #[test]
    fn a_valid_link_and_confrontation_award_only_once() {
        let mut g = prepare_first_case();
        let score = g.score;
        tab(&mut g, 2);
        action(
            &mut g,
            Input {
                tertiary: true,
                ..Input::default()
            },
        );
        tab(&mut g, 3);
        action(
            &mut g,
            Input {
                tertiary: true,
                ..Input::default()
            },
        );
        assert_eq!(g.score, score);
        assert_eq!(g.total_notes, 7);
    }
    #[test]
    fn debrief_requires_confirmation_and_resets_only_case_records() {
        let mut g = Game::new();
        for tick in 0..10_000 {
            let input = g.autoplay(tick);
            g.step(&input);
            if g.debrief {
                break;
            }
        }
        assert!(g.debrief);
        assert_eq!(g.solved, 1);
        let before = g.clone();
        for _ in 0..100 {
            g.step(&Input::default());
        }
        assert_eq!(g, before);
        press(&mut g);
        assert_eq!(g.case, 1);
        assert_eq!(g.evidence_count(), 0);
        assert!(!g.linked);
        assert_eq!(g.credibility, 4);
        assert_eq!(g.score, before.score);
        assert_eq!(g.total_notes, 7);
    }
    #[test]
    fn pointer_selection_and_action_match_keyboard_commands() {
        let mut g = Game::new();
        action(
            &mut g,
            Input {
                click: true,
                pointer: ferravex_arcade::vec2(60., 287.),
                ..Input::default()
            },
        );
        assert_eq!(g.selected[0], 1);
        action(
            &mut g,
            Input {
                click: true,
                pointer: ferravex_arcade::vec2(400., 520.),
                ..Input::default()
            },
        );
        assert!(g.known[1]);
        assert_eq!(g.score, 25);
        action(
            &mut g,
            Input {
                click: true,
                pointer: ferravex_arcade::vec2(f32::NAN, 520.),
                ..Input::default()
            },
        );
        assert_eq!(g.score, 25);
    }
    #[test]
    fn visible_notebook_and_full_width_file_buttons_execute_their_labels() {
        let mut g = Game::new();
        action(
            &mut g,
            Input {
                click: true,
                pointer: ferravex_arcade::vec2(790., 525.),
                ..Input::default()
            },
        );
        assert_eq!(g.view, View::Notebook);

        let mut g = prepare_first_case();
        tab(&mut g, 4);
        action(
            &mut g,
            Input {
                click: true,
                pointer: ferravex_arcade::vec2(890., 525.),
                ..Input::default()
            },
        );
        assert_eq!(g.credibility(), 3);
        assert!(!g.debrief);
    }
    #[test]
    fn a_correct_suspect_still_needs_the_correct_method_and_motive() {
        let mut g = prepare_first_case();
        tab(&mut g, 4);
        // Set the first case's culprit (2) and method (1) through ordinary inputs.
        for _ in 0..2 {
            action(
                &mut g,
                Input {
                    right_pressed: true,
                    ..Input::default()
                },
            );
        }
        down(&mut g);
        action(
            &mut g,
            Input {
                right_pressed: true,
                ..Input::default()
            },
        );
        press(&mut g);
        assert_eq!(g.credibility(), 3);
        assert_eq!(g.cases_solved(), 0);
        down(&mut g);
        for _ in 0..2 {
            action(
                &mut g,
                Input {
                    right_pressed: true,
                    ..Input::default()
                },
            );
        }
        press(&mut g);
        assert!(g.debrief);
        assert_eq!(g.cases_solved(), 1);
    }
    #[test]
    fn all_authored_cases_have_distinct_answers_and_clear_alibi_evidence() {
        let mut titles = std::collections::BTreeSet::new();
        for case in CASES {
            assert!(titles.insert(case.title));
            assert!(case.culprit < 3 && case.method < 3 && case.motive < 3);
            assert!(case.places.iter().all(|p| !p.evidence.is_empty()));
            assert!(case.suspects.iter().all(|s| !s.statement.is_empty()));
            assert!(
                case.confession.contains(
                    case.suspects[case.culprit]
                        .name
                        .split_whitespace()
                        .next()
                        .unwrap()
                )
            );
        }
        assert_eq!(titles.len(), 6);
    }
    #[test]
    fn events_last_one_tick_and_never_change_equality() {
        let mut g = Game::new();
        press(&mut g);
        assert_eq!(g.events.iter().collect::<Vec<_>>(), [Event::Recorded(0)]);
        let before = g.clone();
        g.step(&Input::default());
        assert_eq!(g.events.iter().count(), 0);
        assert_eq!(g, before, "presentation events are not game state");
        press(&mut g);
        assert_eq!(g.events.iter().collect::<Vec<_>>(), [Event::Repeat(0)]);
        assert_eq!(g.score, 25);
        tab(&mut g, 2);
        down(&mut g);
        assert_eq!(
            g.events.iter().collect::<Vec<_>>(),
            [Event::Select(View::People, 1)]
        );
    }
    #[test]
    fn clicking_clear_under_the_notebook_list_matches_x() {
        let mut g = Game::new();
        press(&mut g);
        tab(&mut g, 2);
        press(&mut g);
        tab(&mut g, 3);
        press(&mut g);
        for _ in 0..3 {
            down(&mut g);
        }
        press(&mut g);
        assert_eq!(g.pins, [Some(0), Some(3)]);
        let mut keyed = g.clone();
        action(
            &mut keyed,
            Input {
                secondary: true,
                ..Input::default()
            },
        );
        let cue = g.step(&Input {
            click: true,
            pointer: ferravex_arcade::vec2(174., 523.),
            ..Input::default()
        });
        assert_eq!(cue, Cue::Action);
        assert_eq!(g.pins, [None; 2]);
        assert_eq!(g, keyed);
        assert_eq!(g.events.iter().collect::<Vec<_>>(), [Event::Cleared]);
        // The same spot does nothing on other tabs.
        tab(&mut g, 1);
        let before = g.clone();
        g.step(&Input {
            click: true,
            pointer: ferravex_arcade::vec2(174., 523.),
            ..Input::default()
        });
        assert_eq!(g, before);
    }
    #[test]
    fn the_contradiction_count_follows_the_campaign() {
        let mut g = prepare_first_case();
        assert_eq!(g.contradictions(), 1);
        assert_eq!(g.rank(), "Editor of Record");
        tab(&mut g, 4);
        press(&mut g);
        assert_eq!(g.rank(), "Trusted Correspondent");
        assert_eq!(g.contradictions(), 1);
    }
    #[test]
    fn won_model_is_immutable_and_final_bonus_is_not_repeatable() {
        let mut g = Game::new();
        for tick in 0..30_000 {
            let input = g.autoplay(tick);
            g.step(&input);
            if g.result != Outcome::Playing {
                break;
            }
        }
        let before = g.clone();
        assert_eq!(g.result, Outcome::Won);
        for _ in 0..20 {
            press(&mut g);
            tab(&mut g, 4);
        }
        assert_eq!(g, before);
    }
}
