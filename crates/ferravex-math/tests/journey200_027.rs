use ferravex_math::{Quat, Vec3};
use std::f32::consts::{FRAC_PI_2, PI};

fn near(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} (tolerance {tolerance})"
    );
}

fn rotation_near(actual: Quat, expected: Quat, tolerance: f32) {
    assert!(actual.is_finite() && actual.is_normalized());
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        assert!(
            (actual * axis).abs_diff_eq(expected * axis, tolerance),
            "{actual:?} != {expected:?} on {axis:?}"
        );
    }
}

#[test]
fn distance_is_symmetric_shortest_arc_in_radians() {
    let a = Quat::from_rotation_y(170.0_f32.to_radians());
    let b = Quat::from_rotation_y(-170.0_f32.to_radians());
    for (from, to) in [(a, b), (b, a), (-a, b), (a, -b), (-b, -a)] {
        near(
            from.try_angle_between(to).unwrap(),
            20.0_f32.to_radians(),
            3.0e-7,
        );
    }
    near(
        Quat::IDENTITY
            .try_angle_between(Quat::from_rotation_z(1.5 * PI))
            .unwrap(),
        FRAC_PI_2,
        2.0e-7,
    );
}

#[test]
fn identical_and_antipodal_quaternions_do_not_move_or_flip_sign() {
    for q in [
        Quat::IDENTITY,
        Quat::from_xyzw(0.2, -0.3, 0.4, 0.5),
        Quat::from_xyzw(1.0, 0.0, 0.0, 0.0),
    ] {
        for from in [q, -q] {
            for target in [from, -from] {
                assert_eq!(from.try_angle_between(target), Some(0.0));
                for budget in [0.0, 0.1, PI, f32::MAX] {
                    assert_eq!(
                        from.try_rotate_towards(target, budget),
                        Some(from.normalize())
                    );
                }
            }
        }
    }
}

#[test]
fn tiny_distances_and_steps_remain_resolvable() {
    for angle in [0.01, 1.0e-4, 1.0e-6, 1.0e-12, 1.0e-30] {
        let target = Quat::from_rotation_x(angle);
        for target in [target, -target] {
            near(
                Quat::IDENTITY.try_angle_between(target).unwrap(),
                angle,
                angle * 2.0e-7,
            );
            assert_eq!(
                Quat::IDENTITY.try_rotate_towards(target, 0.0),
                Some(Quat::IDENTITY)
            );
            let budget = angle * 0.25;
            let next = Quat::IDENTITY.try_rotate_towards(target, budget).unwrap();
            near(
                Quat::IDENTITY.try_angle_between(next).unwrap(),
                budget,
                angle * 2.0e-7,
            );
            near(
                next.try_angle_between(target).unwrap(),
                angle - budget,
                angle * 2.0e-7,
            );
            assert!(next.is_normalized());
        }
    }
    // Construct directly: halving the smallest subnormal angle would underflow.
    let tiny = f32::from_bits(1);
    let target = Quat::from_xyzw(tiny * 4.0, 0.0, 0.0, 1.0);
    assert_eq!(Quat::IDENTITY.try_angle_between(target), Some(tiny * 8.0));
    let next = Quat::IDENTITY
        .try_rotate_towards(target, tiny * 2.0)
        .unwrap();
    assert_eq!(next, Quat::from_xyzw(tiny, 0.0, 0.0, 1.0));
}

#[test]
fn near_identical_non_identity_rotations_do_not_snap() {
    let from = Quat::from_rotation_y(0.5);
    let target = Quat::from_rotation_y(0.50001);
    let distance = from.try_angle_between(target).unwrap();
    assert!(distance > 9.9e-6 && distance < 1.01e-5);
    assert_eq!(from.try_rotate_towards(target, 0.0), Some(from.normalize()));
    let next = from.try_rotate_towards(-target, distance * 0.25).unwrap();
    near(
        from.try_angle_between(next).unwrap(),
        distance * 0.25,
        5.0e-8,
    );
    near(
        next.try_angle_between(target).unwrap(),
        distance * 0.75,
        5.0e-8,
    );
}

#[test]
fn finite_non_unit_endpoints_include_extreme_and_subnormal_scales() {
    let axis = Vec3::new(1.0, 1.0, 1.0);
    for from_scale in [f32::from_bits(1), 1.0, f32::MAX] {
        for target_scale in [f32::from_bits(1), 1.0, f32::MAX] {
            let from = Quat::from_xyzw(0.0, 0.0, 0.0, from_scale);
            let target = Quat::from_array([target_scale; 4]);
            near(
                from.try_angle_between(target).unwrap(),
                2.0 * PI / 3.0,
                3.0e-7,
            );
            let next = from.try_rotate_towards(-target, 0.2).unwrap();
            rotation_near(next, Quat::from_axis_angle(axis, 0.2), 2.0e-7);
            assert_eq!(
                from.try_rotate_towards(-target, f32::MAX),
                Some(Quat::from_array([0.5; 4]))
            );
        }
    }
}

