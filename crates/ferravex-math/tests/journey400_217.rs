use ferravex_math::{Quat, SwingTwist, Vec3};
use std::f32::consts::PI;

fn near(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} (tolerance {tolerance})"
    );
}

fn rotation_near(actual: Quat, expected: Quat) {
    assert!(actual.is_finite() && actual.is_normalized(), "{actual:?}");
    assert!(
        actual.try_angle_between(expected).unwrap() < 8.0e-7,
        "{actual:?} != {expected:?}"
    );
}

fn check_parts(rotation: Quat, axis: Vec3) -> SwingTwist {
    let parts = rotation.try_swing_twist(axis).unwrap();
    let unit = axis.normalize();
    assert!(parts.swing.is_normalized());
    assert!(parts.twist.is_normalized());
    assert!((-PI..=PI).contains(&parts.twist_angle));
    rotation_near(parts.swing * parts.twist, rotation);
    assert!((parts.twist * unit).abs_diff_eq(unit, 4.0e-7));
    assert!((parts.swing * unit).abs_diff_eq(rotation.normalize() * unit, 8.0e-7));
    assert!(parts.swing.xyz().dot(unit).abs() < 4.0e-7);
    parts
}

#[test]
fn identity_pure_twist_and_pure_swing() {
    let identity = check_parts(Quat::IDENTITY, Vec3::Z);
    assert_eq!(identity.swing, Quat::IDENTITY);
    assert_eq!(identity.twist, Quat::IDENTITY);
    assert_eq!(identity.twist_angle, 0.0);
    assert!(!identity.is_singular);
    for angle in [-2.7, -0.4, 0.0, 0.7, 2.9] {
        let twist = Quat::from_rotation_z(angle);
        let parts = check_parts(twist, Vec3::Z * 17.0);
        rotation_near(parts.swing, Quat::IDENTITY);
        rotation_near(parts.twist, twist);
        near(parts.twist_angle, angle, 3.0e-7);

        let swing = Quat::from_rotation_x(angle);
        let parts = check_parts(swing, Vec3::Z);
        rotation_near(parts.swing, swing);
        assert_eq!(parts.twist, Quat::IDENTITY);
        assert_eq!(parts.twist_angle, 0.0);
        assert!(!parts.is_singular);
    }
}

#[test]
fn composition_order_is_local_twist_then_swing() {
    let axis = Vec3::new(1.0, 2.0, 3.0).normalize();
    let swing = Quat::from_axis_angle(axis.any_orthonormal_vector(), 1.2);
    let twist = Quat::from_axis_angle(axis, -0.8);
    let parts = check_parts(swing * twist, axis * 8.0);
    rotation_near(parts.swing, swing);
    rotation_near(parts.twist, twist);
    near(parts.twist_angle, -0.8, 2.0e-7);
    assert!(
        (parts.twist * parts.swing)
            .try_angle_between(swing * twist)
            .unwrap()
            > 0.1
    );
}

#[test]
fn antipodal_inputs_have_identical_factors_and_angles() {
    for q in [
        Quat::IDENTITY,
        Quat::from_xyzw(0.2, -0.3, 0.4, -0.5),
        Quat::from_xyzw(-1.0, 2.0, 3.0, 0.0),
        Quat::from_xyzw(0.0, -1.0, 0.0, 0.0),
    ] {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1.0, 2.0, -1.0)] {
            assert_eq!(q.try_swing_twist(axis), (-q).try_swing_twist(axis));
            rotation_near(
                q.try_clamp_twist(axis, -0.2, 0.4).unwrap(),
                (-q).try_clamp_twist(axis, -0.2, 0.4).unwrap(),
            );
        }
    }
}

