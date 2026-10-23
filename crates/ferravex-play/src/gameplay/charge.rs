//! Hold/release actions with minimum commitment, capped power and recovery time.
//!
//! [`ChargeAction::step`] retains the original held-only behavior.
//! [`ChargeAction::step_edges`] also preserves aggregate press/release flags,
//! including release/repress with a final held level. These flags contain no
//! sub-tick timestamps; the helper uses a documented duration attribution, not
//! reconstructed physical timing. Call once per actually executed simulation
//! tick, sharing one consumed input snapshot across systems. Charge releases are
//! requests: the game must accept costs/actions before emitting feedback.
use super::{LoopError, seconds};

/// The single release resolution from an edge-aware step.
///
/// See [`ChargeAction::step_edges`] for a compiling example and ordering rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChargeRelease {
    /// No charge was released (including an ignored tap during recovery).
    None,
    /// A charge ended below the minimum, including its `1e-7` second tolerance.
    /// No recovery starts and no action is requested.
    CanceledTooShort,
    /// A release request with normalized, capped power in `0.0..=1.0`.
    /// This does not mean the game accepted an action or spent resources.
    Emitted(f32),
}

/// A release resolution and the charge state **after** the same snapshot.
///
/// An old release can coexist with a new charge when recovery is zero or the
/// old charge was too short. See [`ChargeAction::step_edges`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChargeStep {
    /// At most one resolution, even if the snapshot aggregates many transitions.
    pub release: ChargeRelease,
    /// Whether a charge remains active after this step.
    pub charging: bool,
    /// Current charge power, not the power of an old emitted release.
    pub power: f32,
    /// Remaining recovery in simulation seconds after this step.
    pub cooldown_seconds: f32,
}

