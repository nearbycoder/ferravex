//! Transport clock shared by native sockets and browser hosts. Browser hosts
//! explicitly supply monotonic microseconds; no unsupported Wasm OS clock call.
#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
thread_local! {static NOW:Cell<u64>=const{Cell::new(0)};}
#[cfg(target_arch = "wasm32")]
pub fn set_browser_time(micros: u64) -> bool {
    NOW.with(|now| {
        if micros > 1 << 53 || micros < now.get() {
            return false;
        }
        now.set(micros);
        true
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(i128);
impl Instant {
    pub fn now() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
            Self(
                ORIGIN
                    .get_or_init(std::time::Instant::now)
                    .elapsed()
                    .as_micros()
                    .min((1u128 << 53) - 1) as i128,
            )
        }
        #[cfg(target_arch = "wasm32")]
        Self(i128::from(NOW.with(Cell::get)))
    }
    pub fn as_micros(self) -> u64 {
        self.0.max(0) as u64
    }
    pub fn elapsed(self) -> Duration {
        Self::now().saturating_duration_since(self)
    }
    pub fn duration_since(self, earlier: Self) -> Duration {
        self.saturating_duration_since(earlier)
    }
    pub fn saturating_duration_since(self, earlier: Self) -> Duration {
        Duration::from_micros((self.0 - earlier.0).clamp(0, i128::from(u64::MAX)) as u64)
    }
}
impl std::ops::Sub<Duration> for Instant {
    type Output = Self;
    fn sub(self, rhs: Duration) -> Self {
        Self(self.0 - rhs.as_micros().min(i128::MAX as u128) as i128)
    }
}
impl std::ops::Add<Duration> for Instant {
    type Output = Self;
    fn add(self, rhs: Duration) -> Self {
        Self(self.0 + rhs.as_micros().min(i128::MAX as u128) as i128)
    }
}
