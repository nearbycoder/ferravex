//! Bounded telemetry. Register counters once; recording does not allocate.
//! Summary construction clones a bounded name; ids belong to their originating profiler.
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
const HISTORY: usize = 120;
const MAX_COUNTERS: usize = 128;
const MAX_NAME_BYTES: usize = 128;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CounterId(usize);
#[derive(Clone, Debug)]
pub struct Summary {
    pub name: String,
    pub mean_us: f64,
    pub p99_us: u64,
    pub max_us: u64,
    pub over_budget: bool,
    pub samples: usize,
}
struct Counter {
    name: String,
    values: [u64; HISTORY],
    next: usize,
    len: usize,
    budget_us: u64,
}
#[derive(Default)]
pub struct Profiler {
    counters: Vec<Counter>,
}
impl Profiler {
    pub fn register(&mut self, name: &str, budget: Duration) -> CounterId {
        self.try_register(name, budget)
            .expect("invalid profiler name or counter capacity exceeded")
    }
    /// Existing names retain their initial budget. Names are 1..=128 UTF-8 bytes.
    pub fn try_register(&mut self, name: &str, budget: Duration) -> Option<CounterId> {
        if name.is_empty() || name.len() > MAX_NAME_BYTES {
            return None;
        }
        if let Some(i) = self
            .counters
            .iter()
            .position(|counter| counter.name == name)
        {
            return Some(CounterId(i));
        }
        if self.counters.len() >= MAX_COUNTERS {
            return None;
        }
        let i = self.counters.len();
        self.counters.push(Counter {
            name: name.into(),
            values: [0; HISTORY],
            next: 0,
            len: 0,
            budget_us: micros(budget),
        });
        Some(CounterId(i))
    }
    pub fn record(&mut self, id: CounterId, duration: Duration) {
        assert!(
            self.try_record(id, duration),
            "counter id does not belong to this profiler"
        );
    }
    pub fn try_record(&mut self, id: CounterId, duration: Duration) -> bool {
        let Some(counter) = self.counters.get_mut(id.0) else {
            return false;
        };
        counter.values[counter.next] = micros(duration);
        counter.next = (counter.next + 1) % HISTORY;
        counter.len = (counter.len + 1).min(HISTORY);
        true
    }
    /// Native convenience. Browser callers supply performance.now through measure_with_clock.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn measure<T>(&mut self, id: CounterId, f: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let result = f();
        self.record(id, start.elapsed());
        result
    }
    pub fn measure_with_clock<T>(
        &mut self,
        id: CounterId,
        mut now: impl FnMut() -> Duration,
        f: impl FnOnce() -> T,
    ) -> T {
        let start = now();
        let result = f();
        self.record(id, now().checked_sub(start).unwrap_or_default());
        result
    }
    /// Clear one counter's history while preserving its name, budget and ID.
    pub fn reset(&mut self, id: CounterId) -> bool {
        let Some(counter) = self.counters.get_mut(id.0) else {
            return false;
        };
        counter.values.fill(0);
        counter.next = 0;
        counter.len = 0;
        true
    }
    /// Begin a fresh profiling capture without invalidating registered IDs.
    pub fn reset_all(&mut self) {
        for i in 0..self.counters.len() {
            self.reset(CounterId(i));
        }
    }
    pub fn summary(&self, id: CounterId) -> Summary {
        self.try_summary(id)
            .expect("counter id does not belong to this profiler")
    }
    pub fn try_summary(&self, id: CounterId) -> Option<Summary> {
        let counter = self.counters.get(id.0)?;
        let mut sorted = counter.values;
        sorted[..counter.len].sort_unstable();
        let max = sorted[..counter.len].last().copied().unwrap_or(0);
        let mean = if counter.len == 0 {
            0.0
        } else {
            sorted[..counter.len].iter().map(|&n| n as f64).sum::<f64>() / counter.len as f64
        };
        let percentile = if counter.len == 0 {
            0
        } else {
            sorted[(counter.len * 99).div_ceil(100) - 1]
        };
        Some(Summary {
            name: counter.name.clone(),
            mean_us: mean,
            p99_us: percentile,
            max_us: max,
            over_budget: max > counter.budget_us,
            samples: counter.len,
        })
    }
}
fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_budget_and_quantile() {
        let mut p = Profiler::default();
        let id = p.register("physics", Duration::from_micros(100));
        for _ in 0..130 {
            p.record(id, Duration::from_micros(5));
        }
        p.record(id, Duration::from_micros(250));
        let s = p.summary(id);
        assert_eq!(s.samples, 120);
        assert!(s.over_budget);
        assert_eq!(s.max_us, 250);
    }
    #[test]
    fn p99_uses_nearest_rank_without_off_by_one() {
        let mut p = Profiler::default();
        let id = p.register("rank", Duration::MAX);
        for n in 1..=100 {
            p.record(id, Duration::from_micros(n));
        }
        assert_eq!(p.summary(id).p99_us, 99);
        assert_eq!(p.summary(id).mean_us, 50.5);
    }
    #[test]
    fn bounded_names_and_external_clock() {
        let mut p = Profiler::default();
        assert!(p.try_register(&"x".repeat(129), Duration::ZERO).is_none());
        let id = p.register("clock", Duration::ZERO);
        let mut stamps = [Duration::from_micros(8), Duration::from_micros(3)].into_iter();
        assert_eq!(
            p.measure_with_clock(id, || stamps.next().unwrap(), || 42),
            42
        );
        assert_eq!(p.summary(id).max_us, 0);
        p.record(id, Duration::MAX);
        assert_eq!(p.summary(id).max_us, u64::MAX);
        assert!(!p.try_record(CounterId(10), Duration::ZERO));
    }
}

#[cfg(test)]
mod reset_tests {
    use super::*;
    #[test]
    fn reset_drops_stale_ring_samples_but_keeps_registration() {
        let mut p = Profiler::default();
        let a = p.register("a", Duration::ZERO);
        let b = p.register("b", Duration::ZERO);
        for _ in 0..130 {
            p.record(a, Duration::from_micros(9));
        }
        p.record(b, Duration::from_micros(3));
        assert!(p.reset(a));
        assert_eq!(p.summary(a).samples, 0);
        assert_eq!(p.summary(a).max_us, 0);
        assert_eq!(p.register("a", Duration::MAX), a);
        assert_eq!(p.summary(b).samples, 1);
        p.record(a, Duration::from_micros(1));
        assert_eq!(p.summary(a).mean_us, 1.);
        p.reset_all();
        assert_eq!(p.summary(a).samples + p.summary(b).samples, 0);
        assert!(!p.reset(CounterId(999)));
    }
}
