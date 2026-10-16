//! Explicit arithmetic for simulation that must avoid platform libm variation.
//! These helpers do not make an entire game deterministic by themselves.
/// Sine/cosine with fixed f64 range reduction and polynomial evaluation, then
/// one f32 rounding. No platform trig calls or fused multiply-add operations.
/// Nonfinite input returns NaNs. Intended for bounded game input angles; huge
/// angles lose reduction accuracy. Absolute error is below 2e-7 for [-2π,2π].
pub fn sin_cos(angle: f32) -> (f32, f32) {
    if !angle.is_finite() {
        return (f32::NAN, f32::NAN);
    }
    let mut x = f64::from(angle) % std::f64::consts::TAU;
    if x > std::f64::consts::PI {
        x -= std::f64::consts::TAU;
    }
    if x < -std::f64::consts::PI {
        x += std::f64::consts::TAU;
    }
    let mut sign = 1.;
    if x > std::f64::consts::FRAC_PI_2 {
        x = std::f64::consts::PI - x;
        sign = -1.;
    } else if x < -std::f64::consts::FRAC_PI_2 {
        x = -std::f64::consts::PI - x;
        sign = -1.;
    }
    let z = x * x;
    // Horner evaluation, highest coefficient first (through x^21 / x^20).
    let s = 1. / 51090942171709440000.;
    let s = -1. / 121645100408832000. + z * s;
    let s = 1. / 355687428096000. + z * s;
    let s = -1. / 1307674368000. + z * s;
    let s = 1. / 6227020800. + z * s;
    let s = -1. / 39916800. + z * s;
    let s = 1. / 362880. + z * s;
    let s = -1. / 5040. + z * s;
    let s = 1. / 120. + z * s;
    let s = -1. / 6. + z * s;
    let s = x * (1. + z * s);
    let c = 1. / 2432902008176640000.;
    let c = -1. / 6402373705728000. + z * c;
    let c = 1. / 20922789888000. + z * c;
    let c = -1. / 87178291200. + z * c;
    let c = 1. / 479001600. + z * c;
    let c = -1. / 3628800. + z * c;
    let c = 1. / 40320. + z * c;
    let c = -1. / 720. + z * c;
    let c = 1. / 24. + z * c;
    let c = -1. / 2. + z * c;
    let c = sign * (1. + z * c);
    (s as f32, c as f32)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_angle_accuracy_symmetry_and_unit_length() {
        for i in -20000..=20000 {
            let angle = i as f32 * std::f32::consts::TAU / 20000.;
            let (s, c) = sin_cos(angle);
            assert!((s - angle.sin()).abs() < 2e-7, "sin {angle}");
            assert!((c - angle.cos()).abs() < 2e-7, "cos {angle}");
            assert!((s * s + c * c - 1.).abs() < 2e-7);
            let (ns, nc) = sin_cos(-angle);
            assert_eq!(s.to_bits() ^ 0x80000000, ns.to_bits());
            assert_eq!(c.to_bits(), nc.to_bits());
        }
        assert!(sin_cos(f32::INFINITY).0.is_nan());
    }
}