/// A fixed-size charge state machine; stepping does not allocate or queue input.
#[derive(Clone, Copy, Debug)]
pub struct ChargeAction {
    minimum: f64,
    maximum: f64,
    recovery: f64,
    held: f64,
    cooldown: f64,
    charging: bool,
}
impl ChargeAction {
    pub fn new(minimum: f32, maximum: f32, recovery: f32) -> Result<Self, LoopError> {
        let minimum = seconds(minimum)?;
        let maximum = seconds(maximum)?;
        let recovery = seconds(recovery)?;
        if maximum == 0. || minimum > maximum {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            minimum,
            maximum,
            recovery,
            held: 0.,
            cooldown: 0.,
            charging: false,
        })
    }
    pub fn power(&self) -> f32 {
        (self.held / self.maximum) as f32
    }
    pub fn charging(&self) -> bool {
        self.charging
    }
    /// Remaining recovery in simulation seconds. Reading does not advance time.
    /// See [`Self::step_edges`] for an example.
    pub fn cooldown_seconds(&self) -> f32 {
        self.cooldown as f32
    }
    /// Discards charge progress without a release, preserving recovery.
    /// A subsequent held sample may start charging again when recovery ends.
    pub fn cancel(&mut self) {
        self.held = 0.;
        self.charging = false;
    }
    /// Discards charge and recovery, preserving constructor configuration.
    ///
    /// Also clear pending host input on restart; this helper owns no input map.
    ///
    /// ```
    /// use ferravex_play::gameplay::charge::ChargeAction;
    /// let mut charge = ChargeAction::new(0.25, 1.0, 0.5)?;
    /// charge.step(1.0, true)?;
    /// charge.step(0.0, false)?;
    /// charge.cancel(); // Does not bypass recovery.
    /// assert_eq!(charge.cooldown_seconds(), 0.5);
    /// charge.reset();
    /// assert_eq!(charge.cooldown_seconds(), 0.0);
    /// assert!(!charge.charging());
    /// assert_eq!(charge.power(), 0.0);
    /// # Ok::<(), ferravex_play::gameplay::LoopError>(())
    /// ```
    pub fn reset(&mut self) {
        self.cancel();
        self.cooldown = 0.;
    }
    /// A release emits at most one normalized power value. Continued holds cap
    /// at full charge. Hosts commit resource costs only for returned releases.
    pub fn step(&mut self, dt: f32, held: bool) -> Result<Option<f32>, LoopError> {
        let dt = seconds(dt)?;
        let active_dt = (dt - self.cooldown).max(0.);
        self.cooldown = (self.cooldown - dt).max(0.);
        if held && self.cooldown == 0. {
            self.charging = true;
            self.held = (self.held + active_dt).min(self.maximum);
            return Ok(None);
        }
        if !held && self.charging {
            let output = (self.held + 1e-7 >= self.minimum).then(|| self.power());
            self.cancel();
            if output.is_some() {
                self.cooldown = self.recovery;
            }
            return Ok(output);
        }
        Ok(None)
    }

    /// Steps one aggregate snapshot, retaining held, pressed and released
    /// independently. All eight flag combinations are accepted. Invalid `dt`
    /// (nonfinite or outside `0.0..=3600.0`) returns [`LoopError::Invalid`]
    /// **before any mutation**.
    ///
    /// Deterministic processing order and duration attribution:
    ///
    /// 1. Age **preexisting** recovery by `dt`. Only its nonnegative remainder
    ///    `max(dt - old_cooldown, 0)` is available for held charging.
    /// 2. If already charging and `released || !held`, resolve that old charge
    ///    using only previously accumulated duration, with no current `dt`.
    ///    Meeting `minimum` within the legacy `1e-7` second tolerance emits its
    ///    capped power and starts a **fresh**, unaged recovery. Otherwise report
    ///    [`ChargeRelease::CanceledTooShort`] without starting recovery.
    /// 3. If finally held and recovery is zero, start/continue charging with the
    ///    available duration. Thus release/repress (`held && pressed && released`)
    ///    may return an old release plus a new charge. Positive fresh recovery
    ///    blocks that new charge for this entire snapshot, even for large `dt`.
    /// 4. If finally unheld, `pressed` and no old charge was resolved, a ready
    ///    action resolves a **zero-duration tap**. This includes both edges with
    ///    `held == false`, and also pressed-only with final unheld. A positive
    ///    minimum exceeding the tolerance cancels it; minimum zero emits zero
    ///    power. No second tap is resolved after closing an old charge.
    ///
    /// A held level can start a charge without `pressed` (including when recovery
    /// expires exactly at the end of `dt`, with zero progress). `pressed` alone
    /// never resets an existing held charge. `released` with no existing charge
    /// is ignored; a final held level can still start one. Zero `dt` still
    /// processes edges and minimum/recovery rules. Preexisting recovery expiring
    /// within `dt` admits a final hold with only the remainder, or a zero-duration
    /// tap with no elapsed charge credit. Fresh recovery is never aged here.
    ///
    /// With `pressed == released == false`, emitted values and final state match
    /// [`Self::step`] exactly. Extra transitions aggregated into the same flags
    /// cannot be counted or timed. The caller must consume edges only once on an
    /// actual tick; replaying the same edge snapshot is a new request.
    /// [`Self::cancel`] drops progress but keeps recovery; [`Self::reset`] clears
    /// both. Neither clears pending host input.
    ///
    /// ```
    /// use ferravex_play::gameplay::charge::{ChargeAction, ChargeRelease};
    /// let mut charge = ChargeAction::new(0.25, 1.0, 0.0)?;
    /// charge.step_edges(0.75, true, true, false)?;
    /// let report = charge.step_edges(0.25, true, true, true)?;
    /// assert_eq!(report.release, ChargeRelease::Emitted(0.75));
    /// assert!(report.charging); // A new charge, not the old released charge.
    /// assert_eq!(report.power, 0.25);
    /// assert_eq!(report.cooldown_seconds, charge.cooldown_seconds());
    /// let report = charge.step_edges(0.0, false, false, true)?;
    /// assert_eq!(report.release, ChargeRelease::Emitted(0.25));
    /// assert!(!report.charging);
    /// assert_eq!(report.power, 0.0);
    /// # Ok::<(), ferravex_play::gameplay::LoopError>(())
    /// ```
    pub fn step_edges(
        &mut self,
        dt: f32,
        held: bool,
        pressed: bool,
        released: bool,
    ) -> Result<ChargeStep, LoopError> {
        let dt = seconds(dt)?;
        let active_dt = (dt - self.cooldown).max(0.);
        self.cooldown = (self.cooldown - dt).max(0.);
        let close_old = self.charging && (released || !held);
        let tap = !held && pressed && !close_old && self.cooldown == 0.;
        let release = if close_old || tap {
            let output = (self.held + 1e-7 >= self.minimum).then(|| self.power());
            self.cancel();
            if let Some(power) = output {
                self.cooldown = self.recovery;
                ChargeRelease::Emitted(power)
            } else {
                ChargeRelease::CanceledTooShort
            }
        } else {
            ChargeRelease::None
        };
        if held && self.cooldown == 0. {
            self.charging = true;
            self.held = (self.held + active_dt).min(self.maximum);
        }
        Ok(ChargeStep {
            release,
            charging: self.charging,
            power: self.power(),
            cooldown_seconds: self.cooldown_seconds(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_taps_cancel_full_charge_caps_and_release_is_single_use() {
        let mut c = ChargeAction::new(0.2, 1., 0.5).unwrap();
        c.step(0.1, true).unwrap();
        assert_eq!(c.step(0., false).unwrap(), None);
        c.step(2., true).unwrap();
        assert_eq!(c.power(), 1.);
        assert_eq!(c.step(0., false).unwrap(), Some(1.));
        assert_eq!(c.step(0., false).unwrap(), None);
        c.step(0.25, true).unwrap();
        assert!(!c.charging());
        c.step(0.5, true).unwrap();
        assert!((c.power() - 0.25).abs() < 1e-6);
        assert!(c.step(f32::NAN, false).is_err());
        assert!(c.charging());
        c.cancel();
        assert_eq!(c.power(), 0.);
    }
}
