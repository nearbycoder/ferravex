use ferravex_math::{Mat4, MatrixError, Quat, Vec3, Vec4, vec3, vec4};
use std::f32::consts::{FRAC_PI_2, PI};

fn near(a: Vec3, b: Vec3, tolerance: f32) {
    assert!(a.abs_diff_eq(b, tolerance), "{a:?} != {b:?}");
}

#[test]
fn inverse_diagnostics_preserve_option_and_nan_contracts() {
    let overflow = Mat4::from_scale(vec3(f32::from_bits(1), 1.0, 1.0));
    for (matrix, error) in [
        (Mat4::ZERO, MatrixError::Singular { column: 0 }),
        (
            Mat4::from_scale(vec3(1.0, 0.0, 1.0)),
            MatrixError::Singular { column: 1 },
        ),
        (Mat4::NAN, MatrixError::NonFiniteInput { input: "matrix" }),
        (
            Mat4::from_scale(vec3(1.0, f32::INFINITY, 1.0)),
            MatrixError::NonFiniteInput { input: "matrix" },
        ),
        (overflow, MatrixError::ResultOutOfRange),
    ] {
        assert_eq!(matrix.checked_inverse(), Err(error));
        assert!(matrix.try_inverse().is_none());
        assert!(matrix.inverse().is_nan());
    }
    let dependent = Mat4::from_cols(Vec4::X, Vec4::X * 2.0, Vec4::Z, Vec4::W);
    assert_eq!(
        dependent.checked_inverse(),
        Err(MatrixError::Singular { column: 1 })
    );
}

#[test]
fn inverse_accepts_tiny_and_extreme_scales_without_a_determinant_cutoff() {
    let tiny = Mat4::from_scale(Vec3::splat(1.0e-20));
    assert_eq!(tiny.determinant(), 0.0);
    assert!(tiny.checked_inverse().is_ok());
    for scale in [
        vec3(1.0e-30, -2.0e-30, 3.0e-30),
        vec3(1.0e-38, 2.0, -3.0e30),
        vec3(f32::MAX, -f32::MAX, f32::MAX),
    ] {
        let matrix = Mat4::from_scale(scale);
        let inverse = matrix.checked_inverse().unwrap();
        assert_eq!(matrix.try_inverse(), Some(inverse));
        assert!((inverse * matrix).abs_diff_eq(Mat4::IDENTITY, 1.0e-6));
        let point = vec3(0.25, -0.5, 0.75);
        near(
            inverse
                .try_project_point3(matrix.try_project_point3(point).unwrap())
                .unwrap(),
            point,
            1.0e-6,
        );
    }
}

#[test]
fn inverse_uses_pivoting_for_general_projective_matrices() {
    let swap = Mat4::from_cols(Vec4::Y, Vec4::X, Vec4::W, Vec4::Z);
    assert_eq!(swap.checked_inverse(), Ok(swap));
    let general = Mat4::from_cols(
        vec4(2.0, 1.0, 0.0, 0.3),
        vec4(-1.0, 3.0, 2.0, 0.0),
        vec4(0.4, 1.0, 4.0, -0.2),
        vec4(5.0, 2.0, -1.0, 1.0),
    );
    let inverse = general.checked_inverse().unwrap();
    assert!((general * inverse).abs_diff_eq(Mat4::IDENTITY, 2.0e-5));
    assert!((inverse * general).abs_diff_eq(Mat4::IDENTITY, 2.0e-5));
    let point = vec3(0.5, -0.25, 2.0);
    near(
        inverse
            .try_project_point3(general.try_project_point3(point).unwrap())
            .unwrap(),
        point,
        2.0e-5,
    );
}

