use super::{MAX_EFFECT_LIFETIME, RibbonPoint, RibbonSource};
pub const MAX_TRAIL_POINTS: usize = 2048;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrailConfig {
    pub capacity: usize,
    pub lifetime: f32,
    pub spacing: f32,
    pub teleport_distance: f32,
    pub max_per_update: u16,
    pub fade_width: bool,
}
impl Default for TrailConfig {
    fn default() -> Self {
        Self {
            capacity: 128,
            lifetime: 0.6,
            spacing: 0.08,
            teleport_distance: 12.,
            max_per_update: 64,
            fade_width: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrailSample {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub width: f32,
}
impl TrailSample {
    fn point(self, break_before: bool) -> RibbonPoint {
        RibbonPoint {
            position: self.position,
            color: self.color,
            width: self.width,
            break_before,
        }
    }
    fn interpolate(self, b: Self, t: f64) -> Self {
        let lerp = |a: f32, b: f32| (f64::from(a) * (1. - t) + f64::from(b) * t) as f32;
        Self {
            position: std::array::from_fn(|i| lerp(self.position[i], b.position[i])),
            color: std::array::from_fn(|i| lerp(self.color[i], b.color[i])),
            width: lerp(self.width, b.width),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrailError {
    InvalidConfig,
    InvalidSample,
    InvalidDelta,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrailStats {
    pub sampled: u64,
    pub expired: u64,
    pub dropped_budget: u64,
    pub overwritten: u64,
    pub teleports: u64,
}
impl TrailStats {
    fn add(&mut self, b: Self) {
        self.sampled = self.sampled.saturating_add(b.sampled);
        self.expired = self.expired.saturating_add(b.expired);
        self.dropped_budget = self.dropped_budget.saturating_add(b.dropped_budget);
        self.overwritten = self.overwritten.saturating_add(b.overwritten);
        self.teleports = self.teleports.saturating_add(b.teleports);
    }
}
#[derive(Clone, Copy, Default)]
struct Point {
    point: RibbonPoint,
    age: f64,
}
/// Fixed ring, distance resampling and a current head. Capacity bounds the visible
/// point count (including that head); one extra head value is stored separately.
/// Stops and teleports create breaks; old segments fade naturally without bridges.
pub struct Trail {
    config: TrailConfig,
    points: Vec<Point>,
    start: usize,
    len: usize,
    previous: Option<TrailSample>,
    head: Option<TrailSample>,
    carry: f64,
    total: TrailStats,
}
impl Trail {
    pub fn new(config: TrailConfig) -> Result<Self, TrailError> {
        if !(2..=MAX_TRAIL_POINTS).contains(&config.capacity)
            || !config.lifetime.is_finite()
            || !(0.0001..=MAX_EFFECT_LIFETIME).contains(&config.lifetime)
            || !config.spacing.is_finite()
            || !(0.0001..=1e6).contains(&config.spacing)
            || !config.teleport_distance.is_finite()
            || !(config.spacing..=1e6).contains(&config.teleport_distance)
            || !(1..=256).contains(&config.max_per_update)
        {
            return Err(TrailError::InvalidConfig);
        }
        Ok(Self {
            points: vec![Point::default(); config.capacity],
            config,
            start: 0,
            len: 0,
            previous: None,
            head: None,
            carry: 0.,
            total: TrailStats::default(),
        })
    }
    pub fn capacity(&self) -> usize {
        self.config.capacity
    }
    pub fn len(&self) -> usize {
        (self.len + usize::from(self.head.is_some())).min(self.config.capacity)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn config(&self) -> TrailConfig {
        self.config
    }
    pub fn stats(&self) -> TrailStats {
        self.total
    }
    pub fn clear(&mut self) {
        self.start = 0;
        self.len = 0;
        self.previous = None;
        self.head = None;
        self.carry = 0.;
    }
    pub fn reset_stats(&mut self) {
        self.total = TrailStats::default();
    }
    fn push(&mut self, sample: TrailSample, age: f64, break_before: bool, stats: &mut TrailStats) {
        if age >= f64::from(self.config.lifetime) {
            stats.expired += 1;
            return;
        }
        let at = (self.start + self.len) % self.config.capacity;
        self.points[at] = Point {
            point: sample.point(break_before),
            age,
        };
        if self.len == self.config.capacity {
            self.start = (self.start + 1) % self.config.capacity;
            stats.overwritten += 1;
        } else {
            self.len += 1;
        }
        stats.sampled += 1;
    }
    /// None stops emission while aging existing points. Invalid input leaves the
    /// trail unchanged. New samples receive their within-frame birth ages.
    pub fn update(
        &mut self,
        dt: f32,
        sample: Option<TrailSample>,
    ) -> Result<TrailStats, TrailError> {
        if !dt.is_finite() || !(0.0..=60.).contains(&dt) {
            return Err(TrailError::InvalidDelta);
        }
        if sample.is_some_and(|s| !s.point(false).valid()) {
            return Err(TrailError::InvalidSample);
        }
        let mut stats = TrailStats::default();
        for i in 0..self.len {
            self.points[(self.start + i) % self.config.capacity].age += f64::from(dt);
        }
        while self.len > 0 && self.points[self.start].age >= f64::from(self.config.lifetime) {
            self.start = (self.start + 1) % self.config.capacity;
            self.len -= 1;
            stats.expired += 1;
        }
        let Some(sample) = sample else {
            if let Some(head) = self.head.take() {
                self.push(head, f64::from(dt), false, &mut stats);
            }
            self.previous = None;
            self.carry = 0.;
            self.total.add(stats);
            return Ok(stats);
        };
        let Some(previous) = self.previous else {
            self.push(sample, 0., true, &mut stats);
            self.previous = Some(sample);
            self.head = None;
            self.total.add(stats);
            return Ok(stats);
        };
        let distance = previous
            .position
            .iter()
            .zip(sample.position)
            .map(|(a, b)| (f64::from(*a) - f64::from(b)).powi(2))
            .sum::<f64>()
            .sqrt();
        if distance > f64::from(self.config.teleport_distance) {
            self.push(sample, 0., true, &mut stats);
            self.previous = Some(sample);
            self.head = None;
            self.carry = 0.;
            stats.teleports += 1;
            self.total.add(stats);
            return Ok(stats);
        }
        if distance > 0. {
            if self.len == 0 {
                self.push(previous, f64::from(dt), true, &mut stats);
            }
            let total = self.carry + distance;
            let spacing = f64::from(self.config.spacing);
            let requested = (total / spacing).floor() as u64;
            let count = requested.min(u64::from(self.config.max_per_update));
            stats.dropped_budget = requested - count;
            for i in requested - count..requested {
                let t = (((i + 1) as f64 * spacing - self.carry) / distance).clamp(0., 1.);
                self.push(
                    previous.interpolate(sample, t),
                    f64::from(dt) * (1. - t),
                    i == requested - count && stats.dropped_budget > 0,
                    &mut stats,
                );
            }
            self.carry = (total - requested as f64 * spacing).clamp(0., spacing);
        }
        self.head = Some(sample);
        self.previous = Some(sample);
        self.total.add(stats);
        Ok(stats)
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = RibbonPoint> + '_ {
        (0..self.len()).map(|i| self.point(i).expect("bounded trail point"))
    }
}
impl RibbonSource for Trail {
    fn point_count(&self) -> usize {
        self.len()
    }
    fn point(&self, index: usize) -> Option<RibbonPoint> {
        if index >= self.len() {
            return None;
        }
        let skip = usize::from(self.head.is_some() && self.len == self.config.capacity);
        let stored = self.len - skip;
        if index < stored {
            let p = self.points[(self.start + skip + index) % self.config.capacity];
            let age = (p.age / f64::from(self.config.lifetime)).clamp(0., 1.) as f32;
            let mut point = p.point;
            point.color[3] *= 1. - age;
            if self.config.fade_width {
                point.width *= 1. - age;
            }
            Some(point)
        } else {
            self.head.map(|s| s.point(false))
        }
    }
}
