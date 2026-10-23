//! Frame-independent scalar and angular presentation smoothing.
//!
//! [`ExponentialSmoother`](crate::smoothing::ExponentialSmoother) retains its scalar interpolation and permissive update
//! behavior. [`AngularSmoother`](crate::smoothing::AngularSmoother) adds checked, shortest-arc interpolation in
//! radians using the same exponential response. Both use platform exp, so they
//! are intended for cameras/UI/audio controls, not cross-platform lockstep state.
//! Time comes entirely from the caller; no wall clock or pending time is stored.

/// Scalar exponential smoothing with a positive half-life in seconds.
#[derive(Clone, Copy, Debug)]
pub struct ExponentialSmoother {
    value: f64,
    half_life: f64,
}
impl ExponentialSmoother {
    pub fn new(value: f64, half_life_seconds: f64) -> Option<Self> {
        if !value.is_finite() || !half_life_seconds.is_finite() || half_life_seconds <= 0. {
            return None;
        }
        Some(Self {
            value,
            half_life: half_life_seconds,
        })
    }
    /// Invalid inputs leave the current value unchanged. Large deltas converge to target.
    pub fn update(&mut self, target: f64, seconds: f64) -> f64 {
        if !target.is_finite() || !seconds.is_finite() || seconds <= 0. {
            return self.value;
        }
        let alpha = -(-std::f64::consts::LN_2 * seconds / self.half_life).exp_m1();
        self.value = self.value * (1. - alpha) + target * alpha;
        self.value
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn snap(&mut self, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        self.value = value;
        true
    }
}

/// Invalid angular smoothing input. Rejected operations leave state unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AngularSmoothingError {
    /// An initial, target, or snapped angle is NaN or infinite.
    NonFiniteAngle,
    /// The half-life is not finite and strictly positive.
    InvalidHalfLife,
    /// Elapsed seconds are not finite and nonnegative. Zero is valid.
    InvalidDelta,
}

impl std::fmt::Display for AngularSmoothingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFiniteAngle => "angle must be finite radians",
            Self::InvalidHalfLife => "half-life must be finite and positive seconds",
            Self::InvalidDelta => "elapsed seconds must be finite and nonnegative",
        })
    }
}

impl std::error::Error for AngularSmoothingError {}

/// Allocation-free shortest-arc smoothing backed by [`ExponentialSmoother`].
///
/// All angles are radians, normalized to `[-PI, PI)`. At exactly half a turn the
/// negative direction wins. Values can jump numerically at the wrap boundary,
/// while the represented orientation remains continuous. This tracks orientation,
/// not accumulated revolutions; a target more than one turn away is wrapped.
/// Very large input angles have only the precision of the supplied `f64`.
///
/// Use [`Self::snap`] on teleport/restart to reset the current angle immediately.
/// There is no cached target, velocity, or elapsed time to clear.
#[derive(Clone, Copy, Debug)]
pub struct AngularSmoother {
    scalar: ExponentialSmoother,
}

impl AngularSmoother {
    /// Start at a finite angle with a finite, strictly positive half-life.
    ///
    /// Checks the angle before the half-life. Finite angles outside the canonical
    /// interval are wrapped, including angles with magnitude up to `f64::MAX`.
    ///
    /// ```
    /// use ferravex_engine::smoothing::{AngularSmoother, AngularSmoothingError};
    /// use std::f64::consts::PI;
    ///
    /// let angle = AngularSmoother::new(3.0 * PI, 0.25)?;
    /// assert_eq!(angle.value(), -PI);
    /// assert_eq!(AngularSmoother::new(0.0, 0.0).unwrap_err(),
    ///            AngularSmoothingError::InvalidHalfLife);
    /// # Ok::<(), AngularSmoothingError>(())
    /// ```
    pub fn new(value_radians: f64, half_life_seconds: f64) -> Result<Self, AngularSmoothingError> {
        if !value_radians.is_finite() {
            return Err(AngularSmoothingError::NonFiniteAngle);
        }
        let scalar = ExponentialSmoother::new(wrap_radians(value_radians), half_life_seconds)
            .ok_or(AngularSmoothingError::InvalidHalfLife)?;
        Ok(Self { scalar })
    }