#[test]
fn axis_reversal_negates_angle_and_reversed_limits_match() {
    for q in [
        Quat::from_xyzw(0.3, -0.5, 0.7, 0.2),
        Quat::from_xyzw(-1.0, 2.0, -3.0, 0.0),
        Quat::from_xyzw(1.0, 0.0, 0.0, 0.0),
    ] {
        for axis in [Vec3::Z, Vec3::new(2.0, -3.0, 1.0)] {
            let a = check_parts(q, axis);
            let b = check_parts(q, -axis);
            assert_eq!(a.swing, b.swing);
            assert_eq!(a.twist, b.twist);
            assert_eq!(a.twist_angle, -b.twist_angle);
            assert_eq!(a.is_singular, b.is_singular);
            rotation_near(
                q.try_clamp_twist(axis, -0.2, 0.7).unwrap(),
                q.try_clamp_twist(-axis, -0.7, 0.2).unwrap(),
            );
        }
    }
}

#[test]
fn exact_half_turn_ties_use_first_nonzero_vector_component() {
    // Positive x selects this quaternion representative even though z is negative.
    let q = Quat::from_xyzw(1.0, 0.0, -1.0, 0.0);
    assert_eq!(check_parts(q, Vec3::Z).twist_angle, -PI);
    assert_eq!(check_parts(-q, Vec3::Z).twist_angle, -PI);
    assert_eq!(check_parts(q, -Vec3::Z).twist_angle, PI);
    for (q, axis) in [
        (Quat::from_xyzw(-1.0, 0.0, 0.0, 0.0), Vec3::X),
        (Quat::from_xyzw(0.0, -1.0, 0.0, 0.0), Vec3::Y),
        (Quat::from_xyzw(0.0, 0.0, -1.0, 0.0), Vec3::Z),
    ] {
        assert_eq!(check_parts(q, axis).twist_angle, PI);
        rotation_near(
            q.try_clamp_twist(axis, -0.4, 0.4).unwrap(),
            Quat::from_axis_angle(axis, 0.4),
        );
    }
}