#[test]
fn inverse_accepts_nearly_dependent_axes_without_an_epsilon_cutoff() {
    // The distinction is one f32 ULP, but still represents an invertible matrix.
    let next = f32::from_bits(1.0_f32.to_bits() + 1);
    let matrix = Mat4::from_cols(
        vec4(1.0, 1.0, 0.0, 0.0),
        vec4(1.0, next, 0.0, 0.0),
        Vec4::Z,
        Vec4::W,
    );
    let inverse = matrix.checked_inverse().unwrap();
    assert_eq!(matrix * inverse, Mat4::IDENTITY);
    assert_eq!(inverse * matrix, Mat4::IDENTITY);
    let point = Vec3::Y;
    assert_eq!(
        inverse.try_project_point3(matrix.try_project_point3(point).unwrap()),
        Ok(point)
    );
}

#[test]
fn finite_outputs_may_underflow_to_zero_as_documented() {
    let tiny = f32::from_bits(1);
    let matrix = Mat4::from_cols(
        vec4(f32::MAX, 0.0, 0.0, 0.0),
        vec4(tiny, f32::MAX, 0.0, 0.0),
        Vec4::Z,
        Vec4::W,
    );
    let inverse = matrix.checked_inverse().unwrap();
    assert_eq!(inverse.y_axis.x, 0.0);
    assert!(inverse.x_axis.x > 0.0 && inverse.y_axis.y > 0.0);
    assert!((matrix * inverse).abs_diff_eq(Mat4::IDENTITY, 1.0e-6));
    assert_eq!(
        Mat4::from_scale(Vec3::splat(tiny)).try_project_point3(Vec3::splat(0.25)),
        Ok(Vec3::ZERO)
    );
}

#[test]
fn projection_checks_inputs_w_and_divided_range() {
    assert_eq!(
        Mat4::NAN.try_project_point3(Vec3::ZERO),
        Err(MatrixError::NonFiniteInput { input: "matrix" })
    );
    for point in [Vec3::NAN, Vec3::INFINITY] {
        assert_eq!(
            Mat4::IDENTITY.try_project_point3(point),
            Err(MatrixError::NonFiniteInput { input: "point" })
        );
    }
    let perspective = Mat4::try_perspective_rh(1.0, 1.0, 0.1, 100.0).unwrap();
    assert_eq!(
        perspective.try_project_point3(Vec3::X),
        Err(MatrixError::ZeroHomogeneousW)
    );
    let tiny_w = Mat4::from_diagonal(vec4(1.0, 1.0, 1.0, f32::from_bits(1)));
    assert_eq!(
        tiny_w.try_project_point3(Vec3::X),
        Err(MatrixError::ResultOutOfRange)
    );
    assert_eq!(tiny_w.try_project_point3(Vec3::ZERO), Ok(Vec3::ZERO));
    // A singular transform may still project individual points successfully.
    assert_eq!(
        Mat4::from_scale(Vec3::ZERO).try_project_point3(Vec3::ONE),
        Ok(Vec3::ZERO)
    );
}

#[test]
fn projection_is_invariant_to_extreme_homogeneous_scales() {
    for (scale, point) in [
        (f32::from_bits(1), vec3(0.25, -0.5, 0.75)),
        (f32::MAX, vec3(2.0, -3.0, 4.0)),
        (-f32::MAX, vec3(2.0, -3.0, 4.0)),
    ] {
        assert_eq!(
            (Mat4::IDENTITY * scale).try_project_point3(point),
            Ok(point)
        );
    }
    let projection = Mat4::try_perspective_rh(1.0, 1.0, 0.1, 100.0).unwrap();
    // Behind-camera and outside-frustum points are permitted, not visibility culled.
    assert!(projection.try_project_point3(vec3(1.0e5, 0.0, 1.0)).is_ok());
}