    /// Move toward a finite target along the shortest arc and return the angle.
    ///
    /// Checks the target before elapsed seconds, even for a zero-duration update.
    /// Non-finite targets, non-finite deltas, and negative deltas return an error
    /// without changing state. Zero seconds (including negative zero) return the
    /// current value exactly, as does a target equal to the current wrapped angle.
    /// Large finite deltas converge to the wrapped target; no catch-up time is
    /// retained. A half-life removes half the angular error, up to floating-point
    /// rounding.
    ///
    /// ```
    /// use ferravex_engine::smoothing::{AngularSmoother, AngularSmoothingError};
    /// use std::f64::consts::PI;
    ///
    /// let mut angle = AngularSmoother::new(170.0_f64.to_radians(), 0.5)?;
    /// let midpoint = angle.update((-170.0_f64).to_radians(), 0.5)?;
    /// assert!((midpoint.abs() - PI).abs() < 1e-12); // Across the wrap boundary.
    /// assert_eq!(angle.update(0.0, 0.0)?, midpoint);
    /// assert_eq!(angle.update(f64::NAN, 1.0), Err(AngularSmoothingError::NonFiniteAngle));
    /// assert_eq!(angle.value(), midpoint);
    /// # Ok::<(), AngularSmoothingError>(())
    /// ```
    pub fn update(
        &mut self,
        target_radians: f64,
        seconds: f64,
    ) -> Result<f64, AngularSmoothingError> {
        if !target_radians.is_finite() {
            return Err(AngularSmoothingError::NonFiniteAngle);
        }
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(AngularSmoothingError::InvalidDelta);
        }
        if seconds == 0.0 {
            return Ok(self.value());
        }

        // Reduce before subtracting to avoid overflowing for extreme inputs.
        let target = wrap_radians(target_radians);
        let current = self.value();
        if target == current {
            return Ok(current);
        }
        let delta = wrap_radians(target - current);
        let nearest_target = current + delta;
        let next = self.scalar.update(nearest_target, seconds);
        // Restore the exact wrapped target at convergence: current + delta can
        // lose a tiny target to cancellation, even when alpha has reached one.
        self.scalar.snap(if next == nearest_target {
            target
        } else {
            wrap_radians(next)
        });
        Ok(self.value())
    }

    /// Read the current angle in radians, always in `[-PI, PI)`.
    ///
    /// ```
    /// use ferravex_engine::smoothing::AngularSmoother;
    /// let angle = AngularSmoother::new(350.0_f64.to_radians(), 0.1)?;
    /// assert!((angle.value().to_degrees() + 10.0).abs() < 1e-12);
    /// # Ok::<(), ferravex_engine::smoothing::AngularSmoothingError>(())
    /// ```
    pub fn value(&self) -> f64 {
        self.scalar.value()
    }

    /// Immediately reset to a finite angle while retaining the half-life.
    ///
    /// The angle is wrapped to `[-PI, PI)`. Non-finite input returns an error
    /// without changing state. Subsequent updates behave like a fresh smoother
    /// constructed at this angle with the original half-life.
    ///
    /// ```
    /// use ferravex_engine::smoothing::{AngularSmoother, AngularSmoothingError};
    /// use std::f64::consts::PI;
    ///
    /// let mut angle = AngularSmoother::new(0.0, 0.5)?;
    /// angle.update(1.0, 0.1)?;
    /// angle.snap(3.0 * PI)?; // Reset orientation after teleport/restart.
    /// assert_eq!(angle.value(), -PI);
    /// assert_eq!(angle.snap(f64::INFINITY), Err(AngularSmoothingError::NonFiniteAngle));
    /// assert_eq!(angle.value(), -PI);
    /// # Ok::<(), AngularSmoothingError>(())
    /// ```
    pub fn snap(&mut self, value_radians: f64) -> Result<(), AngularSmoothingError> {
        if !value_radians.is_finite() {
            return Err(AngularSmoothingError::NonFiniteAngle);
        }
        self.scalar.snap(wrap_radians(value_radians));
        Ok(())
    }
}

