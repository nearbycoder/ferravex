//! Shield absorption and delayed regeneration independent of health/damage types.
//!
//! State uses independent axes, with no phase precedence: an empty shield can
//! be delayed or recharging. A zero maximum is both full and broken. A zero
//! maximum or rate disables recharge, but confirmed positive hits still start
//! the delay and simulation time still consumes it. `recharging` means eligible
//! to recharge; very small advances can round to no stored value change.
//!
//! Advance only on executed simulation ticks. Restart by replacing the shield
//! with [`Shield::new`] using the run's authored configuration. This helper has
//! no attack admission, health, feedback queue, save format, or clock ownership.
//!
//! ```
//! use ferravex_play::gameplay::shield::{Shield, ShieldValuePolicy};
//! let mut shield = Shield::new(20., 8., 4.).unwrap();
//! assert_eq!((shield.maximum(), shield.recharge_rate(), shield.recharge_delay()),
//!            (20., 8., 4.));
//! let hit = shield.absorb_detailed(25.).unwrap(); // After host hit admission.
//! assert_eq!((hit.absorbed, hit.unabsorbed), (20., 5.));
//! assert_eq!(hit.change.value_change(), -20.);
//! assert!(hit.change.depleted());
//! assert!(shield.state().broken && shield.state().delayed);
//! assert_eq!(shield.delay_remaining(), 4.);
//! let waiting = shield.advance_detailed(4.).unwrap();
//! assert!(waiting.after.recharging);
//! assert!(!waiting.recovered()); // Delay expiry alone is not recovery.
//! let recharge = shield.advance_detailed(1.).unwrap();
//! assert_eq!(recharge.value_change(), 8.);
//! assert!(recharge.recovered());
//! shield.reconfigure(40., 8., 4., ShieldValuePolicy::PreserveValue).unwrap();
//! assert_eq!(shield.value(), 8.); // Growth grants no free charge.
//! shield.reconfigure(20., 8., 4., ShieldValuePolicy::PreserveRatio).unwrap();
//! assert_eq!(shield.value(), 4.);
//! assert!(shield.advance_detailed(3600.).unwrap().filled());
//! assert!(!shield.advance_detailed(0.).unwrap().filled());
//! ```
use super::{LoopError, seconds};

/// Explicit charge policy for [`Shield::reconfigure`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShieldValuePolicy {
    /// Keep absolute charge, clamping to the new maximum on shrink. Never heal
    /// on growth.
    PreserveValue,
    /// Scale the old fraction using f64 intermediates, then round to f32 and
    /// cap at the new maximum. An old maximum of zero has fraction zero.
    PreserveRatio,
}

/// Read-only snapshot. Flags are independent; see the module's state contract
/// and executable example. No field is an emitted event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShieldState {
    /// Stored f32 charge.
    pub value: f32,
    /// Exact remaining simulation delay, retaining the helper's f64 timer.
    pub delay_remaining: f64,
    /// Charge equals maximum, including a zero maximum.
    pub full: bool,
    /// Charge is zero, regardless of delay or recharge eligibility.
    pub broken: bool,
    /// A positive live delay remains, even if recharge is disabled.
    pub delayed: bool,
    /// Positive maximum and rate, independent of charge and delay.
    pub recharge_enabled: bool,
    /// Enabled, below full, and no delay. Does not promise a numerical gain on
    /// the next advance, especially for zero dt or sub-ULP increments.
    pub recharging: bool,
}

