//! Small, renderer-independent tools for responsive arcade motion and impacts.
//!
//! [`approach_vec2`] and [`approach_vec3`] limit the magnitude of change per
//! second: on velocities this bounds acceleration; on positions it bounds speed.
//! [`damp_vec2`] and [`damp_vec3`] instead decay the remaining error exponentially.
//! All four are allocation-free, accept caller-supplied time and retain no state.
//! Gameplay callers should supply their fixed simulation timestep.
//!
//! With a constant target and rate, splitting time gives the same mathematical
//! result, subject to floating-point rounding on each call. Bounds likewise have
//! `f32` rounding tolerance: very small changes can round away, and subnormal
//! components have coarse relative precision. Platform square root/exponential
//! operations do not establish cross-platform bitwise determinism.
use ferravex_math::{Vec2, Vec3};

/// Invalid input to a checked approach or damping helper.
///
/// Inputs are checked in the order current value, target, rate, then timestep,
/// including on zero-time/zero-rate calls. No state is modified on failure.
///
/// ```
/// use ferravex_math::Vec2;
/// use ferravex_play::motion::{approach_vec2, MotionError};
/// assert_eq!(approach_vec2(Vec2::ZERO, Vec2::X, -1., 0.1),
///            Err(MotionError::InvalidRate));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionError {
    /// The current vector contains a NaN or infinity.
    NonFiniteCurrent,
    /// The target vector contains a NaN or infinity.
    NonFiniteTarget,
    /// The rate is negative, NaN or infinite.
    InvalidRate,
    /// The timestep is negative, NaN or infinite.
    InvalidDelta,
}

impl std::fmt::Display for MotionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFiniteCurrent => "current motion vector must be finite",
            Self::NonFiniteTarget => "target motion vector must be finite",
            Self::InvalidRate => "motion rate must be finite and nonnegative",
            Self::InvalidDelta => "motion timestep must be finite and nonnegative",
        })
    }
}

impl std::error::Error for MotionError {}

/// Approach a 2D target by at most `max_rate * dt` in Euclidean distance.
///
/// Use velocities for `current`/`target` and acceleration (units/s²) for
/// `max_rate` to accelerate, brake or steer without overshooting. Position inputs
/// instead use a speed (units/s). A reachable target is returned exactly.
/// All vector components must be finite; `max_rate` and `dt` (seconds) must be
/// finite and nonnegative, otherwise a [`MotionError`] is returned. Valid zero
/// time/rate or an unchanged target preserves the current value bit-for-bit.
/// Wider intermediates keep results finite even when the difference or step
/// exceeds `f32::MAX`. See the [module docs](self) for rounding/partition limits.
///
/// ```
/// use ferravex_math::Vec2;
/// use ferravex_play::motion::approach_vec2;
/// let velocity = approach_vec2(Vec2::ZERO, Vec2::new(3., 4.), 2., 0.5)?;
/// assert!(velocity.abs_diff_eq(Vec2::new(0.6, 0.8), 1e-6));
/// assert_eq!(approach_vec2(velocity, Vec2::ZERO, 4., 1.)?, Vec2::ZERO);
/// # Ok::<(), ferravex_play::motion::MotionError>(())
/// ```
pub fn approach_vec2(
    current: Vec2,
    target: Vec2,
    max_rate: f32,
    dt: f32,
) -> Result<Vec2, MotionError> {
    approach(current.to_array(), target.to_array(), max_rate, dt).map(Vec2::from_array)
}