fn wrap_radians(radians: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    // A signed remainder avoids losing tiny angles by adding PI or TAU first.
    let wrapped = radians % TAU;
    if wrapped >= PI {
        wrapped - TAU
    } else if wrapped < -PI {
        wrapped + TAU
    } else {
        wrapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    #[test]
    fn half_life_is_independent_of_frame_partition() {
        for fps in [30, 60, 144] {
            let mut s = ExponentialSmoother::new(0., 1.).unwrap();
            for _ in 0..fps {
                s.update(10., 1. / f64::from(fps));
            }
            assert!((s.value() - 5.).abs() < 1e-12);
        }
        let mut s = ExponentialSmoother::new(-f64::MAX, 1.).unwrap();
        assert!(s.update(f64::MAX, 1.).is_finite());
        let before = s.value();
        assert_eq!(s.update(f64::NAN, 1.), before);
        assert_eq!(s.update(0., f64::INFINITY), before);
        assert!(!s.snap(f64::NAN));
        assert!(s.snap(3.));
        assert_eq!(s.update(10., 1e10), 10.);
        assert!(ExponentialSmoother::new(0., 0.).is_none());
    }

    #[test]
    fn scalar_defaults_remain_linear_and_permissive() {
        let mut scalar = ExponentialSmoother::new(170.0, 1.0).unwrap();
        assert_eq!(scalar.update(-170.0, 1.0), 0.0);
        for seconds in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(scalar.update(10.0, seconds), 0.0);
        }
        assert!(scalar.snap(350.0));
        assert_eq!(scalar.value(), 350.0);
    }

    #[test]
    fn angular_construction_rejects_invalid_values() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                AngularSmoother::new(value, 1.0).unwrap_err(),
                AngularSmoothingError::NonFiniteAngle
            );
        }
        for half_life in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                AngularSmoother::new(0.0, half_life).unwrap_err(),
                AngularSmoothingError::InvalidHalfLife
            );
        }
        assert_eq!(
            AngularSmoother::new(f64::NAN, 0.0).unwrap_err(),
            AngularSmoothingError::NonFiniteAngle
        );
        for half_life in [f64::from_bits(1), f64::MIN_POSITIVE, 0.5, f64::MAX] {
            assert!(AngularSmoother::new(0.0, half_life).is_ok());
        }
    }

    #[test]
    fn angular_values_are_normalized_without_losing_tiny_angles() {
        for (input, expected) in [
            (PI, -PI),
            (-PI, -PI),
            (3.0 * PI, -PI),
            (-3.0 * PI, -PI),
            (TAU, 0.0),
            (-TAU, 0.0),
            (0.0, 0.0),
            (-0.0, -0.0),
            (f64::from_bits(1), f64::from_bits(1)),
            (-f64::from_bits(1), -f64::from_bits(1)),
        ] {
            let mut angle = AngularSmoother::new(input, 1.0).unwrap();
            assert_eq!(angle.value(), expected);
            angle.snap(0.25).unwrap();
            angle.snap(input).unwrap();
            assert_eq!(angle.value(), expected);
        }
        for input in [-f64::MAX, f64::MAX, -1e100, 1e100] {
            let mut angle = AngularSmoother::new(input, 1.0).unwrap();
            assert!((-PI..PI).contains(&angle.value()));
            angle.snap(-input).unwrap();
            assert!((-PI..PI).contains(&angle.value()));
        }
    }

    #[test]
    fn angular_updates_cross_wraparound_in_both_directions() {
        for (start, target, expected) in [
            (170.0_f64, -170.0_f64, -180.0_f64),
            (-170.0, 170.0, -180.0),
            (350.0, 10.0, 0.0),
            (10.0, 350.0, 0.0),
        ] {
            let mut angle = AngularSmoother::new(start.to_radians(), 0.25).unwrap();
            let next = angle.update(target.to_radians(), 0.25).unwrap();
            assert!(wrap_radians(next - expected.to_radians()).abs() < 1e-12);
            assert!((-PI..PI).contains(&next));
            let remaining = wrap_radians(target.to_radians() - next).abs();
            let initial = wrap_radians(target.to_radians() - start.to_radians()).abs();
            assert!((remaining - initial * 0.5).abs() < 1e-12);
        }
    }

    #[test]
    fn antipodal_targets_consistently_take_the_negative_direction() {
        for target in [PI, -PI, 3.0 * PI, -3.0 * PI] {
            let mut angle = AngularSmoother::new(0.0, 1.0).unwrap();
            assert_eq!(angle.update(target, 1.0), Ok(-FRAC_PI_2));
        }
        let mut angle = AngularSmoother::new(-PI, 1.0).unwrap();
        assert_eq!(angle.update(0.0, 1.0), Ok(FRAC_PI_2));
    }

    #[test]
    fn angular_half_life_is_independent_of_frame_partition() {
        for (start, target) in [(3.0, -2.5), (-3.0, 2.5), (0.0, PI)] {
            let mut single = AngularSmoother::new(start, 0.5).unwrap();
            single.update(target, 1.0).unwrap();
            for fps in [30, 60, 144] {
                let mut partitioned = AngularSmoother::new(start, 0.5).unwrap();
                for _ in 0..fps {
                    partitioned.update(target, 1.0 / f64::from(fps)).unwrap();
                }
                assert!(wrap_radians(partitioned.value() - single.value()).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn zero_delta_preserves_bits_and_does_not_retain_a_target() {
        for start in [-0.0, 3.0, -PI, -f64::from_bits(1)] {
            let mut angle = AngularSmoother::new(start, 0.25).unwrap();
            let mut untouched = angle;
            for seconds in [0.0, -0.0] {
                let next = angle.update(f64::MAX, seconds).unwrap();
                assert_eq!(next.to_bits(), untouched.value().to_bits());
                assert_eq!(angle.value().to_bits(), untouched.value().to_bits());
            }
            assert_eq!(angle.update(0.25, 0.5), untouched.update(0.25, 0.5));
        }
    }

    #[test]
    fn stationary_targets_do_not_accumulate_rounding_drift() {
        for start in [-PI, -3.0, -0.0, 0.7, 1.1, 2.1, 3.0] {
            let mut angle = AngularSmoother::new(start, 1.0).unwrap();
            for seconds in [1e-15, 0.001, 0.25, 1.0, f64::MAX] {
                assert_eq!(
                    angle.update(start, seconds).unwrap().to_bits(),
                    start.to_bits()
                );
                assert_eq!(angle.value().to_bits(), start.to_bits());
            }
        }
    }

    #[test]
    fn large_delta_converges_exactly_even_with_cancellation_or_extreme_inputs() {
        for half_life in [f64::from_bits(1), 0.1, 1.0] {
            for target in [-f64::MAX, f64::MAX, -PI, 0.25, 1e-300, -1e-300] {
                let mut angle = AngularSmoother::new(3.0, half_life).unwrap();
                assert_eq!(angle.update(target, f64::MAX), Ok(wrap_radians(target)));
            }
        }
        // A large absolute dt need not converge if the half-life is also large.
        let mut angle = AngularSmoother::new(0.0, f64::MAX).unwrap();
        assert!((angle.update(2.0, f64::MAX).unwrap() - 1.0).abs() < 1e-12);
        for (start, target) in [(f64::MAX, -f64::MAX), (-f64::MAX, f64::MAX)] {
            let mut angle = AngularSmoother::new(start, 1.0).unwrap();
            assert!((-PI..PI).contains(&angle.update(target, 1.0).unwrap()));
        }
    }

    #[test]
    fn invalid_updates_and_snaps_preserve_all_state() {
        let mut angle = AngularSmoother::new(3.0, 0.25).unwrap();
        angle.update(-3.0, 0.1).unwrap();
        let mut untouched = angle;
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for seconds in [0.0, 1.0, -1.0] {
                assert_eq!(
                    angle.update(value, seconds),
                    Err(AngularSmoothingError::NonFiniteAngle)
                );
                assert_eq!(angle.value().to_bits(), untouched.value().to_bits());
            }
            assert_eq!(
                angle.snap(value),
                Err(AngularSmoothingError::NonFiniteAngle)
            );
            assert_eq!(angle.value().to_bits(), untouched.value().to_bits());
        }
        for seconds in [
            -1.0,
            -f64::from_bits(1),
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(
                angle.update(0.0, seconds),
                Err(AngularSmoothingError::InvalidDelta)
            );
            assert_eq!(angle.value().to_bits(), untouched.value().to_bits());
        }
        assert_eq!(angle.update(0.5, 0.125), untouched.update(0.5, 0.125));
    }

    #[test]
    fn snap_resets_to_fresh_behavior_with_the_original_half_life() {
        let mut angle = AngularSmoother::new(0.0, 0.25).unwrap();
        angle.update(-2.0, 0.1).unwrap();
        angle.snap(3.0 * PI).unwrap();
        let mut fresh = AngularSmoother::new(3.0 * PI, 0.25).unwrap();
        assert_eq!(angle.value(), fresh.value());
        for (target, seconds) in [(1.0, 0.0), (2.5, 0.25), (-2.5, 1.0), (0.1, 1e10)] {
            assert_eq!(angle.update(target, seconds), fresh.update(target, seconds));
        }
    }
}
