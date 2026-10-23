use super::{AiError, WorkBudget};
use ferravex_math::Vec3;

pub const MAX_MEMORIES: usize = 256;
pub const MAX_NOISES: usize = 256;
/// True means the host's authoritative static/dynamic world permits sight.
/// Implementations should ignore the observer/target bodies and enforce layers.
pub trait VisibilityQuery {
    fn clear(&mut self, from: Vec3, to: Vec3) -> bool;
}
pub struct Unoccluded;
impl VisibilityQuery for Unoccluded {
    fn clear(&mut self, _: Vec3, _: Vec3) -> bool {
        true
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Observer {
    pub position: Vec3,
    pub forward: Vec3,
}
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub id: u64,
    pub position: Vec3,
    pub groups: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detection {
    Detected,
    Rejected,
    Deferred,
}
#[derive(Clone, Copy, Debug)]
pub struct Sight {
    range: f32,
    cosine: f64,
    mask: u32,
}
impl Sight {
    /// Full field of view, radians in 0..=TAU. Distance/cone edges are inclusive.
    pub fn new(range: f32, field_of_view: f32, mask: u32) -> Result<Self, AiError> {
        if !range.is_finite()
            || range <= 0.
            || !field_of_view.is_finite()
            || !(0. ..=std::f32::consts::TAU).contains(&field_of_view)
        {
            return Err(AiError::InvalidConfiguration);
        }
        let cosine = if field_of_view == std::f32::consts::TAU {
            -1.
        } else {
            (f64::from(field_of_view) * 0.5).cos()
        };
        Ok(Self {
            range,
            cosine,
            mask,
        })
    }
    pub fn test(
        &self,
        observer: Observer,
        target: Candidate,
        budget: &mut WorkBudget,
        query: &mut impl VisibilityQuery,
    ) -> Result<Detection, AiError> {
        if !observer.position.is_finite()
            || !observer.forward.is_finite()
            || !target.position.is_finite()
        {
            return Err(AiError::InvalidInput);
        }
        let forward = observer.forward.to_array().map(f64::from);
        let forward_length = forward.into_iter().map(|v| v * v).sum::<f64>().sqrt();
        if forward_length == 0. {
            return Err(AiError::InvalidInput);
        }
        if !budget.candidate() {
            return Ok(Detection::Deferred);
        }
        if target.groups & self.mask == 0 {
            return Ok(Detection::Rejected);
        }
        let (delta, distance) = delta_distance(observer.position, target.position);
        if distance > f64::from(self.range) {
            return Ok(Detection::Rejected);
        }
        if distance > 0.
            && delta
                .into_iter()
                .zip(forward)
                .map(|(a, b)| a * b)
                .sum::<f64>()
                / (distance * forward_length)
                + 1e-7
                < self.cosine
        {
            return Ok(Detection::Rejected);
        }
        if distance == 0. {
            return Ok(Detection::Detected);
        }
        if !budget.query() {
            return Ok(Detection::Deferred);
        }
        Ok(if query.clear(observer.position, target.position) {
            Detection::Detected
        } else {
            Detection::Rejected
        })
    }
}
fn delta_distance(from: Vec3, to: Vec3) -> ([f64; 3], f64) {
    let a = from.to_array();
    let b = to.to_array();
    let delta = std::array::from_fn(|i| f64::from(b[i]) - f64::from(a[i]));
    (delta, delta.into_iter().map(|v| v * v).sum::<f64>().sqrt())
}
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    pub source: u64,
    pub position: Vec3,
    pub groups: u32,
    pub radius: f32,
}
impl Noise {
    fn valid(self) -> bool {
        self.position.is_finite() && self.radius.is_finite() && self.radius > 0.
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Hearing {
    range: f32,
    mask: u32,
    blocked_scale: f32,
}
impl Hearing {
    /// Occluded sound radius is scaled by blocked_scale. None as query in test
    /// disables occlusion; this is an explicit host choice, not a failed query.
    pub fn new(range: f32, mask: u32, blocked_scale: f32) -> Result<Self, AiError> {
        if !range.is_finite()
            || range <= 0.
            || !blocked_scale.is_finite()
            || !(0. ..=1.).contains(&blocked_scale)
        {
            return Err(AiError::InvalidConfiguration);
        }
        Ok(Self {
            range,
            mask,
            blocked_scale,
        })
    }
    pub fn test(
        &self,
        position: Vec3,
        noise: Noise,
        budget: &mut WorkBudget,
        query: Option<&mut dyn VisibilityQuery>,
    ) -> Result<Detection, AiError> {
        if !position.is_finite() || !noise.valid() {
            return Err(AiError::InvalidInput);
        }
        if !budget.candidate() {
            return Ok(Detection::Deferred);
        }
        let (_, distance) = delta_distance(position, noise.position);
        let radius = f64::from(self.range.min(noise.radius));
        if noise.groups & self.mask == 0 || distance > radius {
            return Ok(Detection::Rejected);
        }
        if let Some(query) = query {
            if !budget.query() {
                return Ok(Detection::Deferred);
            }
            if !query.clear(position, noise.position)
                && distance > radius * f64::from(self.blocked_scale)
            {
                return Ok(Detection::Rejected);
            }
        }
        Ok(Detection::Detected)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct MemorySettings {
    pub capacity: usize,
    pub sight_seconds: f32,
    pub hearing_seconds: f32,
}
impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            capacity: 8,
            sight_seconds: 2.,
            hearing_seconds: 3.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    Sight,
    Hearing,
}
#[derive(Clone, Copy, Debug)]
pub struct Track {
    pub id: u64,
    pub position: Vec3,
    pub groups: u32,
    pub age: f64,
    pub visible: bool,
    pub sense: Sense,
    remaining: f64,
}
impl Track {
    pub fn remaining(&self) -> f64 {
        self.remaining
    }
}
/// Fixed-capacity memory. Current-tick sight always takes precedence over hearing.
/// Older hearing can update a remembered position; it never grants current sight.
pub struct Memory {
    settings: MemorySettings,
    tracks: Vec<Track>,
    dropped: u64,
}
impl Memory {
    pub fn new(settings: MemorySettings) -> Result<Self, AiError> {
        if settings.capacity > MAX_MEMORIES
            || ![settings.sight_seconds, settings.hearing_seconds]
                .into_iter()
                .all(|v| v.is_finite() && (0.0001..=3600.).contains(&v))
        {
            return Err(AiError::InvalidConfiguration);
        }
        Ok(Self {
            settings,
            tracks: Vec::with_capacity(settings.capacity),
            dropped: 0,
        })
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), AiError> {
        if !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(AiError::InvalidInput);
        }
        for t in &mut self.tracks {
            t.age += f64::from(dt);
            t.remaining -= f64::from(dt);
            t.visible = false;
        }
        self.tracks.retain(|t| t.remaining > 1e-9);
        Ok(())
    }
    pub fn observe(&mut self, target: Candidate, sense: Sense) -> Result<bool, AiError> {
        if !target.position.is_finite() {
            return Err(AiError::InvalidInput);
        }
        if let Some(t) = self.tracks.iter_mut().find(|t| t.id == target.id) {
            if sense == Sense::Hearing && t.visible {
                return Ok(true);
            }
            *t = Self::track(target, sense, self.settings);
            return Ok(true);
        }
        if self.tracks.len() == self.settings.capacity {
            self.dropped = self.dropped.saturating_add(1);
            return Ok(false);
        }
        self.tracks.push(Self::track(target, sense, self.settings));
        Ok(true)
    }
    fn track(target: Candidate, sense: Sense, s: MemorySettings) -> Track {
        Track {
            id: target.id,
            position: target.position,
            groups: target.groups,
            age: 0.,
            visible: sense == Sense::Sight,
            sense,
            remaining: f64::from(if sense == Sense::Sight {
                s.sight_seconds
            } else {
                s.hearing_seconds
            }),
        }
    }
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }
    pub fn get(&self, id: u64) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }
    pub fn forget(&mut self, id: u64) {
        self.tracks.retain(|t| t.id != id);
    }
    pub fn clear(&mut self) {
        self.tracks.clear();
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    /// Deterministic: fresh sight first, youngest stimulus, then lowest host ID.
    pub fn best(&self, mask: u32) -> Option<&Track> {
        self.tracks
            .iter()
            .filter(|t| t.groups & mask != 0)
            .min_by(|a, b| {
                b.visible
                    .cmp(&a.visible)
                    .then_with(|| a.age.total_cmp(&b.age))
                    .then_with(|| a.id.cmp(&b.id))
            })
    }
}
#[derive(Clone, Copy)]
struct TimedNoise {
    noise: Noise,
    remaining: f64,
}
/// Retains newest sounds on saturation. Age once per simulation tick, then publish
/// accepted gameplay sounds. Consumers should exclude their own source ID.
pub struct NoiseBuffer {
    items: Vec<TimedNoise>,
    capacity: usize,
    dropped: u64,
}
impl NoiseBuffer {
    pub fn new(capacity: usize) -> Result<Self, AiError> {
        if capacity > MAX_NOISES {
            return Err(AiError::Capacity);
        }
        Ok(Self {
            items: Vec::with_capacity(capacity),
            capacity,
            dropped: 0,
        })
    }
    pub fn push(&mut self, noise: Noise, lifetime: f32) -> Result<bool, AiError> {
        if !noise.valid() || !lifetime.is_finite() || !(0.0001..=10.).contains(&lifetime) {
            return Err(AiError::InvalidInput);
        }
        if self.capacity == 0 {
            self.dropped = self.dropped.saturating_add(1);
            return Ok(false);
        }
        if self.items.len() == self.capacity {
            self.items.remove(0);
            self.dropped = self.dropped.saturating_add(1);
        }
        self.items.push(TimedNoise {
            noise,
            remaining: f64::from(lifetime),
        });
        Ok(true)
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), AiError> {
        if !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(AiError::InvalidInput);
        }
        for item in &mut self.items {
            item.remaining -= f64::from(dt);
        }
        self.items.retain(|n| n.remaining > 1e-9);
        Ok(())
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = Noise> + ExactSizeIterator + '_ {
        self.items.iter().map(|n| n.noise)
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TargetSelection {
    pub target: Option<Candidate>,
    pub incomplete: bool,
}
/// Budgeted deterministic priority selection. Non-finite scores are ignored;
/// equal scores prefer the lower ID. Stops as soon as work is deferred, so an
/// unbounded iterator cannot exhaust the frame. All returned targets passed sight.
pub fn select_target(
    candidates: impl IntoIterator<Item = Candidate>,
    observer: Observer,
    sight: Sight,
    budget: &mut WorkBudget,
    query: &mut impl VisibilityQuery,
    priority: impl FnMut(Candidate) -> f32,
) -> Result<TargetSelection, AiError> {
    let (choices, incomplete) =
        visible_choices(candidates, observer, sight, budget, query, priority, None)?;
    Ok(TargetSelection {
        target: choices.best.map(|v| v.target),
        incomplete,
    })
}

/// Why an opt-in [`TargetSelector`] selected or released its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSelectionReason {
    /// No eligible, finite-scored target was evaluated. See `incomplete` too.
    NoEligibleTarget,
    /// There was no previous target; the best evaluated target was acquired.
    Acquired,
    /// The previous target is still the best evaluated target.
    RetainedBest,
    /// A challenger did not exceed the previous target's score by the margin.
    RetainedByHysteresis,
    /// A challenger's score exceeded the previous target's score by the margin.
    SwitchMarginExceeded,
    /// Fresh sight outranks a remembered target, regardless of score margin.
    SwitchedToVisible,
    /// The previous target was not eligible in the evaluated set.
    PreviousIneligible,
}

/// Constant-size explanation of one selection. Scores describe this call only.
/// `T` is [`Candidate`] for sight selection and [`Track`] for memory selection.
#[derive(Clone, Copy, Debug)]
pub struct TargetSelectionReport<T> {
    pub target: Option<T>,
    /// Target ID held before this call, even when it is no longer eligible.
    pub previous: Option<u64>,
    pub reason: TargetSelectionReason,
    /// Sight exhausted its candidate/query allowance before finishing the scan.
    /// The result describes only the evaluated prefix, never unseen candidates.
    pub incomplete: bool,
    pub selected_score: Option<f32>,
    /// Best eligible target's (ID, score), before applying hysteresis.
    pub best: Option<(u64, f32)>,
    /// None when the previous target was absent, rejected, deferred or unscored.
    pub previous_score: Option<f32>,
}

/// Opt-in, allocation-free target retention using a finite, nonnegative score
/// margin. Keep one selector per observer/selection stream and call it after
/// sensing on actual simulation ticks. Reset it when restarting that stream.
///
/// Higher scores win; equal scores prefer the lower host ID on acquisition.
/// An eligible incumbent is retained until a challenger improves its score by
/// **strictly more** than the margin. A zero margin still retains equal scores.
/// Scores are recomputed each call; no stale positions, scores or sight are kept.
/// [`select_target`] and [`Memory::best`] retain their original behavior.
///
/// ```
/// use ferravex_math::Vec3;
/// use ferravex_play::ai::{
///     Candidate, Memory, MemorySettings, Sense, TargetSelector, TargetSelectionReason,
/// };
/// let mut memory = Memory::new(MemorySettings::default())?;
/// let mut selector = TargetSelector::new(0.25)?;
/// memory.observe(Candidate { id: 9, position: Vec3::Z, groups: 1 }, Sense::Sight)?;
/// let first = selector.select_memory(&memory, 1, |_| 1.0);
/// assert_eq!(first.reason, TargetSelectionReason::Acquired);
/// memory.observe(Candidate { id: 2, position: Vec3::X, groups: 1 }, Sense::Sight)?;
/// let next = selector.select_memory(&memory, 1, |t| if t.id == 2 { 1.1 } else { 1.0 });
/// assert_eq!(next.target.unwrap().id, 9);
/// assert_eq!(next.best, Some((2, 1.1)));
/// assert_eq!(next.reason, TargetSelectionReason::RetainedByHysteresis);
/// assert_eq!(selector.current(), Some(9));
/// selector.reset();
/// assert_eq!(selector.current(), None);
/// # Ok::<(), ferravex_play::ai::AiError>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub struct TargetSelector {
    switch_margin: f32,
    current: Option<u64>,
}
impl TargetSelector {
    /// Rejects negative, NaN and infinite margins with `InvalidConfiguration`.
    pub fn new(switch_margin: f32) -> Result<Self, AiError> {
        if !switch_margin.is_finite() || switch_margin < 0. {
            return Err(AiError::InvalidConfiguration);
        }
        Ok(Self {
            switch_margin,
            current: None,
        })
    }