#[test]
fn checked_views_reject_degenerate_cameras() {
    for constructor in [
        Mat4::try_look_to_rh as fn(_, _, _) -> _,
        Mat4::try_look_to_lh,
        Mat4::try_look_at_rh,
        Mat4::try_look_at_lh,
    ] {
        assert_eq!(
            constructor(Vec3::ZERO, Vec3::ZERO, Vec3::Y),
            Err(MatrixError::DegenerateDirection)
        );
        for up in [Vec3::ZERO, Vec3::Z, Vec3::NEG_Z] {
            assert_eq!(
                constructor(Vec3::ZERO, Vec3::Z, up),
                Err(MatrixError::DegenerateUp)
            );
        }
        assert_eq!(
            constructor(Vec3::NAN, Vec3::Z, Vec3::Y),
            Err(MatrixError::NonFiniteInput { input: "eye" })
        );
        assert_eq!(
            constructor(Vec3::ZERO, Vec3::Z, Vec3::INFINITY),
            Err(MatrixError::NonFiniteInput { input: "up" })
        );
    }
    assert_eq!(
        Mat4::try_look_at_rh(Vec3::ZERO, Vec3::INFINITY, Vec3::Y),
        Err(MatrixError::NonFiniteInput { input: "target" })
    );
    assert_eq!(
        Mat4::try_look_to_lh(Vec3::ZERO, Vec3::NAN, Vec3::Y),
        Err(MatrixError::NonFiniteInput { input: "direction" })
    );
    let direction = vec3(2.0, 3.0, -7.0);
    assert_eq!(
        Mat4::try_look_to_rh(Vec3::ZERO, direction, direction * 2.0),
        Err(MatrixError::DegenerateUp)
    );
    assert!(Mat4::look_at_rh(Vec3::ZERO, Vec3::ZERO, Vec3::Y).is_nan());
}

#[test]
fn checked_views_handle_tiny_and_extreme_finite_basis_vectors() {
    for magnitude in [f32::from_bits(1), 1.0e-30, f32::MAX] {
        let rh = Mat4::try_look_to_rh(Vec3::ZERO, Vec3::NEG_Z * magnitude, Vec3::Y * magnitude);
        assert_eq!(rh, Ok(Mat4::IDENTITY));
        assert_eq!(
            Mat4::try_look_to_lh(Vec3::ZERO, Vec3::Z * magnitude, Vec3::Y * magnitude),
            Ok(Mat4::IDENTITY)
        );
    }
    // target - eye overflows f32, but the view's coefficients fit.
    let eye = vec3(0.0, 0.0, f32::MAX);
    let view = Mat4::try_look_at_rh(eye, -eye, Vec3::Y).unwrap();
    assert_eq!(view.try_project_point3(eye), Ok(Vec3::ZERO));
    // The final translation, unlike the direction above, cannot fit f32.
    assert_eq!(
        Mat4::try_look_at_rh(Vec3::splat(f32::MAX), Vec3::ZERO, Vec3::Y),
        Err(MatrixError::ResultOutOfRange)
    );
}

#[test]
fn checked_views_accept_nonparallel_up_across_extreme_magnitudes() {
    for (direction, up) in [
        (Vec3::NEG_Z, vec3(0.0, f32::from_bits(1), -1.0)),
        (Vec3::NEG_Z * f32::MAX, Vec3::Y * f32::from_bits(1)),
        (Vec3::NEG_Z * f32::from_bits(1), Vec3::Y * f32::MAX),
    ] {
        // Cross products/lengths in f32 would lose these valid camera axes.
        assert_eq!(
            Mat4::try_look_to_rh(Vec3::ZERO, direction, up),
            Ok(Mat4::IDENTITY)
        );
        assert_eq!(
            Mat4::try_look_at_rh(Vec3::ZERO, direction, up),
            Ok(Mat4::IDENTITY)
        );
        assert_eq!(
            Mat4::try_look_to_lh(Vec3::ZERO, -direction, up),
            Ok(Mat4::IDENTITY)
        );
        assert_eq!(
            Mat4::try_look_at_lh(Vec3::ZERO, -direction, up),
            Ok(Mat4::IDENTITY)
        );
    }
}

