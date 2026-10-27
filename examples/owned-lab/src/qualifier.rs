//! The timed qualification drill that follows the four basic drills.
//!
//! Every field here changes only on fixed simulation ticks from accepted
//! offline shots, hits and eliminations, so a replayed input stream always
//! earns the same score and grade. The qualifier never edits the shared FPS
//! match: operators, damage, respawns, peer packets and replay bytes are the
//! same with or without it. Personal bests live outside this state.
use super::*;
use ferravex_engine::play::gameplay::combo::{ChainReset, ScoreChain};

/// Ticks between "drills complete" (or the bell) and the starting whistle.
pub const COUNTDOWN_TICKS: u32 = 3 * ferravex_fps::TICK_RATE;
/// Length of one qualification run.
pub const RUN_TICKS: u32 = 30 * ferravex_fps::TICK_RATE;
/// Ticks the results card stays up after a run; the bell restarts sooner.
pub const DEBRIEF_CARD_TICKS: u32 = 12 * ferravex_fps::TICK_RATE;
/// Brass bell on top of the central cover. Ringing it starts another run;
/// ringing it during a run pays a one-time secret bonus.
pub const BELL: Vec3 = Vec3::new(0., 3.3, -1.);
const BELL_RADIUS: f32 = 0.6;
const BODY_POINTS: u32 = 100;
const HEAD_POINTS: u32 = 250;
const ELIMINATION_BONUS: u32 = 150;
const BELL_BONUS: u32 = 500;
/// Streak window in seconds; a miss also breaks the streak.
const STREAK_WINDOW: f32 = 4.;
const HITS_PER_TIER: u32 = 3;
const MAX_MULTIPLIER: u32 = 4;
/// Minimum score for S, A, B and C. Anything lower is a D.
pub const GRADE_THRESHOLDS: [u32; 4] = [12000, 7500, 4000, 1500];

/// Letter grade for a finished qualification run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    S,
    A,
    B,
    C,
    D,
}

impl Grade {
    /// Grade a final score against [`GRADE_THRESHOLDS`].
    ///
    /// ```
    /// use owned_lab::Grade;
    /// assert_eq!(Grade::from_score(12_500), Grade::S);
    /// assert_eq!(Grade::from_score(4000), Grade::B);
    /// assert_eq!(Grade::from_score(0), Grade::D);
    /// ```
    pub fn from_score(score: u32) -> Self {
        match GRADE_THRESHOLDS.iter().position(|&min| score >= min) {
            Some(0) => Self::S,
            Some(1) => Self::A,
            Some(2) => Self::B,
            Some(3) => Self::C,
            _ => Self::D,
        }
    }
    /// The single-letter label shown on the results card.
    pub fn letter(self) -> &'static str {
        ["S", "A", "B", "C", "D"][self as usize]
    }
    /// A silly rank title for the results card.
    pub fn title(self) -> &'static str {
        [
            "Courtyard Legend",
            "Certified Sharpshooter",
            "Respectable Recruit",
            "Enthusiastic Liability",
            "Danger to Pigeons",
        ][self as usize]
    }
    /// The next better grade and its minimum score, if any.
    pub fn next(self) -> Option<(Self, u32)> {
        match self {
            Self::S => None,
            Self::A => Some((Self::S, GRADE_THRESHOLDS[0])),
            Self::B => Some((Self::A, GRADE_THRESHOLDS[1])),
            Self::C => Some((Self::B, GRADE_THRESHOLDS[2])),
            Self::D => Some((Self::C, GRADE_THRESHOLDS[3])),
        }
    }
}

/// Where the qualifier is in its cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualifierPhase {
    /// The four basic drills are not finished yet.
    Locked,
    /// Ticks left before the run starts.
    Countdown(u32),
    /// Ticks left in the run.
    Running(u32),
    /// Ticks since the last run finished; ring the bell to go again.
    Debrief(u32),
}

/// Final numbers for one finished run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QualifierResult {
    /// Total score, including eliminations, the bell and the accuracy bonus.
    pub score: u32,
    /// Accepted shots fired during the run.
    pub shots: u32,
    /// Confirmed hits.
    pub hits: u32,
    /// Hits that were headshots.
    pub headshots: u32,
    /// Operators eliminated.
    pub eliminations: u32,
    /// Longest hit streak.
    pub best_streak: u32,
    /// `10 * accuracy percent`, already included in `score`.
    pub accuracy_bonus: u32,
    /// Whether the secret bell bonus was claimed.
    pub bell: bool,
}

