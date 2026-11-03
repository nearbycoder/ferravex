//! Pure council rules. Time advances only when a petition is decided.
use crate::content::{Petition, petition};
use ferravex_arcade::{Cue, Input, Outcome};

pub(crate) const RESOURCE_NAMES: [&str; 4] = ["Treasury", "Commons", "Guard", "Chapel"];
pub(crate) const LEGACY_NAMES: [&str; 4] = ["Charter", "Iron", "Commerce", "Concordat"];
pub(crate) const ACT_NAMES: [&str; 3] = ["Act 1", "Act 2", "Act 3"];
pub(crate) const ACT_TEXT: [&str; 3] = [
    "Keep all four resources above zero. Forecasts include 2 Treasury upkeep.",
    "Decisions 11-20. Earlier policies affect the available choices.",
    "Decisions 21-30. Finish projects and preserve resources for a higher score.",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Briefing,
    Council,
    Projects,
    Chronicle,
}

/// The four successful settlements and four immediate causes of a fallen reign.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Charter,
    Iron,
    Commerce,
    Concordat,
    Bankruptcy,
    Revolt,
    Mutiny,
    Schism,
}
impl Ending {
    /// A stable, human-readable name for this reign's settlement.
    pub fn name(self) -> &'static str {
        match self {
            Self::Charter => "The Charter Crown",
            Self::Iron => "The Iron Regency",
            Self::Commerce => "The Merchant Commonwealth",
            Self::Concordat => "The Concordat Crown",
            Self::Bankruptcy => "The auctioned crown",
            Self::Revolt => "The open palace gates",
            Self::Mutiny => "The captain's ultimatum",
            Self::Schism => "The silent bells",
        }
    }
    pub(crate) fn story(self) -> &'static str {
        match self {
            Self::Charter => {
                "Your heir inherits a crown bounded by public promises. The council can now speak even when a monarch would rather not listen."
            }
            Self::Iron => {
                "The oath book outlives its first captains. Your heir inherits a disciplined state whose servants know both the power and the burden of an order."
            }
            Self::Commerce => {
                "Contracts bind the ports into a common market. Your heir inherits a wealthy network of houses that will negotiate, rather than merely obey."
            }
            Self::Concordat => {
                "The hospice lamps remain lit. Crown and chapel bind the succession to a public trust, and your heir must answer to obligations older than a coronation."
            }
            Self::Bankruptcy => {
                "No silver remains to honor royal promises. Creditors auction the crown's estates; the council disperses before the next petition."
            }
            Self::Revolt => {
                "The commons refuse another decree. The palace gates open from within, and your heir leaves by the road the petitioners once traveled."
            }
            Self::Mutiny => {
                "The guard withdraws its oath. Vey places an unsigned ultimatum on the council table and asks the crown to leave the room."
            }
            Self::Schism => {
                "The chapel withdraws its blessing and closes its hospices. The bells fall silent; rival claimants gather beneath the empty towers."
            }
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Project {
    pub name: &'static str,
    pub body: &'static str,
    pub cost: i16,
    pub duration: u8,
    pub reward: [i16; 4],
}
pub(crate) const PROJECTS: [Project; 4] = [
    Project {
        name: "The Winter Granary",
        body: "Improves food reserves.",
        cost: 18,
        duration: 3,
        reward: [2, 18, 4, 0],
    },
    Project {
        name: "The Public Archive",
        body: "Improves public records.",
        cost: 16,
        duration: 4,
        reward: [4, 10, 0, 8],
    },
    Project {
        name: "The Muster House",
        body: "Improves guard training.",
        cost: 16,
        duration: 3,
        reward: [3, 2, 18, 0],
    },
    Project {
        name: "The River Exchange",
        body: "Trade investment.",
        cost: 20,
        duration: 5,
        reward: [30, 4, -2, -2],
    },
];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ProjectState {
    #[default]
    Unbuilt,
    Building(u8),
    Complete,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Record {
    pub petition: Petition,
    pub choice: usize,
    pub before: [i16; 4],
    pub after: [i16; 4],
    pub completed: u8,
}
/// Bounded, renderer-independent state for one thirty-council reign.
///
/// ```
/// use crown_council::Game;
/// use ferravex_arcade::{Input, Outcome};
/// let mut reign = Game::new();
/// assert_eq!(reign.decisions(), 0);
/// reign.step(&Input { primary: true, ..Input::default() });
/// reign.step(&Input { slot: Some(1), ..Input::default() });
/// assert_eq!(reign.decisions(), 1);
/// for tick in 0..10_000 {
///     let command = reign.pilot(tick);
///     reign.step(&command);
///     if reign.outcome() != Outcome::Playing { break; }
/// }
/// assert_eq!(reign.outcome(), Outcome::Won);
/// assert_eq!(reign.decisions(), 30);
/// ```
#[derive(Clone, Debug)]
pub struct Game {
    pub(crate) resources: [i16; 4],
    pub(crate) legacy: [u8; 4],
    pub(crate) flags: u16,
    pub(crate) history: [Option<Record>; 30],
    pub(crate) count: usize,
    pub(crate) screen: Screen,
    pub(crate) resume: Screen,
    pub(crate) selected: usize,
    pub(crate) page: usize,
    pub(crate) projects: [ProjectState; 4],
    pub(crate) score: u32,
    outcome: Outcome,
    ending: Option<Ending>,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin at the first act's readable briefing with four reserves of 60.
    pub fn new() -> Self {
        Self {
            resources: [60; 4],
            legacy: [0; 4],
            flags: 0,
            history: [None; 30],
            count: 0,
            screen: Screen::Briefing,
            resume: Screen::Council,
            selected: 0,
            page: 0,
            projects: [ProjectState::Unbuilt; 4],
            score: 0,
            outcome: Outcome::Playing,
            ending: None,
        }
    }
    /// Accepted petitions so far, in `0..=30`.
    pub fn decisions(&self) -> usize {
        self.count
    }
    /// Treasury, Commons, Guard and Chapel, each bounded to `0..=100`.
    pub fn resources(&self) -> [i16; 4] {
        self.resources
    }
    /// Current terminal state. Terminal updates cannot change the reign.
    pub fn outcome(&self) -> Outcome {
        self.outcome
    }
    /// Settlement or cause of collapse, populated only after the reign ends.
    pub fn ending(&self) -> Option<Ending> {
        self.ending
    }
    /// Monotonic score from accepted councils, completed projects and judgment.
    pub fn score(&self) -> u32 {
        self.score
    }
    pub(crate) fn act(&self) -> usize {
        (self.count / 10).min(2)
    }
    pub(crate) fn current(&self) -> Petition {
        petition(self.count, self.flags)
    }
    pub(crate) fn forecast(&self, choice: usize) -> [i16; 4] {
        let mut delta = self.current().choices[choice].delta;
        delta[0] -= 2;
        delta
    }
    pub(crate) fn finished_projects(&self) -> usize {
        self.projects
            .iter()
            .filter(|p| **p == ProjectState::Complete)
            .count()
    }
    pub(crate) fn affordable(&self, index: usize) -> bool {
        self.projects[index] == ProjectState::Unbuilt
            && self.resources[0] > PROJECTS[index].cost
            && self.count + usize::from(PROJECTS[index].duration) <= 30
    }
    fn apply(&mut self, delta: [i16; 4]) {
        for (value, change) in self.resources.iter_mut().zip(delta) {
            *value = (*value + change).clamp(0, 100);
        }
    }
    fn decide(&mut self, index: usize) -> Cue {
        let event = self.current();
        let before = self.resources;
        let choice = event.choices[index];
        self.apply(self.forecast(index));
        self.flags |= choice.flag;
        self.legacy[choice.legacy] += 1;
        let mut completed = 0;
        // Collapse is checked before project payouts: promises cannot spend nonexistent reserves.
        let failed = self.resources.iter().position(|v| *v == 0);
        if failed.is_none() {
            for (i, project) in PROJECTS.iter().enumerate() {
                match self.projects[i] {
                    ProjectState::Building(1) => {
                        self.apply(project.reward);
                        self.projects[i] = ProjectState::Complete;
                        completed |= 1 << i;
                        self.score += 60;
                    }
                    ProjectState::Building(left) => {
                        self.projects[i] = ProjectState::Building(left - 1)
                    }
                    _ => {}
                }
            }
        }
        self.history[self.count] = Some(Record {
            petition: event,
            choice: index,
            before,
            after: self.resources,
            completed,
        });
        self.count += 1;
        self.score += 100 + u32::try_from(*self.resources.iter().min().unwrap()).unwrap_or(0);
        if let Some(index) = failed.or_else(|| self.resources.iter().position(|v| *v == 0)) {
            self.ending = Some(
                [
                    Ending::Bankruptcy,
                    Ending::Revolt,
                    Ending::Mutiny,
                    Ending::Schism,
                ][index],
            );
            self.outcome = Outcome::Lost;
            return Cue::Bad;
        }
        if self.count == 30 {
            let best = (0..4)
                .max_by_key(|&i| (self.legacy[i], std::cmp::Reverse(i)))
                .unwrap();
            self.ending = Some(
                [
                    Ending::Charter,
                    Ending::Iron,
                    Ending::Commerce,
                    Ending::Concordat,
                ][best],
            );
            self.outcome = Outcome::Won;
            self.score += self.resources.iter().map(|v| *v as u32).sum::<u32>() * 3;
            return Cue::Win;
        }
        self.screen = if self.count.is_multiple_of(10) {
            Screen::Briefing
        } else {
            Screen::Council
        };
        self.selected = 0;
        Cue::Good
    }
    fn invest(&mut self, index: usize) -> Cue {
        if !self.affordable(index) {
            return Cue::None;
        }
        self.resources[0] -= PROJECTS[index].cost;
        self.projects[index] = ProjectState::Building(PROJECTS[index].duration);
        Cue::Action
    }
    /// Consume one supplied 60 Hz input tick. Reading screens never spends a turn.
    /// Number keys choose directly; direction edges and Enter select and confirm.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome != Outcome::Playing {
            return Cue::None;
        }
        if input.secondary {
            if self.screen == Screen::Projects {
                self.screen = self.resume;
            } else if self.screen != Screen::Briefing {
                if self.screen != Screen::Chronicle {
                    self.resume = self.screen;
                }
                self.screen = Screen::Projects;
                self.selected = 0;
            } else {
                return Cue::None;
            }
            return Cue::Action;
        }
        if input.tertiary {
            if self.screen == Screen::Chronicle {
                self.screen = self.resume;
            } else if self.count > 0 && self.screen != Screen::Briefing {
                if self.screen != Screen::Projects {
                    self.resume = self.screen;
                }
                self.screen = Screen::Chronicle;
                self.page = self.count - 1;
            } else {
                return Cue::None;
            }
            return Cue::Action;
        }
        match self.screen {
            Screen::Briefing => {
                if input.primary || (input.click && within(input, 42., 483., 588., 54.)) {
                    self.screen = Screen::Council;
                    Cue::Action
                } else {
                    Cue::None
                }
            }
            Screen::Council | Screen::Projects => {
                let length = if self.screen == Screen::Council { 3 } else { 4 };
                if let Some(slot) = input.slot {
                    if (1..=length).contains(&slot) {
                        return if self.screen == Screen::Council {
                            self.decide(slot - 1)
                        } else {
                            self.invest(slot - 1)
                        };
                    }
                    return Cue::None;
                }
                if input.click {
                    let (y, height) = if self.screen == Screen::Council {
                        (303., 76.)
                    } else {
                        (183., 83.)
                    };
                    for i in 0..length {
                        if within(input, 42., y + i as f32 * height, 588., height - 6.) {
                            return if self.screen == Screen::Council {
                                self.decide(i)
                            } else {
                                self.invest(i)
                            };
                        }
                    }
                }
                if input.down_pressed || input.right_pressed {
                    self.selected = (self.selected + 1) % length;
                    return Cue::Action;
                }
                if input.up_pressed || input.left_pressed {
                    self.selected = (self.selected + length - 1) % length;
                    return Cue::Action;
                }
                if input.primary {
                    if self.screen == Screen::Council {
                        self.decide(self.selected)
                    } else {
                        self.invest(self.selected)
                    }
                } else {
                    Cue::None
                }
            }
            Screen::Chronicle => {
                let before = self.page;
                if (input.left_pressed || input.up_pressed) && self.page > 0 {
                    self.page -= 1;
                }
                if (input.right_pressed || input.down_pressed) && self.page + 1 < self.count {
                    self.page += 1;
                }
                if input.primary {
                    self.screen = self.resume;
                    return Cue::Action;
                }
                if before != self.page {
                    Cue::Action
                } else {
                    Cue::None
                }
            }
        }
    }
    pub(crate) fn strategic_choice(&self, preference: Option<usize>) -> usize {
        (0..3)
            .max_by_key(|&index| {
                let mut projected = self.resources;
                for (v, d) in projected.iter_mut().zip(self.forecast(index)) {
                    *v = (*v + d).clamp(0, 100);
                }
                let lowest = i32::from(*projected.iter().min().unwrap());
                let value = projected.iter().map(|v| i32::from(*v)).sum::<i32>() + lowest * 6
                    - projected
                        .iter()
                        .map(|v| (30 - i32::from(*v)).max(0).pow(2) * 3)
                        .sum::<i32>();
                let choice = self.current().choices[index];
                value
                    + if preference == Some(choice.legacy) {
                        35
                    } else {
                        0
                    }
                    + if choice.flag != 0 && self.flags & choice.flag == 0 {
                        8
                    } else {
                        0
                    }
                    - if lowest == 0 { 100_000 } else { 0 }
            })
            .unwrap()
    }
    fn prudent_investment(&self) -> Option<usize> {
        [3, 0, 2, 1].into_iter().find(|&i| {
            self.affordable(i)
                && self.resources[0] > PROJECTS[i].cost + 25
                && self.resources.iter().all(|v| *v > 20)
                && self.count < 23
        })
    }
    /// A read-only adviser returning ordinary, legal player controls every 45 ticks.
    /// It weighs visible reserves and favors new durable promises; no rules are bypassed.
    pub fn pilot(&self, tick: u64) -> Input {
        if !tick.is_multiple_of(45) || self.outcome != Outcome::Playing {
            return Input::default();
        }
        match self.screen {
            Screen::Briefing | Screen::Chronicle => Input {
                primary: true,
                ..Input::default()
            },
            Screen::Projects => {
                if let Some(index) = self.prudent_investment() {
                    Input {
                        slot: Some(index + 1),
                        ..Input::default()
                    }
                } else {
                    Input {
                        secondary: true,
                        ..Input::default()
                    }
                }
            }
            Screen::Council => {
                if self.prudent_investment().is_some() {
                    Input {
                        secondary: true,
                        ..Input::default()
                    }
                } else {
                    Input {
                        slot: Some(self.strategic_choice(None) + 1),
                        ..Input::default()
                    }
                }
            }
        }
    }
    /// Final grade: S requires reserves of 40 and three completed projects;
    /// A requires reserves of 25 and two projects; B requires reserves of 15.
    pub fn grade(&self) -> &'static str {
        if self.outcome == Outcome::Lost {
            return "F";
        }
        if self.outcome == Outcome::Playing {
            return "Pending";
        }
        let reserve = *self.resources.iter().min().unwrap();
        if reserve >= 40 && self.finished_projects() >= 3 {
            "S"
        } else if reserve >= 25 && self.finished_projects() >= 2 {
            "A"
        } else if reserve >= 15 {
            "B"
        } else {
            "C"
        }
    }
    /// Earned replay badges, bounded to three entries, after a successful reign.
    /// Oathkeeper needs five named promises; Builder needs two works;
    /// Four Pillars needs every final reserve at least forty.
    pub fn badges(&self) -> Vec<&'static str> {
        if self.outcome != Outcome::Won {
            return Vec::new();
        }
        let mut badges = Vec::with_capacity(3);
        if self.flags.count_ones() >= 5 {
            badges.push("Oathkeeper");
        }
        if self.finished_projects() >= 2 {
            badges.push("Builder");
        }
        if self.resources.iter().all(|v| *v >= 40) {
            badges.push("Four Pillars");
        }
        badges
    }
    /// Compact chronicle judgment suitable for the shared ending and local score record.
    pub fn summary(&self) -> String {
        if let Some(ending) = self.ending {
            format!(
                "{} | Grade {} | {}/30 councils | {} projects | {} points | Badges: {}. {}",
                ending.name(),
                self.grade(),
                self.count,
                self.finished_projects(),
                self.score,
                if self.badges().is_empty() {
                    "None".into()
                } else {
                    self.badges().join(", ")
                },
                ending.story()
            )
        } else {
            format!(
                "{} of 30 councils; reserves {:?}; {} completed projects.",
                self.count,
                self.resources,
                self.finished_projects()
            )
        }
    }
}
fn within(input: &Input, x: f32, y: f32, w: f32, h: f32) -> bool {
    input.pointer.x >= x
        && input.pointer.x <= x + w
        && input.pointer.y >= y
        && input.pointer.y <= y + h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{LEVEE, PETITIONS, SANCTUARY};
    fn press(game: &mut Game, slot: usize) {
        game.step(&Input {
            slot: Some(slot),
            ..Input::default()
        });
    }
    fn next_council(game: &mut Game) {
        while game.outcome() == Outcome::Playing && game.screen != Screen::Council {
            game.step(&Input {
                primary: true,
                ..Input::default()
            });
        }
    }
    fn policy_reign(style: usize) -> Game {
        let mut game = Game::new();
        for _ in 0..100 {
            if game.outcome() != Outcome::Playing {
                break;
            }
            next_council(&mut game);
            let slot = game.strategic_choice(Some(style)) + 1;
            press(&mut game, slot);
        }
        game
    }
    #[test]
    fn complete_thirty_council_reign_via_legal_pilot() {
        let mut game = Game::new();
        for tick in 0..10_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won, "{}", game.summary());
        assert_eq!(game.count, 30);
        assert!(game.history.iter().all(Option::is_some));
        assert_eq!(game.finished_projects(), 4);
        assert!(game.badges().contains(&"Builder"));
        assert_eq!(game.grade(), "S");
        let snapshot = (game.resources, game.count, game.score, game.ending);
        assert_eq!(
            game.step(&Input {
                primary: true,
                slot: Some(1),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(
            snapshot,
            (game.resources, game.count, game.score, game.ending)
        );
    }
    #[test]
    fn an_institution_building_reign_can_earn_oathkeeper() {
        let mut game = Game::new();
        next_council(&mut game);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        press(&mut game, 4);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        for choice in [1, 1, 1, 1, 3, 1, 1, 1, 1] {
            next_council(&mut game);
            press(&mut game, choice);
        }
        for tick in 0..10_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won, "{}", game.summary());
        assert!(game.badges().contains(&"Oathkeeper"));
    }
    #[test]
    fn different_policies_produce_distinct_successful_settlements() {
        let games: [Game; 4] = std::array::from_fn(policy_reign);
        for g in &games {
            assert_eq!(g.outcome(), Outcome::Won, "{}", g.summary());
        }
        let mut endings = games
            .iter()
            .filter_map(Game::ending)
            .map(|e| e.name())
            .collect::<Vec<_>>();
        endings.sort_unstable();
        endings.dedup();
        assert_eq!(endings.len(), 4, "{endings:?}");
        println!("Successful policy settlements: {endings:?}");
    }
    #[test]
    fn authored_forecasts_include_upkeep_and_clamp_resources() {
        let mut game = Game::new();
        next_council(&mut game);
        let predicted = game.forecast(0);
        press(&mut game, 1);
        assert_eq!(
            game.resources,
            std::array::from_fn(|i| (60 + predicted[i]).clamp(0, 100))
        );
        assert!(game.flags & LEVEE != 0);
        for style in 0..4 {
            let g = policy_reign(style);
            assert!(g.resources.iter().all(|v| (0..=100).contains(v)));
        }
    }
    #[test]
    fn early_promises_change_later_costs_and_named_followups() {
        let mut game = Game::new();
        next_council(&mut game);
        press(&mut game, 1);
        next_council(&mut game);
        press(&mut game, 1);
        next_council(&mut game);
        press(&mut game, 1);
        assert_ne!(game.flags & SANCTUARY, 0);
        assert_eq!(petition(5, game.flags).title, "Anja's first arch");
        assert_eq!(petition(6, game.flags).title, "The gates of Crown's Reach");
        assert!(petition(6, game.flags).choices[0].delta[0] > petition(6, 0).choices[0].delta[0]);
        assert_ne!(petition(24, game.flags).body, petition(24, 0).body);
        while game.count < 28 && game.outcome() == Outcome::Playing {
            next_council(&mut game);
            let choice = game.strategic_choice(None) + 1;
            press(&mut game, choice);
        }
        assert_eq!(game.count, 28);
        for (turn, expected) in [
            (5, "Anja's first arch"),
            (6, "The gates of Crown's Reach"),
            (12, "The Salt Charter comes home"),
            (24, "The Orchard Oath repaid"),
            (27, "The levee's final test"),
        ] {
            assert_eq!(game.history[turn].unwrap().petition.title, expected);
        }
    }
    #[test]
    fn project_cost_is_once_and_payout_is_after_exact_councils() {
        let mut game = Game::new();
        next_council(&mut game);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        press(&mut game, 1);
        assert_eq!(game.resources[0], 42);
        assert_eq!(game.projects[0], ProjectState::Building(3));
        let copy = game.clone();
        press(&mut game, 1);
        assert_eq!(game.resources, copy.resources);
        assert_eq!(game.projects, copy.projects);
        for _ in 0..100 {
            game.step(&Input::default());
        }
        assert_eq!(game.projects[0], ProjectState::Building(3));
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        for turn in 0..3 {
            next_council(&mut game);
            let idx = game.strategic_choice(None);
            let before = game.resources;
            let delta = game.forecast(idx);
            press(&mut game, idx + 1);
            if turn == 2 {
                assert_eq!(
                    game.resources,
                    std::array::from_fn(|i| ((before[i] + delta[i]).clamp(0, 100)
                        + PROJECTS[0].reward[i])
                        .clamp(0, 100))
                );
            }
        }
        assert_eq!(game.projects[0], ProjectState::Complete);
        assert_eq!(game.history[2].unwrap().completed, 1);
    }
    #[test]
    fn invalid_actions_and_held_controls_cannot_farm_progress() {
        let mut game = Game::new();
        assert_eq!(
            game.step(&Input {
                slot: Some(1),
                ..Input::default()
            }),
            Cue::None
        );
        next_council(&mut game);
        for slot in [0, 4, 5, usize::MAX] {
            assert_eq!(
                game.step(&Input {
                    slot: Some(slot),
                    ..Input::default()
                }),
                Cue::None
            );
        }
        for _ in 0..120 {
            assert_eq!(
                game.step(&Input {
                    primary_down: true,
                    down: true,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(game.count, 0);
        assert_eq!(game.resources, [60; 4]);
        assert_eq!(game.score, 0);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        press(&mut game, 4);
        press(&mut game, 1);
        press(&mut game, 2);
        let before = game.resources;
        assert_eq!(
            game.step(&Input {
                slot: Some(3),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.resources, before);
    }
    #[test]
    fn careless_legal_policy_loses_and_terminal_updates_are_inert() {
        let mut game = Game::new();
        while game.outcome() == Outcome::Playing {
            next_council(&mut game);
            press(&mut game, 1);
        }
        assert_eq!(game.outcome(), Outcome::Lost);
        assert!(game.count < 30);
        assert!(game.resources.contains(&0));
        let snapshot = (
            game.resources,
            game.count,
            game.score,
            game.projects,
            game.ending,
        );
        for _ in 0..60 {
            assert_eq!(
                game.step(&Input {
                    primary: true,
                    slot: Some(1),
                    secondary: true,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(
            snapshot,
            (
                game.resources,
                game.count,
                game.score,
                game.projects,
                game.ending
            )
        );
        let fresh = Game::new();
        assert_eq!(fresh.resources, [60; 4]);
        assert_eq!(fresh.count, 0);
        assert!(fresh.history.iter().all(Option::is_none));
    }
    #[test]
    fn chronicle_preserves_branches_and_returns_to_next_decision() {
        let mut game = Game::new();
        next_council(&mut game);
        press(&mut game, 1);
        let resources = game.resources;
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.screen, Screen::Chronicle);
        assert_eq!(game.history[0].unwrap().petition.title, PETITIONS[0].title);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.screen, Screen::Projects);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.screen, Screen::Council);
        assert_eq!(game.resources, resources);
    }
    #[test]
    fn choices_advance_immediately_and_keep_history() {
        let mut game = Game::new();
        next_council(&mut game);
        press(&mut game, 1);
        assert_eq!(game.screen, Screen::Council);
        assert_eq!(game.count, 1);
        let first = game.history[0].unwrap();
        press(&mut game, 2);
        assert_eq!(game.screen, Screen::Council);
        assert_eq!(game.count, 2);
        assert_eq!(game.history[0].unwrap().after, first.after);
        assert_eq!(game.history[1].unwrap().before, first.after);
        while game.count < 10 {
            let slot = game.strategic_choice(None) + 1;
            press(&mut game, slot);
        }
        assert_eq!(game.outcome(), Outcome::Playing);
        assert_eq!(game.screen, Screen::Briefing);
        assert_eq!(game.act(), 1);
    }
    #[test]
    fn all_thirty_authored_petitions_and_chains_are_distinct() {
        for (i, p) in PETITIONS.iter().enumerate() {
            assert!(!PETITIONS[..i].iter().any(|other| other.title == p.title));
            for choice in p.choices {
                assert!(choice.delta.iter().any(|v| *v < 0));
                assert!(choice.delta.iter().any(|v| *v > 0));
            }
        }
        for turn in [5, 6, 12, 14, 16, 18, 20, 24, 27] {
            assert_ne!(petition(turn, 127).title, petition(turn, 0).title);
        }
    }
}