/// Before/after values for one successful operation, without queued events.
/// Reconfiguration transitions describe charge edits, not gameplay recovery;
/// consume recovery feedback from an accepted `advance_detailed` receipt only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShieldChange {
    pub before: ShieldState,
    pub after: ShieldState,
}
impl ShieldChange {
    /// Signed difference of the two stored f32 charges, promoted before
    /// subtraction to avoid rounding the delta back to f32. The snapshots keep
    /// both endpoints even when their difference itself rounds in f64.
    pub fn value_change(&self) -> f64 {
        f64::from(self.after.value) - f64::from(self.before.value)
    }
    /// A positive stored charge became zero in this call.
    pub fn depleted(&self) -> bool {
        !self.before.broken && self.after.broken
    }
    /// Zero stored charge became positive in this call. Delay expiry without
    /// an actual charge increase does not count.
    pub fn recovered(&self) -> bool {
        self.before.broken && !self.after.broken
    }
    /// Charge actually increased from below full to full in this call.
    /// Capacity shrink alone cannot emit this signal.
    pub fn filled(&self) -> bool {
        !self.before.full && self.after.full && self.after.value > self.before.value
    }
}

/// Damage routing plus actual stored charge change for one accepted hit.
/// `absorbed` uses legacy damage arithmetic and can differ from the magnitude
/// of `change.value_change()` because f32 subtraction rounds. `unabsorbed` is
/// exactly what legacy [`Shield::absorb`] returns for the host's health rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShieldAbsorption {
    pub absorbed: f32,
    pub unabsorbed: f32,
    pub change: ShieldChange,
}