impl QualifierResult {
    /// Rounded hit percentage; zero when no shot was fired.
    pub fn accuracy(&self) -> u32 {
        (self.hits * 100 + self.shots / 2)
            .checked_div(self.shots)
            .unwrap_or(0)
            .min(100)
    }
    /// The run's letter grade.
    pub fn grade(&self) -> Grade {
        Grade::from_score(self.score)
    }
}

/// Read-only HUD snapshot of the qualifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifierStatus {
    pub phase: QualifierPhase,
    /// Score so far in the current (or last) run.
    pub score: u32,
    /// Current hit streak.
    pub streak: u32,
    /// Multiplier the next hit would use if the streak holds.
    pub multiplier: u32,
    /// Completed runs since the last reset.
    pub runs: u32,
}

/// Accepted offline actions from one fixed tick.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TickActions {
    pub drills_complete: bool,
    /// Player 0's accepted shot this tick, as a traced segment.
    pub shot: Option<(Vec3, Vec3)>,
    /// Player 0's confirmed hit this tick: `Some(headshot)`.
    pub hit: Option<bool>,
    pub eliminations: u32,
}

/// Things that happened this tick, for presentation and quips.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TickOutcome {
    /// The qualifier unlocked or the bell restarted it.
    pub armed: bool,
    pub bell_rung: bool,
    /// Whole seconds left on the countdown, when it ticks over.
    pub countdown: Option<u32>,
    pub started: bool,
    /// Points earned by a hit or bonus this tick, with the multiplier used.
    pub points: Option<(u32, u32)>,
    pub bell_bonus: bool,
    /// The streak reached a milestone (5, 10, 15...).
    pub streak_milestone: Option<u32>,
    /// Consecutive headshot hits, when at least two.
    pub headshot_streak: Option<u32>,
    pub first_headshot: bool,
    /// Misses in a row reached a nudge threshold.
    pub cold: bool,
    pub finished: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Qualifier {
    phase: QualifierPhase,
    chain: ScoreChain,
    current: QualifierResult,
    last: Option<QualifierResult>,
    bonus: u32,
    misses: u32,
    headshot_streak: u32,
    runs: u32,
}

impl Default for Qualifier {
    fn default() -> Self {
        Self {
            phase: QualifierPhase::Locked,
            chain: ScoreChain::new(STREAK_WINDOW, HITS_PER_TIER, MAX_MULTIPLIER)
                .expect("authored streak rules"),
            current: QualifierResult::default(),
            last: None,
            bonus: 0,
            misses: 0,
            headshot_streak: 0,
            runs: 0,
        }
    }
}

/// Whether a traced shot segment passes through the bell.
pub(crate) fn rings_bell(origin: Vec3, end: Vec3) -> bool {
    let segment = end - origin;
    let length = segment.length_squared();
    let t = if length > f32::EPSILON {
        ((BELL - origin).dot(segment) / length).clamp(0., 1.)
    } else {
        0.
    };
    (origin + segment * t).distance(BELL) <= BELL_RADIUS
}