/// Approach a 3D target by at most `max_rate * dt` in Euclidean distance.
///
/// This is the 3D counterpart of [`approach_vec2`], with identical validation,
/// units, no-overshoot behavior and floating-point limits. The bound applies to
/// the whole vector, so diagonal movement does not receive extra acceleration.
///
/// ```
/// use ferravex_math::Vec3;
/// use ferravex_play::motion::approach_vec3;
/// let velocity = approach_vec3(Vec3::ZERO, Vec3::new(2., 3., 6.), 7., 0.5)?;
/// assert_eq!(velocity, Vec3::new(1., 1.5, 3.));
/// # Ok::<(), ferravex_play::motion::MotionError>(())
/// ```
pub fn approach_vec3(
    current: Vec3,
    target: Vec3,
    max_rate: f32,
    dt: f32,
) -> Result<Vec3, MotionError> {
    approach(current.to_array(), target.to_array(), max_rate, dt).map(Vec3::from_array)
}

/// Exponentially approach a 2D target without overshoot.
///
/// Remaining error is multiplied by `exp(-rate * dt)`. `rate` is in inverse
/// seconds; `LN_2 / half_life_seconds` gives a desired half-life. A zero target
/// damps velocity toward rest. This bounds output between current and target;
/// it does not impose an acceleration limit (use [`approach_vec2`] for that).
///
/// All components must be finite, and `rate`/`dt` must be finite and nonnegative,
/// otherwise a [`MotionError`] is returned. Valid zero time/rate or an unchanged
/// target preserves the current value bit-for-bit. Wider intermediates preserve
/// small changes and finite results for extreme inputs. See the [module
/// docs](self) for rounding/partition limits. [`response`] keeps its existing
/// permissive scalar behavior; this helper explicitly rejects invalid inputs.
///
/// ```
/// use ferravex_math::Vec2;
/// use ferravex_play::motion::damp_vec2;
/// let velocity = damp_vec2(Vec2::new(8., -4.), Vec2::ZERO,
///                          std::f32::consts::LN_2, 1.)?;
/// assert!(velocity.abs_diff_eq(Vec2::new(4., -2.), 1e-6));
/// # Ok::<(), ferravex_play::motion::MotionError>(())
/// ```
pub fn damp_vec2(current: Vec2, target: Vec2, rate: f32, dt: f32) -> Result<Vec2, MotionError> {
    damp(current.to_array(), target.to_array(), rate, dt).map(Vec2::from_array)
}

/// Exponentially approach a 3D target without overshoot.
///
/// This is the 3D counterpart of [`damp_vec2`], with identical validation,
/// inverse-second rate, finite output and floating-point limits. Pass a zero
/// target to damp velocity; use [`approach_vec3`] to bound acceleration instead.
///
/// ```
/// use ferravex_math::Vec3;
/// use ferravex_play::motion::damp_vec3;
/// let velocity = damp_vec3(Vec3::new(8., -4., 2.), Vec3::ZERO,
///                          std::f32::consts::LN_2, 1.)?;
/// assert!(velocity.abs_diff_eq(Vec3::new(4., -2., 1.), 1e-6));
/// # Ok::<(), ferravex_play::motion::MotionError>(())
/// ```
pub fn damp_vec3(current: Vec3, target: Vec3, rate: f32, dt: f32) -> Result<Vec3, MotionError> {
    damp(current.to_array(), target.to_array(), rate, dt).map(Vec3::from_array)
}

fn validate<const N: usize>(
    current: [f32; N],
    target: [f32; N],
    rate: f32,
    dt: f32,
) -> Result<(), MotionError> {
    if !current.iter().all(|v| v.is_finite()) {
        Err(MotionError::NonFiniteCurrent)
    } else if !target.iter().all(|v| v.is_finite()) {
        Err(MotionError::NonFiniteTarget)
    } else if !rate.is_finite() || rate < 0. {
        Err(MotionError::InvalidRate)
    } else if !dt.is_finite() || dt < 0. {
        Err(MotionError::InvalidDelta)
    } else {
        Ok(())
    }
}