/// Fixed-storage, Copy shield; existing fields and legacy arithmetic retained.
#[derive(Clone, Copy, Debug)]
pub struct Shield {
    maximum: f32,
    value: f32,
    rate: f32,
    delay: f64,
    remaining: f64,
}
impl Shield {
    /// Start full with finite maximum/rate in `0..=1e6`, delay in `0..=3600`
    /// seconds. Invalid input returns [`LoopError::Invalid`]. See module example.
    pub fn new(maximum: f32, rate: f32, delay: f32) -> Result<Self, LoopError> {
        let delay = seconds(delay)?;
        if !maximum.is_finite()
            || !(0. ..=1e6).contains(&maximum)
            || !rate.is_finite()
            || !(0. ..=1e6).contains(&rate)
        {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            maximum,
            value: maximum,
            rate,
            delay,
            remaining: 0.,
        })
    }
    pub fn value(&self) -> f32 {
        self.value
    }
    /// Configured capacity; inspection does not advance the shield.
    pub fn maximum(&self) -> f32 {
        self.maximum
    }
    /// Configured charge per simulation second; zero disables recharge.
    pub fn recharge_rate(&self) -> f32 {
        self.rate
    }
    /// Configured delay for subsequent positive confirmed hits, in seconds.
    pub fn recharge_delay(&self) -> f32 {
        self.delay as f32
    }
    /// Live delay in seconds without reducing the precision of the f64 timer.
    pub fn delay_remaining(&self) -> f64 {
        self.remaining
    }
    /// Inspect independent state axes; see the module example and contract.
    pub fn state(&self) -> ShieldState {
        let full = self.value == self.maximum;
        let delayed = self.remaining > 0.;
        let recharge_enabled = self.maximum > 0. && self.rate > 0.;
        ShieldState {
            value: self.value,
            delay_remaining: self.remaining,
            full,
            broken: self.value == 0.,
            delayed,
            recharge_enabled,
            recharging: recharge_enabled && !full && !delayed,
        }
    }
    /// Atomically validate and replace maximum/rate/delay using [`Self::new`]'s
    /// ranges, then apply the explicit value policy. See the module example.
    ///
    /// The live timer becomes `min(old_remaining, new_delay)`: shorter delays
    /// cap a pending wait, longer delays never extend it, and elapsed waits
    /// never restart. Rate-only and repeated identical edits preserve the wait.
    /// The next positive hit uses the new full delay, even when empty/disabled.
    /// PreserveValue clamps on shrink and never heals on growth. PreserveRatio
    /// may deliberately add charge, with fraction zero for an old zero maximum.
    /// Rejected configurations leave every field unchanged.
    pub fn reconfigure(
        &mut self,
        maximum: f32,
        rate: f32,
        delay: f32,
        policy: ShieldValuePolicy,
    ) -> Result<ShieldChange, LoopError> {
        let mut next = Self::new(maximum, rate, delay)?;
        next.value = match policy {
            ShieldValuePolicy::PreserveValue => self.value.min(maximum),
            ShieldValuePolicy::PreserveRatio if self.maximum == 0. => 0.,
            ShieldValuePolicy::PreserveRatio => ((f64::from(self.value) / f64::from(self.maximum))
                * f64::from(maximum))
            .min(f64::from(maximum)) as f32,
        };
        next.remaining = self.remaining.min(next.delay);
        let before = self.state();
        *self = next;
        Ok(ShieldChange {
            before,
            after: self.state(),
        })
    }
    /// Returns unabsorbed damage for the host's health rules. Zero damage does
    /// not reset recovery; nonzero confirmed hits reset even an empty shield.
    pub fn absorb(&mut self, damage: f32) -> Result<f32, LoopError> {
        self.absorb_detailed(damage).map(|hit| hit.unabsorbed)
    }
    /// Apply an already admitted hit with legacy arithmetic. Finite damage in
    /// `0..=1e6` is accepted; invalid damage changes nothing. Zero never refreshes
    /// delay or emits depletion. Positive damage refreshes even an empty shield.
    /// See the module example. This method does not admit/deduplicate attacks.
    pub fn absorb_detailed(&mut self, damage: f32) -> Result<ShieldAbsorption, LoopError> {
        if !damage.is_finite() || !(0. ..=1e6).contains(&damage) {
            return Err(LoopError::Invalid);
        }
        let before = self.state();
        let absorbed = if damage == 0. {
            0.
        } else {
            self.remaining = self.delay;
            let absorbed = damage.min(self.value);
            self.value -= absorbed;
            absorbed
        };
        Ok(ShieldAbsorption {
            absorbed,
            unabsorbed: if damage == 0. { 0. } else { damage - absorbed },
            change: ShieldChange {
                before,
                after: self.state(),
            },
        })
    }
    /// Advance using legacy delay/recharge arithmetic; see [`Self::advance_detailed`].
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        self.advance_detailed(dt).map(|_| ())
    }
    /// Advance finite simulation seconds in `0..=3600`, consuming delay first.
    /// Invalid dt changes nothing. Reports actual f32 charge change after capping
    /// and rounding; no repeat recovery/full signal on no-op advances. See the
    /// module example. A large step can both recover from zero and fill.
    ///
    /// Each call rounds charge to f32, as before. Partitioning time therefore
    /// has floating-point tolerance, not a bitwise equality guarantee (and very
    /// small increments may all round away). Delay is retained in f64.
    pub fn advance_detailed(&mut self, dt: f32) -> Result<ShieldChange, LoopError> {
        let dt = seconds(dt)?;
        let before = self.state();
        let recovery = (dt - self.remaining).max(0.);
        self.remaining = (self.remaining - dt).max(0.);
        self.value = (f64::from(self.value) + f64::from(self.rate) * recovery)
            .min(f64::from(self.maximum)) as f32;
        Ok(ShieldChange {
            before,
            after: self.state(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn damage_conserves_and_recovery_only_uses_time_after_delay() {
        let mut s = Shield::new(20., 10., 2.).unwrap();
        assert_eq!(s.absorb(30.).unwrap(), 10.);
        let mut partitioned = s;
        s.advance(3.).unwrap();
        for _ in 0..6 {
            partitioned.advance(0.5).unwrap();
        }
        assert_eq!(s.value(), 10.);
        assert_eq!(s.value(), partitioned.value());
        s.absorb(0.).unwrap();
        s.advance(1.).unwrap();
        assert_eq!(s.value(), 20.);
        assert!(s.absorb(f32::NAN).is_err());
        assert_eq!(s.value(), 20.);
        s.absorb(20.).unwrap();
        s.advance(1.).unwrap();
        s.absorb(1.).unwrap();
        s.advance(1.).unwrap();
        assert_eq!(s.value(), 0.);
    }
}