impl Qualifier {
    #[cfg(test)]
    pub fn phase(&self) -> QualifierPhase {
        self.phase
    }
    pub fn last(&self) -> Option<QualifierResult> {
        self.last
    }
    #[cfg(test)]
    pub fn current(&self) -> QualifierResult {
        self.current
    }
    pub fn status(&self) -> QualifierStatus {
        QualifierStatus {
            phase: self.phase,
            score: self.current.score,
            streak: self.chain.chain(),
            multiplier: self.chain.multiplier(),
            runs: self.runs,
        }
    }
    /// The bell is visible and live once the basic drills are complete.
    pub fn bell_active(&self) -> bool {
        self.phase != QualifierPhase::Locked
    }
    fn arm(&mut self) {
        self.phase = QualifierPhase::Countdown(COUNTDOWN_TICKS);
        self.chain.reset(ChainReset::All);
        self.current = QualifierResult::default();
        self.bonus = 0;
        self.misses = 0;
        self.headshot_streak = 0;
    }
    fn total(&self) -> u32 {
        u32::try_from(self.chain.total())
            .unwrap_or(u32::MAX)
            .saturating_add(self.bonus)
    }
    pub fn tick(&mut self, actions: TickActions, dt: f32) -> TickOutcome {
        let mut out = TickOutcome::default();
        match self.phase {
            QualifierPhase::Locked => {
                if actions.drills_complete {
                    self.arm();
                    out.armed = true;
                }
            }
            QualifierPhase::Countdown(left) => {
                let left = left.saturating_sub(1);
                if left == 0 {
                    self.phase = QualifierPhase::Running(RUN_TICKS);
                    out.started = true;
                } else {
                    self.phase = QualifierPhase::Countdown(left);
                    if left.is_multiple_of(ferravex_fps::TICK_RATE) {
                        out.countdown = Some(left / ferravex_fps::TICK_RATE);
                    }
                }
            }
            QualifierPhase::Running(left) => {
                self.run_tick(actions, dt, &mut out);
                let left = left.saturating_sub(1);
                if left == 0 {
                    self.finish();
                    out.finished = true;
                } else {
                    self.phase = QualifierPhase::Running(left);
                }
            }
            QualifierPhase::Debrief(since) => {
                if actions
                    .shot
                    .is_some_and(|(origin, end)| rings_bell(origin, end))
                {
                    self.arm();
                    out.armed = true;
                    out.bell_rung = true;
                } else {
                    self.phase = QualifierPhase::Debrief(since.saturating_add(1));
                }
            }
        }
        out
    }
    fn run_tick(&mut self, actions: TickActions, dt: f32, out: &mut TickOutcome) {
        if let Some((origin, end)) = actions.shot {
            self.current.shots += 1;
            if !self.current.bell && rings_bell(origin, end) {
                self.current.bell = true;
                self.bonus += BELL_BONUS;
                out.bell_bonus = true;
                out.bell_rung = true;
                out.points = Some((BELL_BONUS, 1));
            }
        }
        match actions.hit {
            Some(headshot) => {
                let award = self
                    .chain
                    .award_with_receipt(if headshot { HEAD_POINTS } else { BODY_POINTS })
                    .expect("nonzero hit points");
                self.current.hits += 1;
                self.misses = 0;
                if headshot {
                    self.current.headshots += 1;
                    self.headshot_streak += 1;
                    out.first_headshot = self.current.headshots == 1;
                    if self.headshot_streak >= 2 {
                        out.headshot_streak = Some(self.headshot_streak);
                    }
                } else {
                    self.headshot_streak = 0;
                }
                self.current.best_streak = self.chain.best();
                if award.chain >= 5 && award.chain.is_multiple_of(5) {
                    out.streak_milestone = Some(award.chain);
                }
                out.points = Some((award.nominal_points as u32, award.multiplier));
            }
            None if actions.shot.is_some() && !out.bell_bonus => {
                self.chain.break_chain();
                self.headshot_streak = 0;
                self.misses += 1;
                out.cold = self.misses == 4 || (self.misses > 4 && self.misses.is_multiple_of(6));
            }
            None => {}
        }
        if actions.hit.is_none() {
            let _ = self.chain.advance(dt);
        }
        if actions.eliminations > 0 {
            self.current.eliminations += actions.eliminations;
            self.bonus += ELIMINATION_BONUS * actions.eliminations;
        }
        self.current.score = self.total();
    }
    fn finish(&mut self) {
        let accuracy_bonus = self.current.accuracy() * 10;
        self.current.accuracy_bonus = accuracy_bonus;
        self.bonus += accuracy_bonus;
        self.current.score = self.total();
        self.current.best_streak = self.chain.best();
        self.last = Some(self.current);
        self.runs = self.runs.saturating_add(1);
        self.phase = QualifierPhase::Debrief(0);
    }
}

/// Personal bests outside the deterministic simulation. Browser builds persist
/// them through the shared record store; native and test builds keep them for
/// the session so local validation never writes a player's records.
#[derive(Default)]
pub(crate) struct Bests {
    best: Option<u32>,
    #[cfg(target_arch = "wasm32")]
    records: Option<ferravex_engine::render::storage::Records>,
}

#[cfg(target_arch = "wasm32")]
const RECORD_ID: &str = "qualifier";
#[cfg(target_arch = "wasm32")]
const RECORD_RULES: u64 = 1;