#[test]
fn singular_half_turn_is_explicit_and_reconstructs() {
    for (q, axis) in [
        (Quat::from_xyzw(1.0, 0.0, 0.0, 0.0), Vec3::Z),
        (
            Quat::from_xyzw(1.0, -1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ),
    ] {
        let parts = check_parts(q, axis);
        assert!(parts.is_singular);
        assert_eq!(parts.twist, Quat::IDENTITY);
        assert_eq!(parts.twist_angle, 0.0);
        assert_eq!(q.try_clamp_twist(axis, -0.2, 0.2), q.try_normalize());
        // The documented fallback can author a nonzero twist on the singular swing.
        let result = q.try_clamp_twist(axis, 0.3, 0.5).unwrap();
        rotation_near(result, parts.swing * Quat::from_axis_angle(axis, 0.3));
        assert!((result * axis.normalize()).abs_diff_eq(q.normalize() * axis.normalize(), 5.0e-7));
    }
}

#[test]
fn near_singular_inputs_keep_measurable_twist_without_an_epsilon_snap() {
    for tiny in [0.001, 1.0e-8, 1.0e-20, f32::MIN_POSITIVE, f32::from_bits(1)] {
        for sign in [-1.0, 1.0] {
            let q = Quat::from_xyzw(1.0, 0.0, sign * tiny, tiny * 2.0);
            let parts = check_parts(q, Vec3::Z);
            assert!(!parts.is_singular);
            near(parts.twist_angle, sign * 2.0 * 0.5_f32.atan(), 2.0e-7);
        }
        assert!(!check_parts(Quat::from_xyzw(1.0, 0.0, 0.0, tiny), Vec3::Z).is_singular);
        assert_eq!(
            check_parts(Quat::from_xyzw(1.0, 0.0, tiny, 0.0), Vec3::Z).twist_angle,
            PI
        );
    }
    // f32 trig does not make an exact mathematical half turn.
    assert!(!check_parts(Quat::from_rotation_x(PI), Vec3::Z).is_singular);
}

#[test]
fn near_singular_limits_preserve_aim_even_at_subnormal_precision() {
    for tiny in [0.001, 1.0e-8, 1.0e-20, f32::from_bits(1)] {
        for sign in [-1.0, 1.0] {
            let q = Quat::from_xyzw(1.0, 0.0, sign * tiny, tiny * 2.0);
            let parts = q.try_swing_twist(Vec3::Z).unwrap();
            let limited = q.try_clamp_twist(Vec3::Z, -0.2, 0.2).unwrap();
            rotation_near(limited, parts.swing * Quat::from_rotation_z(sign * 0.2));
            assert!((limited * Vec3::Z).abs_diff_eq(q.normalize() * Vec3::Z, 8.0e-7));
            assert!(limited.try_angle_between(q).unwrap() > 0.7);
            if tiny >= f32::MIN_POSITIVE {
                near(
                    limited.try_swing_twist(Vec3::Z).unwrap().twist_angle,
                    sign * 0.2,
                    5.0e-7,
                );
            }
            // At subnormal precision the composed scalar/projection can round
            // to zero. Orientation and aim still have the guarantees above;
            // recovering the authored twist angle cannot have a fixed error.
        }
    }
}

#[test]
fn shortest_angle_wraps_and_retains_tiny_twists() {
    for (angle, expected) in [(1.5 * PI, -0.5 * PI), (-1.5 * PI, 0.5 * PI)] {
        near(
            check_parts(Quat::from_rotation_z(angle), Vec3::Z).twist_angle,
            expected,
            3.0e-7,
        );
    }
    let tiny = f32::from_bits(1);
    for component in [tiny, 1.0e-30, 1.0e-12] {
        let q = Quat::from_xyzw(0.0, 0.0, component, 1.0);
        assert_eq!(check_parts(q, Vec3::Z).twist_angle, 2.0 * component);
    }
}

#[test]
fn non_unit_extreme_axes_and_quaternions_are_accepted() {
    for scale in [f32::from_bits(1), 1.0, f32::MAX] {
        let q = Quat::from_array([scale; 4]);
        for axis_scale in [f32::from_bits(1), 1.0, f32::MAX] {
            for axis in [Vec3::Z * axis_scale, Vec3::splat(axis_scale)] {
                let actual = check_parts(q, axis);
                let expected = check_parts(Quat::from_array([1.0; 4]), axis);
                assert_eq!(actual, expected);
                let limited = q.try_clamp_twist(axis, -0.2, 0.2).unwrap();
                rotation_near(
                    limited,
                    Quat::from_array([1.0; 4])
                        .try_clamp_twist(axis, -0.2, 0.2)
                        .unwrap(),
                );
            }
        }
    }
    // Normalize in f64 before projection: this ratio is below f32 range.
    let q = Quat::from_xyzw(f32::MAX, 0.0, f32::from_bits(1), f32::from_bits(1));
    let parts = check_parts(q, Vec3::Z);
    assert!(!parts.is_singular);
    near(parts.twist_angle, PI * 0.5, 1.0e-7);
}

#[test]
fn limits_preserve_valid_input_and_clamp_both_sides() {
    let axis = Vec3::Z;
    let swing = Quat::from_rotation_x(0.9);
    for angle in [-1.5, -0.2, 0.0, 0.4, 1.5] {
        let q = swing * Quat::from_rotation_z(angle);
        let limited = q.try_clamp_twist(axis, -0.2, 0.4).unwrap();
        rotation_near(
            limited,
            swing * Quat::from_rotation_z(angle.clamp(-0.2, 0.4)),
        );
        assert!((limited * axis).abs_diff_eq(q * axis, 4.0e-7));
        if (-0.2..=0.4).contains(&q.try_swing_twist(axis).unwrap().twist_angle) {
            assert_eq!(limited, q.normalize());
        }
        for q in [q, -q, q * 7.0] {
            assert_eq!(q.try_clamp_twist(axis, -PI, PI), q.try_normalize());
        }
        rotation_near(q.try_clamp_twist(axis, 0.0, 0.0).unwrap(), swing);
    }
    let q = swing * Quat::from_rotation_z(-0.7);
    let angle = q.try_swing_twist(axis).unwrap().twist_angle;
    assert_eq!(q.try_clamp_twist(axis, angle, angle), q.try_normalize());
}

#[test]
fn invalid_inputs_and_limits_are_rejected_without_retaining_state() {
    for index in 0..4 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut values = [1.0; 4];
            values[index] = value;
            let q = Quat::from_array(values);
            assert_eq!(q.try_swing_twist(Vec3::Z), None);
            assert_eq!(q.try_clamp_twist(Vec3::Z, -PI, PI), None);
        }
    }
    let zero = Quat::from_array([0.0; 4]);
    assert_eq!(zero.try_swing_twist(Vec3::Z), None);
    assert_eq!(zero.try_clamp_twist(Vec3::Z, 0.0, 0.0), None);
    for axis in [Vec3::ZERO, Vec3::NAN, Vec3::INFINITY] {
        assert_eq!(Quat::IDENTITY.try_swing_twist(axis), None);
        assert_eq!(Quat::IDENTITY.try_clamp_twist(axis, -PI, PI), None);
    }
    for index in 0..3 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut values = [1.0; 3];
            values[index] = value;
            assert_eq!(
                Quat::IDENTITY.try_swing_twist(Vec3::from_array(values)),
                None
            );
        }
    }
    for (min, max) in [
        (0.3, -0.3),
        (-PI - 0.01, PI),
        (-PI, PI + 0.01),
        (f32::NAN, 0.0),
        (0.0, f32::NAN),
        (f32::NEG_INFINITY, 0.0),
        (f32::INFINITY, 0.0),
        (0.0, f32::INFINITY),
        (0.0, f32::NEG_INFINITY),
    ] {
        assert_eq!(Quat::IDENTITY.try_clamp_twist(Vec3::Z, min, max), None);
    }
    assert_eq!(
        Quat::IDENTITY.try_clamp_twist(Vec3::Z, -0.0, 0.0),
        Some(Quat::IDENTITY)
    );
    check_parts(Quat::IDENTITY, Vec3::Z);
}