#[test]
fn invalid_endpoints_are_rejected_even_when_the_budget_is_zero() {
    let mut invalid = vec![Quat::from_array([0.0; 4]), Quat::NAN];
    for component in 0..4 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut array = [1.0; 4];
            array[component] = value;
            invalid.push(Quat::from_array(array));
        }
    }
    for invalid in invalid {
        for (from, target) in [(invalid, Quat::IDENTITY), (Quat::IDENTITY, invalid)] {
            assert_eq!(from.try_angle_between(target), None);
            for budget in [0.0, 0.5, f32::MAX] {
                assert_eq!(from.try_rotate_towards(target, budget), None);
            }
        }
    }
}

#[test]
fn invalid_angular_budgets_are_rejected_without_panicking() {
    let target = Quat::from_rotation_x(1.0);
    for budget in [
        -1.0,
        -f32::from_bits(1),
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        assert_eq!(Quat::IDENTITY.try_rotate_towards(target, budget), None);
        assert_eq!(target.try_rotate_towards(target, budget), None);
    }
    assert_eq!(
        Quat::IDENTITY.try_rotate_towards(target, -0.0),
        Some(Quat::IDENTITY)
    );
}

#[test]
fn step_budget_stops_at_the_target_with_a_continuous_quaternion_sign() {
    let from = Quat::from_rotation_z(0.3) * 3.0;
    let target = Quat::from_rotation_z(1.4) * -4.0;
    assert_eq!(from.try_rotate_towards(target, 0.0), Some(from.normalize()));
    let next = from.try_rotate_towards(target, 0.25).unwrap();
    rotation_near(next, Quat::from_rotation_z(0.55), 2.0e-7);
    assert!(next.dot(from) > 0.0);
    let arrived = from.try_rotate_towards(target, 2.0).unwrap();
    assert_eq!(arrived, -target.normalize());
    assert_eq!(arrived.try_rotate_towards(target, 0.25), Some(arrived));
}

#[test]
fn rotation_takes_the_short_path_across_the_angle_wrap() {
    let from = Quat::from_rotation_z(170.0_f32.to_radians());
    let target = Quat::from_rotation_z(-170.0_f32.to_radians());
    for target in [target, -target] {
        let next = from
            .try_rotate_towards(target, 5.0_f32.to_radians())
            .unwrap();
        rotation_near(next, Quat::from_rotation_z(175.0_f32.to_radians()), 3.0e-7);
        near(
            next.try_angle_between(target).unwrap(),
            15.0_f32.to_radians(),
            3.0e-7,
        );
    }
}

#[test]
fn exact_half_turn_uses_the_supplied_target_sign_to_break_the_tie() {
    let half_turn = Quat::from_xyzw(0.0, 0.0, 1.0, 0.0);
    for sign in [-1.0, 1.0] {
        let target = half_turn * sign;
        assert_eq!(Quat::IDENTITY.try_angle_between(target), Some(PI));
        let next = Quat::IDENTITY
            .try_rotate_towards(target, FRAC_PI_2)
            .unwrap();
        assert!((next * Vec3::X).abs_diff_eq(Vec3::Y * sign, 2.0e-7));
        assert_eq!(Quat::IDENTITY.try_rotate_towards(target, PI), Some(target));
    }
}

#[test]
fn repeated_steps_advance_by_the_budget_then_remain_at_the_target() {
    let start = Quat::from_axis_angle(Vec3::new(1.0, -2.0, 3.0), 0.7);
    let target = Quat::from_axis_angle(Vec3::new(-3.0, 1.0, 2.0), -1.9);
    let budget = 0.03;
    let mut current = start;
    for _ in 0..128 {
        let remaining = current.try_angle_between(target).unwrap();
        let next = current.try_rotate_towards(target, budget).unwrap();
        let moved = current.try_angle_between(next).unwrap();
        let after = next.try_angle_between(target).unwrap();
        near(moved, remaining.min(budget), 2.0e-7);
        near(after, (remaining - budget).max(0.0), 3.0e-7);
        assert!(next.is_normalized());
        assert!(current.dot(next) > 0.0);
        current = next;
    }
    rotation_near(current, target, 2.0e-7);
}