#[test]
fn checked_views_match_existing_handedness_and_round_trip_points() {
    let eye = vec3(4.0, 2.0, 7.0);
    let target = vec3(-1.0, 1.0, 0.0);
    for (view, old, forward_sign) in [
        (
            Mat4::try_look_at_rh(eye, target, Vec3::Y).unwrap(),
            Mat4::look_at_rh(eye, target, Vec3::Y),
            -1.0,
        ),
        (
            Mat4::try_look_at_lh(eye, target, Vec3::Y).unwrap(),
            Mat4::look_at_lh(eye, target, Vec3::Y),
            1.0,
        ),
    ] {
        assert!(view.abs_diff_eq(old, 1.0e-6));
        near(view.try_project_point3(eye).unwrap(), Vec3::ZERO, 1.0e-6);
        let forward = view.try_project_point3(target).unwrap();
        assert!(forward.z * forward_sign > 0.0);
        assert!(forward.x.abs() < 1.0e-6 && forward.y.abs() < 1.0e-6);
        near(
            view.checked_inverse()
                .unwrap()
                .try_project_point3(forward)
                .unwrap(),
            target,
            2.0e-6,
        );
    }
}

#[test]
fn perspective_reports_invalid_parameters_without_panics() {
    for constructor in [
        Mat4::try_perspective_rh as fn(_, _, _, _) -> _,
        Mat4::try_perspective_rh_gl,
        Mat4::try_perspective_lh,
    ] {
        for (values, parameter) in [
            ([0.0, 1.0, 0.1, 10.0], "fov_y"),
            ([PI, 1.0, 0.1, 10.0], "fov_y"),
            ([1.0, 0.0, 0.1, 10.0], "aspect"),
            ([1.0, -1.0, 0.1, 10.0], "aspect"),
            ([1.0, 1.0, 0.0, 10.0], "near"),
            ([1.0, 1.0, 1.0, 1.0], "far"),
            ([1.0, 1.0, 1.0, -1.0], "far"),
        ] {
            let [fov, aspect, near, far] = values;
            assert_eq!(
                constructor(fov, aspect, near, far),
                Err(MatrixError::InvalidPerspective { parameter })
            );
        }
        for (i, input) in ["fov_y", "aspect", "near", "far"].into_iter().enumerate() {
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut values = [1.0, 1.0, 0.1, 10.0];
                values[i] = invalid;
                let [fov, aspect, near, far] = values;
                assert_eq!(
                    constructor(fov, aspect, near, far),
                    Err(MatrixError::NonFiniteInput { input })
                );
            }
        }
    }
}

#[test]
fn perspective_preserves_depth_conventions_and_point_round_trips() {
    let near_plane = 0.5;
    let far = 100.0;
    for (projection, old, sign, near_depth) in [
        (
            Mat4::try_perspective_rh_gl(FRAC_PI_2, 2.0, near_plane, far).unwrap(),
            Mat4::perspective_rh_gl(FRAC_PI_2, 2.0, near_plane, far),
            -1.0,
            -1.0,
        ),
        (
            Mat4::try_perspective_rh(FRAC_PI_2, 2.0, near_plane, far).unwrap(),
            Mat4::perspective_rh(FRAC_PI_2, 2.0, near_plane, far),
            -1.0,
            0.0,
        ),
        (
            Mat4::try_perspective_lh(FRAC_PI_2, 2.0, near_plane, far).unwrap(),
            Mat4::perspective_lh(FRAC_PI_2, 2.0, near_plane, far),
            1.0,
            0.0,
        ),
    ] {
        assert!(projection.abs_diff_eq(old, 2.0e-7));
        near(
            projection
                .try_project_point3(vec3(0.0, 0.0, sign * near_plane))
                .unwrap(),
            vec3(0.0, 0.0, near_depth),
            1.0e-6,
        );
        near(
            projection
                .try_project_point3(vec3(0.0, 0.0, sign * far))
                .unwrap(),
            Vec3::Z,
            1.0e-6,
        );
        let inverse = projection.checked_inverse().unwrap();
        for point in [vec3(0.25, -0.5, sign), vec3(2.0, 1.0, 12.0 * sign)] {
            near(
                inverse
                    .try_project_point3(projection.try_project_point3(point).unwrap())
                    .unwrap(),
                point,
                5.0e-5,
            );
        }
    }
}

