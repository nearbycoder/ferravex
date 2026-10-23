//! Poise accumulation, finite stagger windows and opt-in recovery resistance.
//!
//! [`Stagger::new`] retains immediate re-stagger after recovery. Opt in through
//! [`Stagger::with_recovery_policy`]; grace rejects poise hits entirely, including
//! zero hits, without restarting either timer. Damage admission remains host-owned.
//! Configuration is immutable: construct a replacement to change tuning. Reset
//! clears runtime state while retaining tuning and does not emit transitions.
//!
//! # Accepted combat admission recipe
//!
//! This headless recipe uses the same first-party [`ActionTimeline`](crate::action::ActionTimeline)
//! interrupt mechanism as Neon's heavy enemies. Call it only for an admitted
//! combat hit, once per actual simulation tick/event. It does not admit damage,
//! perform collision queries, or implement a renderer. Neon retains its authored
//! zero-grace default; a tougher optional loadout can opt in after balance review.
//!
//! ```
//! use ferravex_play::action::{ActionPhase, ActionSpec, ActionTimeline};
//! use ferravex_play::gameplay::stagger::{
//!     Stagger, StaggerHit, StaggerPhase, StaggerRecoveryPolicy,
//! };
//! let mut attack = ActionTimeline::new(ActionSpec {
//!     windup: 0.375, active: 0.0625, recovery: 1.0,
//!     cancel_before: 0.25, buffer_seconds: 0.1,
//! }).unwrap();
//! let policy = StaggerRecoveryPolicy::reject_during_grace(0.25).unwrap();
//! let mut stagger = Stagger::with_recovery_policy(40., 10., 0.5, policy).unwrap();
//! attack.step(0.125, true).unwrap();
//! for admitted_poise in [24., 24.] {
//!     let report = stagger.hit_report(admitted_poise).unwrap();
//!     if report == StaggerHit::Onset {
//!         assert!(attack.cancel(true)); // Only accepted onset interrupts.
//!     }
//! }
//! assert_eq!(attack.phase(), ActionPhase::Idle);
//! let recovery = stagger.advance_report(0.5).unwrap();
//! assert_eq!(recovery.recovered_at, Some(0.5));
//! attack.step(0.125, true).unwrap(); // Recovery allows attacks during grace.
//! assert_eq!(stagger.hit_report(100.).unwrap(), StaggerHit::RejectedGrace);
//! assert_eq!(attack.phase(), ActionPhase::Windup); // No repeat interruption.
//! // Render a steady outline plus "Resisting" text while this is true;
//! // no flash, motion or essential color-only signal is required.
//! let resisting_cue = stagger.phase() == StaggerPhase::Grace;
//! assert!(resisting_cue);
//! assert_eq!(stagger.grace_remaining(), 0.25);
//! assert_eq!(stagger.stagger_remaining(), 0.0);
//! assert_eq!(stagger.recovery_policy().grace_seconds(), 0.25);
//! assert_eq!(stagger.advance_report(0.25).unwrap().grace_ended_at, Some(0.25));
//! assert_eq!(stagger.advance_report(0.0).unwrap().recovered_at, None);
//! stagger.reset();
//! assert_eq!(stagger.phase(), StaggerPhase::Idle);
//! assert_eq!(stagger.buildup(), 0.0);
//! ```
use super::{LoopError, seconds};

/// Immutable recovery policy. Default is zero grace (the legacy rule).
/// During positive grace, poise hits are rejected rather than accumulated.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StaggerRecoveryPolicy {
    grace: f64,
}
impl StaggerRecoveryPolicy {
    /// Validate a finite grace duration in `0..=3600` simulation seconds.
    /// Zero disables grace. No existing stagger is mutated on failure.
    ///
    /// ```
    /// use ferravex_play::gameplay::stagger::StaggerRecoveryPolicy;
    /// assert_eq!(StaggerRecoveryPolicy::default().grace_seconds(), 0.0);
    /// assert!(StaggerRecoveryPolicy::reject_during_grace(f32::NAN).is_err());
    /// let policy = StaggerRecoveryPolicy::reject_during_grace(0.25).unwrap();
    /// assert_eq!(policy.grace_seconds(), 0.25);
    /// ```
    pub fn reject_during_grace(grace_seconds: f32) -> Result<Self, LoopError> {
        Ok(Self {
            grace: seconds(grace_seconds)?,
        })
    }