impl Bests {
    #[cfg(target_arch = "wasm32")]
    pub fn open() -> Self {
        let records = ferravex_engine::render::storage::Records::new("owned-lab").ok();
        let best = records
            .as_ref()
            .and_then(|r| r.get(RECORD_ID, RECORD_RULES))
            .filter(|r| r.runs > 0)
            .map(|r| u32::try_from(r.score).unwrap_or(u32::MAX));
        Self { best, records }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open() -> Self {
        Self::default()
    }
    pub fn best(&self) -> Option<u32> {
        self.best
    }
    /// Record one finished run and return the best before it.
    pub fn record(&mut self, score: u32) -> Option<u32> {
        let previous = self.best;
        self.best = Some(previous.map_or(score, |best| best.max(score)));
        #[cfg(target_arch = "wasm32")]
        if let Some(records) = &mut self.records {
            // A storage failure still keeps the session best.
            let _ = records.attempt(RECORD_ID, RECORD_RULES, u64::from(score), None);
        }
        previous
    }
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    pub fn pretend(&mut self, best: Option<u32>) {
        self.best = best;
    }
}

impl Lab {
    /// Live qualifier phase, score and streak for HUDs and tests.
    ///
    /// ```
    /// use owned_lab::QualifierPhase;
    /// let lab = owned_lab::Lab::new_interactive();
    /// let status = lab.qualifier_status();
    /// assert_eq!(status.phase, QualifierPhase::Locked);
    /// assert_eq!((status.score, status.streak, status.multiplier), (0, 0, 1));
    /// ```
    pub fn qualifier_status(&self) -> QualifierStatus {
        self.qualifier.status()
    }
    /// Results of the most recent finished qualification run, if any.
    ///
    /// ```
    /// assert!(owned_lab::Lab::new_interactive().qualifier_result().is_none());
    /// ```
    pub fn qualifier_result(&self) -> Option<QualifierResult> {
        self.qualifier.last()
    }
    /// Best qualifier score this session (persisted per browser profile).
    /// Reset keeps it; it is never part of replay or peer state.
    ///
    /// ```
    /// assert_eq!(owned_lab::Lab::new_interactive().qualifier_best(), None);
    /// ```
    pub fn qualifier_best(&self) -> Option<u32> {
        self.bests.best()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot_at(target: Vec3) -> Option<(Vec3, Vec3)> {
        Some((vec3(0., 1.6, 20.), target))
    }

    fn running() -> Qualifier {
        let mut q = Qualifier::default();
        assert!(
            q.tick(
                TickActions {
                    drills_complete: true,
                    ..Default::default()
                },
                1. / 64.
            )
            .armed
        );
        let mut started = false;
        let mut counts = Vec::new();
        for _ in 0..COUNTDOWN_TICKS {
            let out = q.tick(TickActions::default(), 1. / 64.);
            started |= out.started;
            counts.extend(out.countdown);
        }
        assert!(started);
        assert_eq!(counts, [2, 1]);
        assert_eq!(q.phase(), QualifierPhase::Running(RUN_TICKS));
        q
    }

    #[test]
    fn grades_titles_and_next_goals_cover_every_score() {
        assert_eq!(Grade::from_score(12000), Grade::S);
        assert_eq!(Grade::from_score(11999), Grade::A);
        assert_eq!(Grade::from_score(1499), Grade::D);
        assert_eq!(Grade::S.next(), None);
        assert_eq!(Grade::D.next(), Some((Grade::C, 1500)));
        for grade in [Grade::S, Grade::A, Grade::B, Grade::C, Grade::D] {
            assert!(!grade.title().is_empty());
            assert_eq!(grade.letter().len(), 1);
        }
    }

    #[test]
    fn streak_multiplier_misses_bell_bonus_and_accuracy_score_a_run() {
        let mut q = running();
        let hit = |headshot| TickActions {
            shot: shot_at(vec3(5., 1., 0.)),
            hit: Some(headshot),
            ..Default::default()
        };
        // Three body hits at x1, then the fourth hit reaches x2.
        for _ in 0..3 {
            assert_eq!(q.tick(hit(false), 1. / 64.).points, Some((100, 1)));
        }
        let out = q.tick(hit(true), 1. / 64.);
        assert_eq!(out.points, Some((500, 2)));
        assert!(out.first_headshot);
        assert_eq!(q.status().streak, 4);
        // A miss breaks the streak; the bell pays once and is not a miss.
        let miss = q.tick(
            TickActions {
                shot: shot_at(vec3(-20., 1., -26.)),
                ..Default::default()
            },
            1. / 64.,
        );
        assert_eq!(miss.points, None);
        assert_eq!(q.status().streak, 0);
        let bell = q.tick(
            TickActions {
                shot: Some((vec3(0., 1.62, 22.), BELL + (BELL - vec3(0., 1.62, 22.)))),
                ..Default::default()
            },
            1. / 64.,
        );
        assert!(bell.bell_bonus);
        assert!(
            !q.tick(
                TickActions {
                    shot: Some((vec3(0., 1.62, 22.), BELL)),
                    ..Default::default()
                },
                1. / 64.
            )
            .bell_bonus,
            "the secret pays once per run"
        );
        q.tick(
            TickActions {
                eliminations: 1,
                ..Default::default()
            },
            1. / 64.,
        );
        assert_eq!(q.status().score, 300 + 500 + 500 + 150);
        let mut finished = false;
        while !finished {
            finished = q.tick(TickActions::default(), 1. / 64.).finished;
        }
        let result = q.last().unwrap();
        assert_eq!((result.shots, result.hits, result.headshots), (7, 4, 1));
        assert_eq!(result.accuracy(), 57);
        assert_eq!(result.accuracy_bonus, 570);
        assert_eq!(result.score, 300 + 500 + 500 + 150 + 570);
        assert_eq!(result.best_streak, 4);
        assert!(result.bell);
        assert_eq!(q.phase(), QualifierPhase::Debrief(0));
    }

    #[test]
    fn streak_expires_without_hits_and_milestones_fire_every_five() {
        let mut q = running();
        let hit = TickActions {
            shot: shot_at(vec3(5., 1., 0.)),
            hit: Some(false),
            ..Default::default()
        };
        let mut milestones = Vec::new();
        for _ in 0..10 {
            milestones.extend(q.tick(hit, 1. / 64.).streak_milestone);
        }
        assert_eq!(milestones, [5, 10]);
        assert_eq!(q.status().multiplier, 4);
        for _ in 0..(STREAK_WINDOW * 64.) as u32 + 1 {
            q.tick(TickActions::default(), 1. / 64.);
        }
        assert_eq!(q.status().streak, 0, "streak window expired");
        assert_eq!(q.current().best_streak, 10);
    }

    #[test]
    fn debrief_restarts_only_from_the_bell_and_never_before_drills() {
        let mut q = Qualifier::default();
        for _ in 0..10 {
            q.tick(
                TickActions {
                    shot: Some((vec3(0., 1.62, 22.), BELL)),
                    ..Default::default()
                },
                1. / 64.,
            );
        }
        assert_eq!(
            q.phase(),
            QualifierPhase::Locked,
            "bell is inert until drills"
        );
        let mut q = running();
        while !q.tick(TickActions::default(), 1. / 64.).finished {}
        let result = q.last().unwrap();
        assert_eq!(result.grade(), Grade::D);
        assert_eq!(result.accuracy(), 0);
        for _ in 0..5 {
            q.tick(
                TickActions {
                    shot: shot_at(vec3(20., 1., 0.)),
                    ..Default::default()
                },
                1. / 64.,
            );
        }
        assert_eq!(q.phase(), QualifierPhase::Debrief(5));
        let out = q.tick(
            TickActions {
                shot: Some((vec3(0., 1.62, 22.), BELL)),
                ..Default::default()
            },
            1. / 64.,
        );
        assert!(out.armed && out.bell_rung);
        assert_eq!(q.phase(), QualifierPhase::Countdown(COUNTDOWN_TICKS));
        assert_eq!(q.status().score, 0);
        assert_eq!(q.last(), Some(result), "the previous card stays available");
        assert_eq!(q.status().runs, 1);
    }

    #[test]
    fn bell_segment_test_uses_the_traced_shot() {
        let eye = vec3(0., 1.62, 22.);
        assert!(rings_bell(eye, eye + (BELL - eye) * 3.));
        assert!(!rings_bell(eye, eye + (BELL - eye) * 0.5), "stopped short");
        assert!(!rings_bell(eye, vec3(4., 3.3, -1.)));
        assert!(
            rings_bell(eye, eye + (BELL + vec3(0.5, 0., 0.) - eye) * 2.),
            "generous rim"
        );
    }

    #[test]
    fn bests_track_previous_and_keep_the_maximum() {
        let mut bests = Bests::open();
        assert_eq!(bests.record(2000), None);
        assert_eq!(bests.record(1500), Some(2000));
        assert_eq!(bests.best(), Some(2000));
        assert_eq!(bests.record(2600), Some(2000));
        assert_eq!(bests.best(), Some(2600));
    }
}
