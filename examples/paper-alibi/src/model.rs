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
    pub(crate) notice: &'static str,
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
            notice: "Select a location, then examine it with Space.",
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
    fn discover(&mut self, index: usize) -> Cue {
        if self.known[index] {
            self.notice = "Already recorded. Read it again; no evidence is lost.";
            return Cue::None;
        }
        self.known[index] = true;
        self.total_notes += 1;
        self.score += 25;
        self.notice = if index < 3 {
            "Physical evidence added to Notebook [3]."
        } else {
            "Testimony added to Notebook [3]. Compare words with facts."
        };
        Cue::Good
    }
    fn pin(&mut self) -> Cue {
        let index = self.selected[View::Notebook.index()];
        if !self.known[index] {
            self.notice = "That record is still missing. Visit its location or witness.";
            return Cue::None;
        }
        if self.pins[0] == Some(index) {
            self.pins[0] = self.pins[1];
            self.pins[1] = None;
        } else if self.pins[1] == Some(index) {
            self.pins[1] = None;
        } else if self.pins[0].is_none() {
            self.pins[0] = Some(index);
        } else if self.pins[1].is_none() {
            self.pins[1] = Some(index);
        } else {
            self.pins = [self.pins[1], Some(index)];
        }
        self.notice = "Pin two conflicting records, then press C to connect them.";
        Cue::Action
    }
    fn connect(&mut self) -> Cue {
        if self.linked {
            self.notice = "The contradiction is established. Confront its witness in People [2].";
            return Cue::None;
        }
        let expected = self.connected_pair();
        if self.pins == [Some(expected[0]), Some(expected[1])]
            || self.pins == [Some(expected[1]), Some(expected[0])]
        {
            self.linked = true;
            self.score += 100;
            self.notice =
                "A statement contradicts physical evidence. Confront that witness with C.";
            Cue::Good
        } else {
            self.notice =
                "No direct contradiction. Compare a verified object with a witness's claim.";
            Cue::None
        }
    }
    fn confront(&mut self) -> Cue {
        if self.confronted {
            self.notice = "The signed response is in Notebook. Complete your theory.";
            return Cue::None;
        }
        if !self.linked {
            self.notice = "First connect contradictory records in Notebook [3].";
            return Cue::None;
        }
        if self.selected[View::People.index()] != self.case_data().culprit {
            self.notice = "Your established contradiction belongs to a different witness.";
            return Cue::None;
        }
        self.confronted = true;
        self.known[6] = true;
        self.total_notes += 1;
        self.score += 75;
        self.notice = "Signed response recorded. Read it, then assemble Theory [4].";
        Cue::Good
    }
    fn file(&mut self) -> Cue {
        if !self.can_file() {
            self.notice = "Need all three scene records, three interviews, and a confrontation.";
            return Cue::None;
        }
        let case = self.case_data();
        if self.theory == [case.culprit, case.method, case.motive] {
            self.solved += 1;
            self.debrief = true;
            self.score += 500 + u32::from(self.credibility) * 50;
            self.notice = "Case closed. Read the result, then press Space to continue.";
            Cue::Win
        } else {
            self.credibility -= 1;
            self.mistakes += 1;
            self.notice = "The accusation does not fit the record. One credibility point lost.";
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
                self.notice = "C connects records in Notebook, or confronts a witness in People.";
                Cue::None
            }
        }
    }
    fn secondary(&mut self) -> Cue {
        if self.view == View::Notebook {
            self.pins = [None; 2];
            self.notice = "Pins cleared. Collected records are safe.";
        } else {
            self.view = View::Notebook;
        }
        Cue::Action
    }
    fn next_case(&mut self) -> Cue {
        if self.solved == CASES.len() {
            self.result = Outcome::Won;
            self.debrief = false;
            self.score += 500;
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
        self.notice = "A new case. Examine its locations and interview all witnesses.";
        Cue::Good
    }
    fn pointer(&mut self, input: &Input) -> Cue {
        let x = input.pointer.x;
        let y = input.pointer.y;
        if !x.is_finite() || !y.is_finite() {
            return Cue::None;
        }
        if (112. ..146.).contains(&y) && (24. ..936.).contains(&x) {
            self.view = View::from_index(((x - 24.) / 228.) as usize);
            return Cue::Action;
        }
        if self.view == View::Theory {
            if (352. ..918.).contains(&x) && (229. ..448.).contains(&y) {
                let row = ((y - 229.) / 73.) as usize;
                if row < 3 {
                    self.selected[3] = row;
                    self.theory[row] = (self.theory[row] + 1) % 3;
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
                self.selected[self.view.index()] = ((y - top) / height) as usize;
                return Cue::Action;
            }
        }
        if (505. ..547.).contains(&y) {
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
                self.view = View::from_index(slot - 1);
                return Cue::Action;
            }
            return Cue::None;
        }
        if input.click {
            return self.pointer(input);
        }
        if input.up_pressed || input.down_pressed {
            let count = if self.view == View::Notebook { 7 } else { 3 };
            let selected = &mut self.selected[self.view.index()];
            *selected = (*selected + if input.up_pressed { count - 1 } else { 1 }) % count;
            return Cue::Action;
        }
        if self.view == View::Theory && (input.left_pressed || input.right_pressed) {
            let selection = &mut self.theory[self.selected[3]];
            *selection = (*selection + if input.left_pressed { 2 } else { 1 }) % 3;
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