#[test]
fn seeded_reconstruction_axis_reversal_and_limit_properties() {
    let mut seed = 217_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) as u32 as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32
    };
    for _ in 0..1024 {
        let q = Quat::from_xyzw(random(), random(), random(), random());
        let axis = Vec3::new(random(), random(), random());
        let parts = check_parts(q, axis);
        assert_eq!(parts, check_parts(-q, axis));
        assert_eq!(parts.twist_angle, -check_parts(q, -axis).twist_angle);
        let limited = q.try_clamp_twist(axis, -0.2, 0.4).unwrap();
        let limited_parts = check_parts(limited, axis);
        // Recovering either factor is ill-conditioned near a half-turn swing.
        // Reconstruction and axis-direction checks retain their fixed tolerances.
        let factor_tolerance = 8.0e-7 / parts.swing.w.abs().max(1.0e-7);
        near(
            limited_parts.twist_angle,
            parts.twist_angle.clamp(-0.2, 0.4),
            factor_tolerance,
        );
        assert!(limited_parts.swing.try_angle_between(parts.swing).unwrap() < factor_tolerance);
        let unit = axis.normalize();
        assert!((limited * unit).abs_diff_eq(q.normalize() * unit, 8.0e-7));
        let mut repeated = limited;
        for _ in 0..8 {
            repeated = repeated.try_clamp_twist(axis, -0.2, 0.4).unwrap();
        }
        assert!(repeated.try_angle_between(limited).unwrap() < 8.0 * factor_tolerance);
    }
}

#[test]
fn aiming_pose_recipe_preserves_aim_and_limits_roll() {
    let aim = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.4, 0.2, 1.0));
    let authored_pose = aim * Quat::from_rotation_z(1.2);
    let limited_pose = authored_pose.try_clamp_twist(Vec3::Z, -0.3, 0.3).unwrap();
    assert!((limited_pose * Vec3::Z).abs_diff_eq(authored_pose * Vec3::Z, 1.0e-6));
    let parts = limited_pose.try_swing_twist(Vec3::Z).unwrap();
    near(parts.twist_angle, 0.3, 1.0e-6);
    rotation_near(parts.swing, aim);
}
