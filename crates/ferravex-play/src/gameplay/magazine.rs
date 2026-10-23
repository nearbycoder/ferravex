//! Ammunition and reload timing. A shot spends one round only on acceptance.
//!
//! Advance with simulation time on actual ticks. Status queries do not advance
//! the reload or emit completion; use the result of [`Magazine::advance`] for
//! accepted reload feedback.
//!
//! ```
//! use ferravex_play::gameplay::magazine::Magazine;
//!
//! let mut magazine = Magazine::new(4, 6, 2.0)?;
//! assert_eq!(magazine.capacity(), 4);
//! assert_eq!(magazine.total_ammo(), 10);
//! assert!(magazine.can_fire());
//! assert!(!magazine.can_reload());
//! assert!(magazine.fire());
//! assert!(magazine.can_reload());
//! assert!(magazine.reload());
//! assert!(!magazine.can_fire());
//! assert!(!magazine.can_reload());
//! assert!(!magazine.advance(0.5)?);
//! let status = magazine.reload_status().unwrap();
//! assert_eq!(status.duration_seconds, 2.0);
//! assert_eq!(status.remaining_seconds, 1.5);
//! assert_eq!(status.progress, 0.25);
//! assert_eq!(status.rounds, 1);
//! assert!(magazine.advance(1.5)?);
//! assert_eq!(magazine.reload_status(), None);
//! assert!(!magazine.advance(1.0)?); // No duplicate completion.
//! # Ok::<(), ferravex_play::gameplay::LoopError>(())
//! ```
use super::{LoopError, seconds};

const MAX_CAPACITY: u16 = 4096;
const MAX_RESERVE: u32 = 1_000_000;

/// Read-only snapshot of an active reload, obtained with
/// [`Magazine::reload_status`]. Ammo remains in reserve until completion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReloadStatus {
    /// Total duration of this reload in simulation seconds.
    pub duration_seconds: f32,
    /// Simulation seconds still needed, as reported by [`Magazine::remaining`].
    pub remaining_seconds: f32,
    /// Elapsed fraction in `0.0..=1.0`. Rounding can yield `1.0` while active;
    /// only [`Magazine::advance`] reports completion.
    pub progress: f32,
    /// Rounds that would transfer at completion with the current capacity and
    /// reserve. Reconfiguration or reserve edits can change this projection.
    pub rounds: u16,
}