    /// Last selected ID, not a claim that the target is still alive or visible.
    pub fn current(&self) -> Option<u64> {
        self.current
    }

    /// Forgets target identity while preserving the configured margin.
    pub fn reset(&mut self) {
        self.current = None;
    }

    /// Applies hysteresis to [`select_target`]'s budgeted sight scan. Every
    /// returned candidate passed sight **in this call**, including the incumbent.
    /// No extra query is made to retain an incumbent outside the evaluated prefix;
    /// hosts can put it first when budgets are small. Rejected/deferred candidates
    /// never get scored. Non-finite scores are ignored.
    ///
    /// Incomplete scans can replace or clear an unevaluated incumbent. Inspect
    /// `incomplete` before treating absence as a confirmed loss of sight. On an
    /// input error the selected ID is unchanged, but work/query/callback effects
    /// already performed are not rolled back. Empty input consumes no work.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::ai::{
    ///     Candidate, Observer, Sight, TargetSelector, Unoccluded, WorkBudget,
    /// };
    /// let mut selector = TargetSelector::new(0.5)?;
    /// let observer = Observer { position: Vec3::ZERO, forward: Vec3::Z };
    /// let sight = Sight::new(10.0, std::f32::consts::TAU, 1)?;
    /// let candidates = [Candidate { id: 7, position: Vec3::Z, groups: 1 }];
    /// let report = selector.select_visible(
    ///     candidates, observer, sight, &mut WorkBudget::new(1, 1, 0),
    ///     &mut Unoccluded, |_| 1.0,
    /// )?;
    /// assert_eq!(report.target.unwrap().id, 7);
    /// assert!(!report.incomplete);
    /// # Ok::<(), ferravex_play::ai::AiError>(())
    /// ```
    pub fn select_visible(
        &mut self,
        candidates: impl IntoIterator<Item = Candidate>,
        observer: Observer,
        sight: Sight,
        budget: &mut WorkBudget,
        query: &mut impl VisibilityQuery,
        priority: impl FnMut(Candidate) -> f32,
    ) -> Result<TargetSelectionReport<Candidate>, AiError> {
        let (choices, incomplete) = visible_choices(
            candidates,
            observer,
            sight,
            budget,
            query,
            priority,
            self.current,
        )?;
        Ok(self.finish(choices, incomplete))
    }