#[test]
fn perspective_avoids_intermediate_overflow_and_reports_unrepresentable_scales() {
    let projection = Mat4::try_perspective_rh_gl(1.0, 1.0, 1.0e20, 2.0e20).unwrap();
    near(
        projection
            .try_project_point3(vec3(0.0, 0.0, -1.0e20))
            .unwrap(),
        Vec3::NEG_Z,
        1.0e-6,
    );
    assert_eq!(
        Mat4::try_perspective_rh(f32::from_bits(1), 1.0, 0.1, 100.0),
        Err(MatrixError::ResultOutOfRange)
    );
    assert_eq!(
        Mat4::try_perspective_rh(f32::from_bits(PI.to_bits() - 1), f32::MAX, 0.1, 100.0),
        Err(MatrixError::ResultOutOfRange)
    );
    assert_eq!(
        Mat4::try_perspective_rh_gl(1.0, 1.0, f32::MAX * 0.5, f32::MAX),
        Err(MatrixError::ResultOutOfRange)
    );
}

#[test]
fn infinite_projections_validate_near_directly_and_keep_depth_conventions() {
    for (constructor, reverse) in [
        (Mat4::try_perspective_infinite_rh as fn(_, _, _) -> _, false),
        (Mat4::try_perspective_infinite_reverse_rh, true),
    ] {
        for near_plane in [f32::from_bits(1), 0.5, f32::MAX] {
            let projection = constructor(1.0, 1.0, near_plane).unwrap();
            let projected = projection
                .try_project_point3(vec3(0.0, 0.0, -near_plane))
                .unwrap();
            assert_eq!(projected.z, if reverse { 1.0 } else { 0.0 });
        }
        assert_eq!(
            constructor(1.0, 1.0, 0.0),
            Err(MatrixError::InvalidPerspective { parameter: "near" })
        );
        assert_eq!(
            constructor(1.0, 1.0, f32::INFINITY),
            Err(MatrixError::NonFiniteInput { input: "near" })
        );
        let projection = constructor(1.0, 1.0, 0.5).unwrap();
        let far = projection
            .try_project_point3(vec3(0.0, 0.0, -1.0e8))
            .unwrap()
            .z;
        assert!((far - if reverse { 0.0 } else { 1.0 }).abs() < 1.0e-7);
    }
}

#[test]
fn orthographic_reports_degenerate_axes_and_nonfinite_bounds() {
    for constructor in [
        Mat4::try_orthographic_rh_gl as fn(_, _, _, _, _, _) -> _,
        Mat4::try_orthographic_rh,
        Mat4::try_orthographic_lh,
    ] {
        for (pair, axis) in ["x", "y", "z"].into_iter().enumerate() {
            let mut bounds = [-1.0, 1.0, -1.0, 1.0, 0.1, 10.0];
            bounds[pair * 2 + 1] = bounds[pair * 2];
            let [l, r, b, t, n, f] = bounds;
            assert_eq!(
                constructor(l, r, b, t, n, f),
                Err(MatrixError::DegenerateOrthographic { axis })
            );
        }
        for (index, input) in ["left", "right", "bottom", "top", "near", "far"]
            .into_iter()
            .enumerate()
        {
            let mut bounds = [-1.0, 1.0, -1.0, 1.0, 0.1, 10.0];
            bounds[index] = f32::NAN;
            let [l, r, b, t, n, f] = bounds;
            assert_eq!(
                constructor(l, r, b, t, n, f),
                Err(MatrixError::NonFiniteInput { input })
            );
        }
    }
    assert_eq!(
        Mat4::try_orthographic_rh(0.0, f32::from_bits(1), -1.0, 1.0, 0.1, 10.0),
        Err(MatrixError::ResultOutOfRange)
    );
}