/// Bounded ammunition storage and a caller-driven reload timer.
///
/// Reloads and capacity changes conserve [`Self::total_ammo`]. Only an accepted
/// shot or explicit reserve addition/removal changes that total after creation.
#[derive(Clone, Copy, Debug)]
pub struct Magazine {
    capacity: u16,
    loaded: u16,
    reserve: u32,
    duration: f64,
    remaining: f64,
}
impl Magazine {
    /// Creates a full magazine with additional reserve ammunition.
    ///
    /// Capacity must be in `1..=4096`, reserve at most `1_000_000`, and reload
    /// duration finite and in `0.0 < reload_seconds <= 3600.0`. Invalid inputs
    /// return [`LoopError::Invalid`].
    pub fn new(capacity: u16, reserve: u32, reload_seconds: f32) -> Result<Self, LoopError> {
        let duration = seconds(reload_seconds)?;
        if capacity == 0 || capacity > MAX_CAPACITY || reserve > MAX_RESERVE || duration == 0. {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            capacity,
            loaded: capacity,
            reserve,
            duration,
            remaining: 0.,
        })
    }
    /// Maximum loaded rounds. See the [module example](crate::gameplay::magazine).
    pub fn capacity(&self) -> u16 {
        self.capacity
    }
    pub fn loaded(&self) -> u16 {
        self.loaded
    }
    pub fn reserve(&self) -> u32 {
        self.reserve
    }
    /// Loaded plus reserve rounds, including during a reload. This is bounded
    /// by `1_004_096`. See the [module example](crate::gameplay::magazine).
    pub fn total_ammo(&self) -> u32 {
        u32::from(self.loaded) + self.reserve
    }
    pub fn reloading(&self) -> bool {
        self.remaining > 0.
    }
    pub fn remaining(&self) -> f32 {
        self.remaining as f32
    }
    /// Whether a call to [`Self::fire`] would accept a shot in the current state.
    /// See the [module example](crate::gameplay::magazine).
    pub fn can_fire(&self) -> bool {
        !self.reloading() && self.loaded > 0
    }
    /// Whether a call to [`Self::reload`] would start a new reload in the current
    /// state. See the [module example](crate::gameplay::magazine).
    pub fn can_reload(&self) -> bool {
        !self.reloading() && self.loaded < self.capacity && self.reserve > 0
    }
    /// Inspects an active reload without consuming time or transferring ammo.
    ///
    /// Returns `None` before starting, after cancellation, and after completion.
    /// The projected round count uses the current capacity and reserve, without
    /// reserving ammo. See the [module example](crate::gameplay::magazine).
    pub fn reload_status(&self) -> Option<ReloadStatus> {
        self.reloading().then(|| ReloadStatus {
            duration_seconds: self.duration as f32,
            remaining_seconds: self.remaining(),
            progress: (1. - self.remaining / self.duration).clamp(0., 1.) as f32,
            rounds: self.reload_amount(),
        })
    }
    pub fn fire(&mut self) -> bool {
        if !self.can_fire() {
            return false;
        }
        self.loaded -= 1;
        true
    }
    pub fn reload(&mut self) -> bool {
        if !self.can_reload() {
            return false;
        }
        self.remaining = self.duration;
        true
    }
    pub fn cancel_reload(&mut self) {
        self.remaining = 0.;
    }
    /// Returns true exactly once when ammunition transfers at reload completion.
    ///
    /// Supply simulation seconds on actual ticks. Invalid deltas (non-finite,
    /// negative, or greater than `3600.0`) leave the entire state unchanged.
    pub fn advance(&mut self, dt: f32) -> Result<bool, LoopError> {
        let dt = seconds(dt)?;
        if !self.reloading() {
            return Ok(false);
        }
        self.remaining = (self.remaining - dt).max(0.);
        if self.remaining > 1e-7 {
            return Ok(false);
        }
        self.remaining = 0.;
        let amount = self.reload_amount();
        self.loaded += amount;
        self.reserve -= u32::from(amount);
        Ok(true)
    }
    /// Changes capacity while conserving all loaded and reserve ammunition.
    ///
    /// Shrinking returns excess loaded rounds to reserve and reports that count.
    /// Growing creates empty space without loading or creating rounds. A reload
    /// keeps its timing unless the magazine becomes full, which cancels it
    /// without a completion event. Setting the same capacity is a no-op.
    ///
    /// Returns [`LoopError::Invalid`] outside `1..=4096`, or
    /// [`LoopError::Capacity`] if returned rounds would exceed the reserve bound
    /// of `1_000_000`. Either error leaves ammo, capacity and reload untouched.
    ///
    /// ```
    /// use ferravex_play::gameplay::{LoopError, magazine::Magazine};
    ///
    /// let mut magazine = Magazine::new(6, 10, 1.0)?;
    /// let total = magazine.total_ammo();
    /// assert_eq!(magazine.set_capacity(3)?, 3);
    /// assert_eq!((magazine.loaded(), magazine.reserve()), (3, 13));
    /// assert_eq!(magazine.total_ammo(), total);
    /// assert_eq!(magazine.set_capacity(8)?, 0);
    /// assert_eq!(magazine.loaded(), 3); // Growing does not refill.
    /// assert_eq!(magazine.total_ammo(), total);
    /// assert_eq!(magazine.set_capacity(0), Err(LoopError::Invalid));
    /// assert_eq!(magazine.capacity(), 8);
    /// # Ok::<(), LoopError>(())
    /// ```
    pub fn set_capacity(&mut self, capacity: u16) -> Result<u16, LoopError> {
        if capacity == 0 || capacity > MAX_CAPACITY {
            return Err(LoopError::Invalid);
        }
        let returned = self.loaded.saturating_sub(capacity);
        let reserve = self
            .reserve
            .checked_add(u32::from(returned))
            .filter(|n| *n <= MAX_RESERVE)
            .ok_or(LoopError::Capacity)?;
        self.capacity = capacity;
        self.loaded -= returned;
        self.reserve = reserve;
        if self.loaded == self.capacity {
            self.cancel_reload();
        }
        Ok(returned)
    }
    /// Adds externally supplied rounds to reserve without loading them or
    /// changing reload timing. Increases [`Self::total_ammo`] by exactly `amount`.
    ///
    /// Zero is a no-op. Returns [`LoopError::Capacity`] without changes on
    /// overflow or if the reserve would exceed `1_000_000` rounds. See
    /// [`Self::remove_reserve`] for a transfer example.
    pub fn add_reserve(&mut self, amount: u32) -> Result<(), LoopError> {
        self.reserve = self
            .reserve
            .checked_add(amount)
            .filter(|n| *n <= MAX_RESERVE)
            .ok_or(LoopError::Capacity)?;
        Ok(())
    }
    /// Removes exactly `amount` reserve rounds for the caller to transfer or
    /// spend. Decreases [`Self::total_ammo`] by that amount; loaded ammo is intact.
    ///
    /// Zero is a no-op. Returns [`LoopError::Unavailable`] without changes when
    /// reserve is insufficient. Removing the last reserve round cancels any
    /// reload without completion; otherwise reload timing is preserved.
    ///
    /// ```
    /// use ferravex_play::gameplay::{LoopError, magazine::Magazine};
    ///
    /// let mut magazine = Magazine::new(3, 5, 1.0)?;
    /// assert!(magazine.fire());
    /// assert!(magazine.reload());
    /// assert_eq!(magazine.remove_reserve(6), Err(LoopError::Unavailable));
    /// assert!(magazine.reloading());
    /// magazine.remove_reserve(5)?; // Caller now owns these five rounds.
    /// assert_eq!(magazine.total_ammo(), 2);
    /// assert!(!magazine.advance(1.0)?); // Cancellation is not completion.
    /// magazine.add_reserve(5)?; // Return the rounds without restarting reload.
    /// assert_eq!(magazine.total_ammo(), 7);
    /// assert!(!magazine.reloading());
    /// # Ok::<(), LoopError>(())
    /// ```
    pub fn remove_reserve(&mut self, amount: u32) -> Result<(), LoopError> {
        let reserve = self
            .reserve
            .checked_sub(amount)
            .ok_or(LoopError::Unavailable)?;
        self.reserve = reserve;
        if self.reserve == 0 {
            self.cancel_reload();
        }
        Ok(())
    }

    fn reload_amount(&self) -> u16 {
        u32::from(self.capacity - self.loaded).min(self.reserve) as u16
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn state(m: &Magazine) -> (u16, u16, u32, f64, f64) {
        (m.capacity, m.loaded, m.reserve, m.duration, m.remaining)
    }

    #[test]
    fn reloading_transfers_once_and_cancellation_conserves_ammo() {
        let mut m = Magazine::new(3, 5, 1.).unwrap();
        for _ in 0..3 {
            assert!(m.fire());
        }
        assert!(!m.fire());
        assert!(m.reload());
        assert!(!m.fire());
        m.advance(0.5).unwrap();
        m.cancel_reload();
        assert_eq!((m.loaded(), m.reserve()), (0, 5));
        m.reload();
        for _ in 0..9 {
            assert!(!m.advance(0.1).unwrap());
        }
        assert!(m.advance(0.1).unwrap());
        assert_eq!((m.loaded(), m.reserve()), (3, 2));
        assert!(!m.advance(2.).unwrap());
        assert!(m.add_reserve(u32::MAX).is_err());
        assert_eq!(m.reserve(), 2);
    }

    #[test]
    fn status_matches_actions_and_projects_a_partial_reload_without_side_effects() {
        let mut m = Magazine::new(4, 1, 2.).unwrap();
        assert_eq!(m.capacity(), 4);
        assert_eq!(m.total_ammo(), 5);
        assert!(m.can_fire());
        assert!(!m.can_reload());
        assert!(!m.reload());
        assert_eq!(m.reload_status(), None);
        assert!(m.fire());
        assert!(m.fire());
        assert!(m.can_reload());
        assert!(m.reload());
        let initial = ReloadStatus {
            duration_seconds: 2.,
            remaining_seconds: 2.,
            progress: 0.,
            rounds: 1,
        };
        assert_eq!(m.reload_status(), Some(initial));
        assert!(!m.can_fire());
        assert!(!m.can_reload());
        assert!(!m.advance(0.5).unwrap());
        let before = state(&m);
        let status = ReloadStatus {
            remaining_seconds: 1.5,
            progress: 0.25,
            ..initial
        };
        for _ in 0..3 {
            assert_eq!(m.reload_status(), Some(status));
            assert!(!m.fire());
            assert!(!m.reload());
            assert!(!m.advance(0.).unwrap());
            assert_eq!(state(&m), before);
        }
        assert_eq!((m.loaded(), m.reserve(), m.total_ammo()), (2, 1, 3));
        assert!(m.advance(2.).unwrap());
        assert_eq!((m.loaded(), m.reserve(), m.total_ammo()), (3, 0, 3));
        assert_eq!(m.reload_status(), None);
        assert!(m.can_fire());
        assert!(!m.can_reload());
        assert!(!m.advance(2.).unwrap());
        for _ in 0..3 {
            assert!(m.fire());
        }
        assert!(!m.can_fire());
        assert!(!m.fire());
        assert!(!m.can_reload());
        assert!(!m.reload());
    }

    #[test]
    fn capacity_edits_conserve_ammo_and_do_not_fill_new_space() {
        let mut m = Magazine::new(6, 10, 1.).unwrap();
        let total = m.total_ammo();
        assert_eq!(m.set_capacity(3), Ok(3));
        assert_eq!((m.capacity(), m.loaded(), m.reserve()), (3, 3, 13));
        assert_eq!(m.total_ammo(), total);
        assert_eq!(m.set_capacity(8), Ok(0));
        assert_eq!((m.capacity(), m.loaded(), m.reserve()), (8, 3, 13));
        assert_eq!(m.total_ammo(), total);
        assert!(m.can_reload());
        assert_eq!(m.reload_status(), None);
        assert!(!m.advance(1.).unwrap());
        assert!(m.reload());
        assert!(m.advance(1.).unwrap());
        assert_eq!((m.loaded(), m.reserve(), m.total_ammo()), (8, 8, total));
    }

    #[test]
    fn capacity_edits_during_reload_keep_elapsed_time_and_use_current_capacity() {
        let mut m = Magazine::new(6, 10, 2.).unwrap();
        for _ in 0..4 {
            assert!(m.fire());
        }
        let total = m.total_ammo();
        assert!(m.reload());
        assert!(!m.advance(0.5).unwrap());
        let original = m.reload_status().unwrap();
        assert_eq!(m.set_capacity(8), Ok(0));
        assert_eq!(
            m.reload_status(),
            Some(ReloadStatus {
                rounds: 6,
                ..original
            })
        );
        assert_eq!(m.set_capacity(4), Ok(0));
        assert_eq!(
            m.reload_status(),
            Some(ReloadStatus {
                rounds: 2,
                ..original
            })
        );
        assert_eq!(m.total_ammo(), total);
        assert!(m.advance(1.5).unwrap());
        assert_eq!((m.loaded(), m.reserve(), m.total_ammo()), (4, 8, total));
        assert!(!m.advance(3600.).unwrap());
    }

    #[test]
    fn shrinking_to_or_below_loaded_cancels_without_completion() {
        for capacity in [2, 3] {
            let mut m = Magazine::new(6, 8, 1.).unwrap();
            for _ in 0..3 {
                assert!(m.fire());
            }
            let total = m.total_ammo();
            assert!(m.reload());
            assert!(!m.advance(0.25).unwrap());
            assert_eq!(m.set_capacity(capacity), Ok(3 - capacity));
            assert_eq!(m.total_ammo(), total);
            assert_eq!(m.loaded(), capacity);
            assert_eq!(m.reserve(), 8 + u32::from(3 - capacity));
            assert!(!m.reloading());
            assert_eq!(m.reload_status(), None);
            assert_eq!(m.remaining(), 0.);
            assert!(!m.advance(1.).unwrap());
            assert!(m.fire());
            assert!(m.reload());
            assert_eq!(m.reload_status().unwrap().progress, 0.);
            assert_eq!(m.remaining(), 1.);
            assert!(m.advance(1.).unwrap());
            assert!(!m.advance(1.).unwrap());
            assert_eq!(m.total_ammo(), total - 1);
        }
    }

    #[test]
    fn failed_edits_and_invalid_time_leave_ammo_capacity_and_reload_untouched() {
        let mut m = Magazine::new(4, 1_000_000, 1.).unwrap();
        assert!(m.fire());
        assert!(m.reload());
        assert!(!m.advance(0.25).unwrap());
        let before = state(&m);
        for capacity in [0, 4097, u16::MAX] {
            assert_eq!(m.set_capacity(capacity), Err(LoopError::Invalid));
            assert_eq!(state(&m), before);
        }
        assert_eq!(m.set_capacity(2), Err(LoopError::Capacity));
        assert_eq!(state(&m), before);
        for amount in [1, u32::MAX] {
            assert_eq!(m.add_reserve(amount), Err(LoopError::Capacity));
            assert_eq!(state(&m), before);
        }
        for amount in [1_000_001, u32::MAX] {
            assert_eq!(m.remove_reserve(amount), Err(LoopError::Unavailable));
            assert_eq!(state(&m), before);
        }
        for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 3601.] {
            assert_eq!(m.advance(dt), Err(LoopError::Invalid));
            assert_eq!(state(&m), before);
        }
        assert_eq!(m.set_capacity(4), Ok(0));
        assert_eq!(m.add_reserve(0), Ok(()));
        assert_eq!(m.remove_reserve(0), Ok(()));
        assert_eq!(state(&m), before);
        assert!(m.advance(0.75).unwrap());
        assert_eq!((m.loaded(), m.reserve()), (4, 999_999));
        assert!(!m.advance(1.).unwrap());
    }

    #[test]
    fn capacity_and_reserve_bounds_are_exact_and_overflow_never_loses_rounds() {
        let mut m = Magazine::new(4096, 1_000_000, 1.).unwrap();
        assert_eq!(m.total_ammo(), 1_004_096);
        assert_eq!(m.set_capacity(4095), Err(LoopError::Capacity));
        assert_eq!(m.total_ammo(), 1_004_096);
        assert_eq!(m.capacity(), 4096);
        assert_eq!(m.remove_reserve(4095), Ok(()));
        assert_eq!(m.set_capacity(1), Ok(4095));
        assert_eq!((m.loaded(), m.reserve()), (1, 1_000_000));
        assert_eq!(m.total_ammo(), 1_000_001);
        assert_eq!(m.set_capacity(4096), Ok(0));
        assert_eq!(m.total_ammo(), 1_000_001);
        assert_eq!(m.loaded(), 1);

        let mut empty_reserve = Magazine::new(2, 0, 1.).unwrap();
        assert_eq!(empty_reserve.add_reserve(1_000_000), Ok(()));
        assert_eq!(empty_reserve.add_reserve(1), Err(LoopError::Capacity));
        assert_eq!(empty_reserve.total_ammo(), 1_000_002);
    }

    #[test]
    fn reserve_edits_update_projection_preserve_timing_and_conserve_transfers() {
        let mut m = Magazine::new(6, 2, 2.).unwrap();
        for _ in 0..5 {
            assert!(m.fire());
        }
        assert!(m.reload());
        assert!(!m.advance(0.5).unwrap());
        let initial = m.reload_status().unwrap();
        assert_eq!(initial.rounds, 2);
        let total = m.total_ammo();
        assert_eq!(m.add_reserve(5), Ok(()));
        assert_eq!(m.total_ammo(), total + 5);
        assert_eq!(
            m.reload_status(),
            Some(ReloadStatus {
                rounds: 5,
                ..initial
            })
        );
        assert_eq!(m.remove_reserve(4), Ok(()));
        assert_eq!(m.total_ammo(), total + 1);
        assert_eq!(
            m.reload_status(),
            Some(ReloadStatus {
                rounds: 3,
                ..initial
            })
        );
        assert!(m.advance(1.5).unwrap());
        assert_eq!((m.loaded(), m.reserve(), m.total_ammo()), (4, 0, total + 1));
        assert!(!m.advance(2.).unwrap());
    }

    #[test]
    fn draining_reserve_cancels_and_returning_ammo_requires_a_fresh_reload() {
        let mut m = Magazine::new(3, 5, 1.).unwrap();
        assert!(m.fire());
        assert!(m.reload());
        assert!(!m.advance(0.75).unwrap());
        assert_eq!(m.remove_reserve(5), Ok(()));
        assert_eq!((m.loaded(), m.reserve(), m.total_ammo()), (2, 0, 2));
        assert_eq!(m.reload_status(), None);
        assert!(!m.reloading());
        assert!(m.can_fire());
        assert!(!m.can_reload());
        assert!(!m.advance(1.).unwrap());
        assert_eq!(m.add_reserve(5), Ok(()));
        assert!(m.can_reload());
        assert!(!m.advance(1.).unwrap());
        assert!(m.reload());
        assert_eq!(m.reload_status().unwrap().progress, 0.);
        assert_eq!(m.remaining(), 1.);
        assert!(m.advance(1.).unwrap());
        assert!(!m.advance(1.).unwrap());
        assert_eq!((m.loaded(), m.reserve(), m.total_ammo()), (3, 4, 7));
    }

    #[test]
    fn cancellation_discards_progress_and_catch_up_completes_only_once() {
        let mut m = Magazine::new(3, 5, 1.).unwrap();
        assert!(m.fire());
        assert!(m.reload());
        assert!(!m.advance(0.75).unwrap());
        let total = m.total_ammo();
        m.cancel_reload();
        m.cancel_reload();
        assert_eq!(m.reload_status(), None);
        assert_eq!(m.total_ammo(), total);
        assert!(!m.advance(1.).unwrap());
        assert!(m.reload());
        assert_eq!(m.reload_status().unwrap().progress, 0.);
        let mut catch_up = m;
        for progress in [0.25, 0.5, 0.75] {
            assert!(!m.advance(0.25).unwrap());
            assert_eq!(m.reload_status().unwrap().progress, progress);
        }
        assert!(m.advance(0.25).unwrap());
        assert!(catch_up.advance(3600.).unwrap());
        assert_eq!(state(&m), state(&catch_up));
        assert_eq!(m.total_ammo(), total);
        assert!(!m.advance(0.25).unwrap());
        assert!(!catch_up.advance(3600.).unwrap());
    }

    #[test]
    fn status_is_finite_at_supported_duration_extremes() {
        for duration in [f32::from_bits(1), f32::MIN_POSITIVE, 0.000_001, 3600.] {
            let mut m = Magazine::new(1, 1, duration).unwrap();
            assert!(m.fire());
            assert!(m.reload());
            assert_eq!(
                m.reload_status(),
                Some(ReloadStatus {
                    duration_seconds: duration,
                    remaining_seconds: duration,
                    progress: 0.,
                    rounds: 1,
                })
            );
            assert!(m.advance(duration).unwrap());
            assert_eq!(m.reload_status(), None);
            assert_eq!((m.loaded(), m.reserve()), (1, 0));
            assert!(!m.advance(duration).unwrap());
        }
    }
}