fn approach<const N: usize>(
    current: [f32; N],
    target: [f32; N],
    rate: f32,
    dt: f32,
) -> Result<[f32; N], MotionError> {
    validate(current, target, rate, dt)?;
    if rate == 0. || dt == 0. || current == target {
        return Ok(current);
    }
    // Subtract before narrowing: opposing finite f32 endpoints may differ by
    // more than f32::MAX. Squared distances and rate * dt still fit in f64.
    let distance = current
        .iter()
        .zip(target)
        .map(|(&a, b)| (f64::from(b) - f64::from(a)).powi(2))
        .sum::<f64>()
        .sqrt();
    let step = f64::from(rate) * f64::from(dt);
    if step >= distance {
        return Ok(target);
    }
    Ok(blend(
        current,
        target,
        step / distance,
        (distance - step) / distance,
    ))
}

fn damp<const N: usize>(
    current: [f32; N],
    target: [f32; N],
    rate: f32,
    dt: f32,
) -> Result<[f32; N], MotionError> {
    validate(current, target, rate, dt)?;
    if rate == 0. || dt == 0. || current == target {
        return Ok(current);
    }
    let exponent = -f64::from(rate) * f64::from(dt);
    // Retain the smaller weight directly: 1 - exp(x) loses tiny steps, while
    // 1 - response loses a small but significant residual on huge vectors.
    let (toward, remaining) = if exponent > -std::f64::consts::LN_2 {
        let toward = -exponent.exp_m1();
        (toward, 1. - toward)
    } else {
        let remaining = exponent.exp();
        (1. - remaining, remaining)
    };
    Ok(blend(current, target, toward, remaining))
}

fn blend<const N: usize>(
    current: [f32; N],
    target: [f32; N],
    toward: f64,
    remaining: f64,
) -> [f32; N] {
    if toward == 0. {
        return current;
    }
    if remaining == 0. {
        return target;
    }
    std::array::from_fn(|i| {
        let a = f64::from(current[i]);
        let b = f64::from(target[i]);
        let next = if toward <= remaining {
            a + (b - a) * toward
        } else {
            b + (a - b) * remaining
        };
        next.clamp(a.min(b), a.max(b)) as f32
    })
}

/// Exponential approach weight, independent of render cadence. Invalid or
/// nonpositive intervals/rates do no work; callers still own their fixed clock.
pub fn response(rate: f32, dt: f32) -> f32 {
    if !rate.is_finite() || !dt.is_finite() || rate <= 0. || dt <= 0. {
        return 0.;
    }
    -(-rate * dt).exp_m1()
}

/// Change only an incoming velocity's normal component and retain a chosen
/// amount of tangential speed. Glancing contacts can slide without stopping.
/// Invalid parameters/zero normals return None; separating velocities are intact.
pub fn impact_velocity(velocity: Vec3, normal: Vec3, restitution: f32, slide: f32) -> Option<Vec3> {
    if !velocity.is_finite()
        || !normal.is_finite()
        || !restitution.is_finite()
        || !(0. ..=1.).contains(&restitution)
        || !slide.is_finite()
        || !(0. ..=1.).contains(&slide)
    {
        return None;
    }
    let normal = normal.try_normalize()?;
    let into = velocity.dot(normal);
    if into >= 0. {
        return Some(velocity);
    }
    Some((velocity - normal * into) * slide - normal * into * restitution)
}