    /// Authored grace seconds, represented exactly from the supplied `f32`.
    pub fn grace_seconds(&self) -> f64 {
        self.grace
    }
}

/// Current simulation phase. Grace permits action, but rejects poise hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaggerPhase {
    Idle,
    Staggered,
    Grace,
}

/// Immediate outcome of one validated poise hit; never queued for later reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaggerHit {
    /// Poise admitted below threshold (including a zero hit while idle).
    Accumulated,
    /// Threshold reached: poise cleared and exactly one stagger started now.
    Onset,
    /// Already staggered: poise and timers unchanged.
    RejectedActive,
    /// Recovery grace: poise and timers unchanged.
    RejectedGrace,
}

/// One advance's boundary crossings; offsets are `f64` simulation seconds from
/// the start of that call, including an exact end-of-step boundary. At most one
/// of each can occur, in recovery then grace-end order. Zero grace has no
/// grace-end event. Later reads/zero steps never replay these events.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StaggerAdvance {
    /// Stagger ended here; positive grace starts at this exact boundary.
    pub recovered_at: Option<f64>,
    /// Positive grace ended here; new threshold hits can stagger again.
    pub grace_ended_at: Option<f64>,
}

/// Copyable, allocation-free poise and timing state. See the module recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stagger {
    threshold: f32,
    decay: f32,
    duration: f64,
    poise: f32,
    remaining: f64,
    policy: StaggerRecoveryPolicy,
    grace_remaining: f64,
}
impl Stagger {
    /// Legacy setup: threshold `0.01..=1e6`, decay `0..=1e6` poise/second,
    /// stagger duration `(0..=3600]` seconds, all finite, with zero grace.
    pub fn new(threshold: f32, decay: f32, duration: f32) -> Result<Self, LoopError> {
        Self::with_recovery_policy(threshold, decay, duration, StaggerRecoveryPolicy::default())
    }

