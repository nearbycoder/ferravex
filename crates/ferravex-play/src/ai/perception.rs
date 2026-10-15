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
    mut priority: impl FnMut(Candidate) -> f32,
) -> Result<TargetSelection, AiError> {
    let mut best: Option<(Candidate, f32)> = None;
    let mut incomplete = false;
    for target in candidates {
        match sight.test(observer, target, budget, query)? {
            Detection::Deferred => {
                incomplete = true;
                break;
            }
            Detection::Rejected => {}
            Detection::Detected => {
                let score = priority(target);
                if score.is_finite()
                    && best.is_none_or(|(old, value)| {
                        score > value || score == value && target.id < old.id
                    })
                {
                    best = Some((target, score));
                }
            }
        }
    }
    Ok(TargetSelection {
        target: best.map(|v| v.0),
        incomplete,
    })
}