#[test]
fn orthographic_preserves_reversed_ranges_and_extreme_finite_bounds() {
    for (projection, sign, near_depth) in [
        (
            Mat4::try_orthographic_rh_gl(4.0, -2.0, 3.0, -1.0, 10.0, -0.5).unwrap(),
            -1.0,
            -1.0,
        ),
        (
            Mat4::try_orthographic_rh(4.0, -2.0, 3.0, -1.0, 10.0, -0.5).unwrap(),
            -1.0,
            0.0,
        ),
        (
            Mat4::try_orthographic_lh(4.0, -2.0, 3.0, -1.0, 10.0, -0.5).unwrap(),
            1.0,
            0.0,
        ),
    ] {
        near(
            projection
                .try_project_point3(vec3(4.0, 3.0, sign * 10.0))
                .unwrap(),
            vec3(-1.0, -1.0, near_depth),
            1.0e-6,
        );
        near(
            projection
                .try_project_point3(vec3(-2.0, -1.0, sign * -0.5))
                .unwrap(),
            Vec3::ONE,
            1.0e-6,
        );
        let point = vec3(1.0, 2.0, 3.0);
        near(
            projection
                .checked_inverse()
                .unwrap()
                .try_project_point3(projection.try_project_point3(point).unwrap())
                .unwrap(),
            point,
            1.0e-6,
        );
    }
    let wide =
        Mat4::try_orthographic_rh_gl(-f32::MAX, f32::MAX, -f32::MAX, f32::MAX, 0.0, 1.0).unwrap();
    near(
        wide.try_project_point3(vec3(f32::MAX, -f32::MAX, -1.0))
            .unwrap(),
        vec3(1.0, -1.0, 1.0),
        1.0e-6,
    );
    // 1 / MAX rounds to 2^-128; its reciprocal 2^128 overflows f32.
    // Constructing a usable projection does not promise a representable inverse.
    assert_eq!(wide.checked_inverse(), Err(MatrixError::ResultOutOfRange));

    // A slightly smaller range still needs f64 to subtract its endpoints, and
    // leaves enough headroom for both the projection and its inverse to fit f32.
    let bound = f32::MAX * 0.75;
    let wide = Mat4::try_orthographic_rh_gl(-bound, bound, -bound, bound, 0.0, 1.0).unwrap();
    let inverse = wide.checked_inverse().unwrap();
    let point = vec3(bound * 0.5, -bound * 0.25, -0.75);
    let restored = inverse
        .try_project_point3(wide.try_project_point3(point).unwrap())
        .unwrap();
    near(restored / bound, point / bound, 1.0e-6);
    assert!((restored.z - point.z).abs() < 1.0e-6);
}

#[test]
fn composed_model_view_projection_round_trips() {
    let model = Mat4::from_scale_rotation_translation(
        vec3(2.0, -0.5, 1.5),
        Quat::from_rotation_y(0.4),
        vec3(0.5, 0.0, -1.0),
    );
    let view = Mat4::try_look_at_rh(vec3(4.0, 3.0, 8.0), Vec3::ZERO, Vec3::Y).unwrap();
    let projection = Mat4::try_perspective_rh(1.0, 1.5, 0.5, 100.0).unwrap();
    let matrix = projection * view * model;
    let inverse = matrix.checked_inverse().unwrap();
    for x in -2..=2 {
        for y in -2..=2 {
            let point = vec3(x as f32 * 0.25, y as f32 * 0.25, -0.25);
            near(
                inverse
                    .try_project_point3(matrix.try_project_point3(point).unwrap())
                    .unwrap(),
                point,
                3.0e-5,
            );
        }
    }
}

#[test]
fn errors_provide_display_and_standard_error_integration() {
    let error = MatrixError::InvalidPerspective {
        parameter: "aspect",
    };
    let diagnostic: &dyn std::error::Error = &error;
    assert!(diagnostic.to_string().contains("aspect > 0"));
    assert!(
        MatrixError::DegenerateUp
            .to_string()
            .contains("nonparallel")
    );
}