    /// Construct with validated immutable resistance tuning. See the module
    /// recipe. Uses the same bounds as [`Self::new`]; invalid input returns
    /// [`LoopError::Invalid`] without changing any existing instance.
    pub fn with_recovery_policy(
        threshold: f32,
        decay: f32,
        duration: f32,
        policy: StaggerRecoveryPolicy,
    ) -> Result<Self, LoopError> {
        let duration = seconds(duration)?;
        if !threshold.is_finite()
            || !(0.01..=1e6).contains(&threshold)
            || !decay.is_finite()
            || !(0. ..=1e6).contains(&decay)
            || duration == 0.
        {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            threshold,
            decay,
            duration,
            poise: 0.,
            remaining: 0.,
            policy,
            grace_remaining: 0.,
        })
    }
    /// True only while staggered, never during recovery grace.
    pub fn active(&self) -> bool {
        self.remaining > 0.
    }
    /// Current phase without consuming an event or advancing time.
    pub fn phase(&self) -> StaggerPhase {
        if self.active() {
            StaggerPhase::Staggered
        } else if self.grace_remaining > 0. {
            StaggerPhase::Grace
        } else {
            StaggerPhase::Idle
        }
    }

    /// Remaining active stagger seconds; zero during idle/grace.
    pub fn stagger_remaining(&self) -> f64 {
        self.remaining
    }

    /// Remaining current grace seconds; zero while staggered (not yet started).
    pub fn grace_remaining(&self) -> f64 {
        self.grace_remaining
    }

    /// Immutable authored policy, including pending grace after an active stagger.
    pub fn recovery_policy(&self) -> StaggerRecoveryPolicy {
        self.policy
    }

    /// Clear poise and both timers, retaining all tuning. Emits no transition.
    pub fn reset(&mut self) {
        self.poise = 0.;
        self.remaining = 0.;
        self.grace_remaining = 0.;
    }

    /// Current poise as a fraction of threshold, always in `0..1`.
    pub fn buildup(&self) -> f32 {
        self.poise / self.threshold
    }
    /// A stagger cannot be extended by repeated hits during its active window.
    pub fn hit(&mut self, amount: f32) -> Result<bool, LoopError> {
        Ok(self.hit_report(amount)? == StaggerHit::Onset)
    }

    /// Admit finite poise in `0..=1e6`, or report the current rejection phase.
    /// Validation precedes all phase checks; invalid input changes nothing.
    /// Active/grace hits never accumulate poise or restart timers. Only Onset
    /// should trigger an interrupt or onset feedback. See the module recipe.
    pub fn hit_report(&mut self, amount: f32) -> Result<StaggerHit, LoopError> {
        if !amount.is_finite() || !(0. ..=1e6).contains(&amount) {
            return Err(LoopError::Invalid);
        }
        if self.active() {
            return Ok(StaggerHit::RejectedActive);
        }
        if self.grace_remaining > 0. {
            return Ok(StaggerHit::RejectedGrace);
        }
        self.poise = (self.poise + amount).min(self.threshold);
        if self.poise >= self.threshold {
            self.poise = 0.;
            self.remaining = self.duration;
            Ok(StaggerHit::Onset)
        } else {
            Ok(StaggerHit::Accumulated)
        }
    }
    /// Advance simulation time with the legacy unit result. Transition reports
    /// are discarded; call [`Self::advance_report`] instead when needed.
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        self.advance_report(dt).map(|_| ())
    }

    /// Advance finite `0..=3600` seconds, returning only boundaries crossed now.
    /// Consume stagger remainder, then grace, then idle poise decay, in order.
    /// Grace starts at actual recovery, even inside a large step. No hits or
    /// new onsets are synthesized. Invalid time leaves every field unchanged;
    /// zero time emits no transitions. Offsets use stored `f64` arithmetic;
    /// idle decay retains legacy per-call `f32` poise rounding, so arbitrary
    /// repartitioning need not bit-match. See the module recipe.
    pub fn advance_report(&mut self, dt: f32) -> Result<StaggerAdvance, LoopError> {
        let dt = seconds(dt)?;
        let recovered_at = (self.active() && dt >= self.remaining).then_some(self.remaining);
        let after_stagger = (dt - self.remaining).max(0.);
        self.remaining = (self.remaining - dt).max(0.);
        if recovered_at.is_some() {
            self.grace_remaining = self.policy.grace;
        }
        let grace_ended_at = (self.grace_remaining > 0. && after_stagger >= self.grace_remaining)
            .then_some(recovered_at.unwrap_or(0.) + self.grace_remaining);
        let idle = (after_stagger - self.grace_remaining).max(0.);
        self.grace_remaining = (self.grace_remaining - after_stagger).max(0.);
        self.poise = (f64::from(self.poise) - f64::from(self.decay) * idle).max(0.) as f32;
        Ok(StaggerAdvance {
            recovered_at,
            grace_ended_at,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concentrated_hits_stagger_but_spam_cannot_extend_lockout() {
        let mut s = Stagger::new(10., 2., 0.5).unwrap();
        assert!(!s.hit(6.).unwrap());
        s.advance(1.).unwrap();
        assert!((s.buildup() - 0.4).abs() < 1e-6);
        assert!(s.hit(6.).unwrap());
        s.advance(0.25).unwrap();
        assert!(!s.hit(100.).unwrap());
        s.advance(0.25).unwrap();
        assert!(!s.active());
        assert!(s.hit(10.).unwrap());
        assert!(s.hit(f32::NAN).is_err());
    }
}