    /// Scores matching tracks in existing bounded memory without observing,
    /// refreshing expiry or issuing visibility queries. Call [`Memory::advance`]
    /// once per simulation tick before recording accepted detections and selecting.
    /// Removed, expired or masked-out tracks cannot be retained.
    ///
    /// As in [`Memory::best`], fresh sight outranks remembered sight/hearing.
    /// Within that tier, finite scores and the switch margin determine selection;
    /// the callback can use `-(track.age as f32)` for youngest-first priority.
    /// Each matching track is scored once; non-finite scores exclude it. Returned
    /// tracks preserve their visibility, age and remaining lifetime. A selected
    /// memory is awareness, not a new sight detection or permission to attack.
    pub fn select_memory(
        &mut self,
        memory: &Memory,
        mask: u32,
        mut priority: impl FnMut(&Track) -> f32,
    ) -> TargetSelectionReport<Track> {
        let mut choices = SelectionChoices::new(self.current);
        for track in memory.tracks().iter().filter(|t| t.groups & mask != 0) {
            choices.consider(*track, track.id, priority(track), track.visible);
        }
        self.finish(choices, false)
    }

    fn finish<T: Copy>(
        &mut self,
        choices: SelectionChoices<T>,
        incomplete: bool,
    ) -> TargetSelectionReport<T> {
        let previous = self.current;
        let (selected, reason) = match (choices.best, choices.incumbent) {
            (None, _) => (None, TargetSelectionReason::NoEligibleTarget),
            (Some(best), None) => (
                Some(best),
                if previous.is_some() {
                    TargetSelectionReason::PreviousIneligible
                } else {
                    TargetSelectionReason::Acquired
                },
            ),
            (Some(best), Some(old)) if best.id == old.id => {
                (Some(best), TargetSelectionReason::RetainedBest)
            }
            (Some(best), Some(old)) if best.visible && !old.visible => {
                (Some(best), TargetSelectionReason::SwitchedToVisible)
            }
            (Some(best), Some(old))
                if f64::from(best.score) - f64::from(old.score) > f64::from(self.switch_margin) =>
            {
                (Some(best), TargetSelectionReason::SwitchMarginExceeded)
            }
            (_, Some(old)) => (Some(old), TargetSelectionReason::RetainedByHysteresis),
        };
        self.current = selected.map(|v| v.id);
        TargetSelectionReport {
            target: selected.map(|v| v.target),
            previous,
            reason,
            incomplete,
            selected_score: selected.map(|v| v.score),
            best: choices.best.map(|v| (v.id, v.score)),
            previous_score: choices.incumbent.map(|v| v.score),
        }
    }
}

#[derive(Clone, Copy)]
struct ScoredTarget<T> {
    target: T,
    id: u64,
    score: f32,
    visible: bool,
}
impl<T> ScoredTarget<T> {
    fn outranks(&self, old: &Self) -> bool {
        self.visible && !old.visible
            || self.visible == old.visible
                && (self.score > old.score || self.score == old.score && self.id < old.id)
    }
}
struct SelectionChoices<T> {
    current: Option<u64>,
    best: Option<ScoredTarget<T>>,
    incumbent: Option<ScoredTarget<T>>,
}
impl<T: Copy> SelectionChoices<T> {
    fn new(current: Option<u64>) -> Self {
        Self {
            current,
            best: None,
            incumbent: None,
        }
    }

    fn consider(&mut self, target: T, id: u64, score: f32, visible: bool) {
        if !score.is_finite() {
            return;
        }
        let value = ScoredTarget {
            target,
            id,
            score,
            visible,
        };
        if self.best.is_none_or(|old| value.outranks(&old)) {
            self.best = Some(value);
        }
        if self.current == Some(id) && self.incumbent.is_none_or(|old| value.outranks(&old)) {
            self.incumbent = Some(value);
        }
    }
}

fn visible_choices(
    candidates: impl IntoIterator<Item = Candidate>,
    observer: Observer,
    sight: Sight,
    budget: &mut WorkBudget,
    query: &mut impl VisibilityQuery,
    mut priority: impl FnMut(Candidate) -> f32,
    current: Option<u64>,
) -> Result<(SelectionChoices<Candidate>, bool), AiError> {
    let mut choices = SelectionChoices::new(current);
    let mut incomplete = false;
    for target in candidates {
        match sight.test(observer, target, budget, query)? {
            Detection::Deferred => {
                incomplete = true;
                break;
            }
            Detection::Rejected => {}
            Detection::Detected => {
                choices.consider(target, target.id, priority(target), true);
            }
        }
    }
    Ok((choices, incomplete))
}
