//! Small, renderer-independent tools for responsive arcade motion and impacts.
use ferravex_math::Vec3;

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
}