/// Bounded radial velocity change, with a smooth falloff to zero at the radius.
/// Apply mass * result as an impulse to give light/heavy props equal launch speed.
pub fn radial_velocity(center: Vec3, point: Vec3, radius: f32, speed: f32) -> Option<Vec3> {
    if !center.is_finite()
        || !point.is_finite()
        || !radius.is_finite()
        || radius <= 0.
        || !speed.is_finite()
        || !(0. ..=1000.).contains(&speed)
    {
        return None;
    }
    let delta = point - center;
    let distance = delta.length();
    if distance >= radius {
        return None;
    }
    let weight = (1. - distance / radius).powi(2);
    Some(delta.try_normalize().unwrap_or(Vec3::Y) * (speed * weight))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_partitions_and_impacts_preserve_tangent_and_separation() {
        let once = response(8., 1. / 30.);
        let twice = 1. - (1. - response(8., 1. / 60.)).powi(2);
        assert!((once - twice).abs() < 1e-6);
        assert_eq!(response(f32::NAN, 1.), 0.);
        let v = Vec3::new(-10., 4., 2.);
        let hit = impact_velocity(v, Vec3::X * 2., 0.2, 0.9).unwrap();
        assert!(hit.abs_diff_eq(Vec3::new(2., 3.6, 1.8), 1e-5));
        assert_eq!(impact_velocity(-v, Vec3::X, 0.2, 0.9), Some(-v));
        assert_eq!(impact_velocity(v, Vec3::ZERO, 0.2, 0.9), None);
    }
    #[test]
    fn radial_pulse_is_finite_bounded_and_has_no_effect_outside_its_radius() {
        assert_eq!(radial_velocity(Vec3::ZERO, Vec3::X * 5., 4., 8.), None);
        assert_eq!(
            radial_velocity(Vec3::ZERO, Vec3::ZERO, 4., 8.),
            Some(Vec3::Y * 8.)
        );
        assert_eq!(
            radial_velocity(Vec3::ZERO, Vec3::X * 2., 4., 8.),
            Some(Vec3::X * 2.)
        );
        assert_eq!(radial_velocity(Vec3::ZERO, Vec3::X, f32::NAN, 8.), None);
    }

    type Step2 = fn(Vec2, Vec2, f32, f32) -> Result<Vec2, MotionError>;
    type Step3 = fn(Vec3, Vec3, f32, f32) -> Result<Vec3, MotionError>;
    const HELPERS: [(Step2, Step3); 2] = [(approach_vec2, approach_vec3), (damp_vec2, damp_vec3)];

    #[test]
    fn approach_bounds_acceleration_across_axes_and_brakes_without_reversing() {
        let next2 = approach_vec2(Vec2::ZERO, Vec2::new(3., 4.), 2., 0.5).unwrap();
        assert!(next2.abs_diff_eq(Vec2::new(0.6, 0.8), 1e-6));
        assert!((next2.length() - 1.).abs() < 1e-6);
        let next3 = approach_vec3(Vec3::ZERO, Vec3::new(2., 3., 6.), 7., 0.5).unwrap();
        assert_eq!(next3, Vec3::new(1., 1.5, 3.));
        assert_eq!(next3.length(), 3.5);
        assert_eq!(
            approach_vec2(Vec2::X * 10., -Vec2::X * 10., 4., 0.5),
            Ok(Vec2::X * 8.)
        );
        assert_eq!(
            approach_vec3(Vec3::Z * 10., -Vec3::Z * 10., 4., 0.5),
            Ok(Vec3::Z * 8.)
        );
        assert_eq!(approach_vec2(next2, Vec2::ZERO, 4., 1.), Ok(Vec2::ZERO));
        assert_eq!(approach_vec3(next3, Vec3::ZERO, 4., 1.), Ok(Vec3::ZERO));
    }

    #[test]
    fn approach_arrives_exactly_at_reachable_targets() {
        for dt in [1., 2., f32::MAX] {
            let target2 = Vec2::new(3., 4.);
            let target3 = Vec3::new(2., 3., 6.);
            assert_eq!(approach_vec2(Vec2::ZERO, target2, 5., dt), Ok(target2));
            assert_eq!(approach_vec3(Vec3::ZERO, target3, 7., dt), Ok(target3));
        }
        let tiny = f32::from_bits(1);
        let target2 = Vec2::new(tiny, -tiny);
        let target3 = Vec3::new(tiny, -tiny, tiny);
        assert_eq!(
            approach_vec2(Vec2::splat(f32::MAX), target2, f32::MAX, 2.),
            Ok(target2)
        );
        assert_eq!(
            approach_vec3(Vec3::splat(f32::MAX), target3, f32::MAX, 2.),
            Ok(target3)
        );
    }

    #[test]
    fn damping_has_the_expected_half_life_toward_rest_or_a_moving_velocity() {
        let rate = std::f32::consts::LN_2;
        let next2 = damp_vec2(Vec2::new(8., -4.), Vec2::ZERO, rate, 1.).unwrap();
        assert!(next2.abs_diff_eq(Vec2::new(4., -2.), 1e-6));
        let next3 = damp_vec3(Vec3::new(8., -4., 2.), Vec3::ZERO, rate, 1.).unwrap();
        assert!(next3.abs_diff_eq(Vec3::new(4., -2., 1.), 1e-6));
        let next2 = damp_vec2(Vec2::new(-4., 6.), Vec2::new(8., -2.), rate, 1.).unwrap();
        assert!(next2.abs_diff_eq(Vec2::new(2., 2.), 1e-6));
        let next3 = damp_vec3(Vec3::new(-4., 6., 2.), Vec3::new(8., -2., 4.), rate, 1.).unwrap();
        assert!(next3.abs_diff_eq(Vec3::new(2., 2., 3.), 1e-6));
    }

    #[test]
    fn fixed_targets_are_stable_across_regular_and_irregular_time_partitions() {
        for (step2, step3) in HELPERS {
            for rate in [0.001, 2., 20., 100.] {
                let start2 = Vec2::new(-3., 4.);
                let target2 = Vec2::new(7., -2.);
                let start3 = Vec3::new(-3., 4., -8.);
                let target3 = Vec3::new(7., -2., 5.);
                let once2 = step2(start2, target2, rate, 1.).unwrap();
                let once3 = step3(start3, target3, rate, 1.).unwrap();
                for hz in [30, 60, 144] {
                    let mut many2 = start2;
                    let mut many3 = start3;
                    for _ in 0..hz {
                        many2 = step2(many2, target2, rate, 1. / hz as f32).unwrap();
                        many3 = step3(many3, target3, rate, 1. / hz as f32).unwrap();
                    }
                    assert!(once2.abs_diff_eq(many2, 1e-4));
                    assert!(once3.abs_diff_eq(many3, 1e-4));
                }
                let mut many2 = start2;
                let mut many3 = start3;
                for dt in [0.125, 0., 0.25, 0.0625, 0.5625] {
                    many2 = step2(many2, target2, rate, dt).unwrap();
                    many3 = step3(many3, target3, rate, dt).unwrap();
                }
                assert!(once2.abs_diff_eq(many2, 1e-5));
                assert!(once3.abs_diff_eq(many3, 1e-5));
            }
        }
    }

    #[test]
    fn invalid_vectors_are_rejected_even_with_zero_time_or_rate() {
        for (step2, step3) in HELPERS {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                for (rate, dt) in [(1., 1.), (0., 1.), (1., 0.)] {
                    for axis in 0..2 {
                        let mut vector = Vec2::ZERO;
                        vector[axis] = bad;
                        assert_eq!(
                            step2(vector, Vec2::ZERO, rate, dt),
                            Err(MotionError::NonFiniteCurrent)
                        );
                        assert_eq!(
                            step2(Vec2::ZERO, vector, rate, dt),
                            Err(MotionError::NonFiniteTarget)
                        );
                    }
                    for axis in 0..3 {
                        let mut vector = Vec3::ZERO;
                        vector[axis] = bad;
                        assert_eq!(
                            step3(vector, Vec3::ZERO, rate, dt),
                            Err(MotionError::NonFiniteCurrent)
                        );
                        assert_eq!(
                            step3(Vec3::ZERO, vector, rate, dt),
                            Err(MotionError::NonFiniteTarget)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_rates_and_time_are_rejected_before_early_return() {
        for (step2, step3) in HELPERS {
            for bad in [
                -1.,
                -f32::from_bits(1),
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ] {
                for other in [0., 1.] {
                    assert_eq!(
                        step2(Vec2::X, Vec2::X, bad, other),
                        Err(MotionError::InvalidRate)
                    );
                    assert_eq!(
                        step3(Vec3::X, Vec3::X, bad, other),
                        Err(MotionError::InvalidRate)
                    );
                    assert_eq!(
                        step2(Vec2::X, Vec2::X, other, bad),
                        Err(MotionError::InvalidDelta)
                    );
                    assert_eq!(
                        step3(Vec3::X, Vec3::X, other, bad),
                        Err(MotionError::InvalidDelta)
                    );
                }
            }
            assert_eq!(
                step2(Vec2::NAN, Vec2::NAN, -1., -1.),
                Err(MotionError::NonFiniteCurrent)
            );
            assert_eq!(
                step3(Vec3::NAN, Vec3::NAN, -1., -1.),
                Err(MotionError::NonFiniteCurrent)
            );
            assert_eq!(
                step2(Vec2::ZERO, Vec2::NAN, -1., -1.),
                Err(MotionError::NonFiniteTarget)
            );
            assert_eq!(
                step3(Vec3::ZERO, Vec3::NAN, -1., -1.),
                Err(MotionError::NonFiniteTarget)
            );
        }
    }

    #[test]
    fn zero_time_rate_and_stationary_targets_preserve_bits() {
        let start2 = Vec2::new(-0., f32::from_bits(1));
        let start3 = Vec3::new(-0., f32::from_bits(1), -f32::MAX);
        for (step2, step3) in HELPERS {
            for (rate, dt) in [(0., f32::MAX), (-0., 1.), (f32::MAX, 0.), (1., -0.)] {
                let next2 = step2(start2, Vec2::splat(f32::MAX), rate, dt).unwrap();
                let next3 = step3(start3, Vec3::splat(f32::MAX), rate, dt).unwrap();
                assert_eq!(
                    next2.to_array().map(f32::to_bits),
                    start2.to_array().map(f32::to_bits)
                );
                assert_eq!(
                    next3.to_array().map(f32::to_bits),
                    start3.to_array().map(f32::to_bits)
                );
            }
            let next2 = step2(start2, start2, f32::MAX, f32::MAX).unwrap();
            let next3 = step3(start3, start3, f32::MAX, f32::MAX).unwrap();
            assert_eq!(
                next2.to_array().map(f32::to_bits),
                start2.to_array().map(f32::to_bits)
            );
            assert_eq!(
                next3.to_array().map(f32::to_bits),
                start3.to_array().map(f32::to_bits)
            );
        }
    }

    #[test]
    fn approach_handles_extreme_differences_products_and_subnormal_steps() {
        let largest = f32::MAX;
        assert_eq!(
            approach_vec2(Vec2::X * -largest, Vec2::X * largest, largest, 1.),
            Ok(Vec2::ZERO)
        );
        assert_eq!(
            approach_vec3(Vec3::Z * -largest, Vec3::Z * largest, largest, 1.),
            Ok(Vec3::ZERO)
        );
        assert_eq!(
            approach_vec2(
                Vec2::splat(-largest),
                Vec2::splat(largest),
                largest,
                largest
            ),
            Ok(Vec2::splat(largest))
        );
        assert_eq!(
            approach_vec3(
                Vec3::splat(-largest),
                Vec3::splat(largest),
                largest,
                largest
            ),
            Ok(Vec3::splat(largest))
        );
        let tiny = f32::from_bits(1);
        assert_eq!(
            approach_vec2(Vec2::ZERO, Vec2::X, tiny, 1.),
            Ok(Vec2::X * tiny)
        );
        assert_eq!(
            approach_vec3(Vec3::ZERO, Vec3::Z, tiny, 1.),
            Ok(Vec3::Z * tiny)
        );
    }

    #[test]
    fn damping_preserves_tiny_weights_and_large_vector_residuals() {
        // Multiplying rate * dt in f32 would underflow before it can be scaled
        // by the large target, losing a representable output entirely.
        let rate = f32::MIN_POSITIVE;
        let expected = (f64::from(f32::MAX) * f64::from(rate).powi(2)) as f32;
        let next2 = damp_vec2(Vec2::ZERO, Vec2::X * f32::MAX, rate, rate).unwrap();
        let next3 = damp_vec3(Vec3::ZERO, Vec3::Z * f32::MAX, rate, rate).unwrap();
        assert_eq!(next2, Vec2::X * expected);
        assert_eq!(next3, Vec3::Z * expected);
        assert!(expected > 0.);

        // A response weight rounds to one here, but the remaining motion is
        // still significant. Preserve exp(-100), rather than subtracting from 1.
        let expected = (f64::from(f32::MAX) * (-100_f64).exp()) as f32;
        let next2 = damp_vec2(Vec2::X * f32::MAX, Vec2::ZERO, 100., 1.).unwrap();
        let next3 = damp_vec3(Vec3::Z * f32::MAX, Vec3::ZERO, 100., 1.).unwrap();
        assert_eq!(next2, Vec2::X * expected);
        assert_eq!(next3, Vec3::Z * expected);
        assert!(expected > 1e-5);
        let split2 = damp_vec2(
            damp_vec2(Vec2::X * f32::MAX, Vec2::ZERO, 100., 0.5).unwrap(),
            Vec2::ZERO,
            100.,
            0.5,
        )
        .unwrap();
        let split3 = damp_vec3(
            damp_vec3(Vec3::Z * f32::MAX, Vec3::ZERO, 100., 0.5).unwrap(),
            Vec3::ZERO,
            100.,
            0.5,
        )
        .unwrap();
        assert!((split2.x - expected).abs() <= expected * 1e-6);
        assert!((split3.z - expected).abs() <= expected * 1e-6);
    }

    fn assert_bounded<const N: usize>(start: [f32; N], target: [f32; N], next: [f32; N]) {
        for i in 0..N {
            assert!(next[i].is_finite());
            assert!((start[i].min(target[i])..=start[i].max(target[i])).contains(&next[i]));
        }
    }

    #[test]
    fn checked_results_stay_finite_between_endpoints_across_float_scales() {
        let scales = [
            -f32::MAX,
            -1.,
            -f32::from_bits(1),
            0.,
            f32::from_bits(1),
            1.,
            f32::MAX,
        ];
        for (step2, step3) in HELPERS {
            for a in scales {
                for b in scales {
                    let start2 = Vec2::new(a, -a);
                    let target2 = Vec2::new(b, -b);
                    let start3 = Vec3::new(a, -a, a * 0.5);
                    let target3 = Vec3::new(b, -b, b * 0.5);
                    for (rate, dt) in [
                        (0., 1.),
                        (1., 0.),
                        (f32::from_bits(1), 1.),
                        (2., 0.25),
                        (1., 100.),
                        (f32::MAX, f32::MAX),
                    ] {
                        let next2 = step2(start2, target2, rate, dt).unwrap();
                        let next3 = step3(start3, target3, rate, dt).unwrap();
                        assert_bounded(start2.to_array(), target2.to_array(), next2.to_array());
                        assert_bounded(start3.to_array(), target3.to_array(), next3.to_array());
                    }
                }
            }
        }
    }

    #[test]
    fn damping_saturates_exactly_at_tiny_targets_and_keeps_legacy_response_contract() {
        let tiny = f32::from_bits(1);
        let target2 = Vec2::new(tiny, -tiny);
        let target3 = Vec3::new(tiny, -tiny, -0.);
        assert_eq!(
            damp_vec2(Vec2::splat(f32::MAX), target2, f32::MAX, 1.),
            Ok(target2)
        );
        let next3 = damp_vec3(Vec3::splat(f32::MAX), target3, f32::MAX, 1.).unwrap();
        assert_eq!(
            next3.to_array().map(f32::to_bits),
            target3.to_array().map(f32::to_bits)
        );
        for bad in [-1., 0., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(response(bad, 1.), 0.);
            assert_eq!(response(1., bad), 0.);
        }
        assert_eq!(response(f32::MAX, f32::MAX), 1.);
    }
}
