use ferravex_math::{EulerRot, Mat4, Quat, Rect, Vec2, Vec3, Vec4, vec2, vec3, vec4};
use std::f32::consts::{FRAC_PI_2, PI};

fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 2.0e-5, "{a} != {b}");
}
fn vector_near(a: Vec3, b: Vec3) {
    assert!(a.abs_diff_eq(b, 2.0e-5), "{a:?} != {b:?}");
}

#[test]
fn vector_arithmetic_and_dimension_changes() {
    let mut a = vec3(2.0, 4.0, 8.0);
    a *= vec3(2.0, 0.5, 0.25);
    a += 1.0;
    a /= 2.0;
    a[1] = 7.0;
    assert_eq!(a, vec3(2.5, 7.0, 1.5));
    assert_eq!(a.extend(3.0).truncate(), a);
    assert_eq!(a.zyx(), vec3(1.5, 7.0, 2.5));
    assert_eq!(8.0 / vec2(2.0, 4.0), vec2(4.0, 2.0));
    assert_eq!(a.to_array(), [2.5, 7.0, 1.5]);
    assert!(a.cmpgt(Vec3::ONE).all());
    assert_eq!(vec2(-1.25, 2.5).fract(), vec2(0.75, 0.5));
}

#[test]
fn normalization_handles_zero_subnormal_and_extreme_finite_values() {
    assert!(Vec3::ZERO.try_normalize().is_none());
    assert_eq!(Vec3::ZERO.normalize_or_zero(), Vec3::ZERO);
    assert!(Vec4::NAN.try_normalize().is_none());
    assert!(Vec2::INFINITY.normalize().is_nan());
    for v in [
        vec3(f32::MAX, f32::MAX, 0.0),
        vec3(f32::from_bits(1), 0.0, 0.0),
        vec3(3.0, 4.0, 5.0),
    ] {
        let n = v.try_normalize().unwrap();
        assert!(n.is_finite());
        near(n.length(), 1.0);
    }
    near(vec2(3.0, 4.0).clamp_length_max(2.0).length(), 2.0);
    assert_eq!(Vec3::ZERO.clamp_length_min(3.0), Vec3::ZERO);
}

#[test]
fn cross_products_and_projections_obey_geometry() {
    let a = vec3(3.0, 2.0, -4.0);
    let b = vec3(-1.0, 5.0, 2.0);
    let normal = a.cross(b);
    near(normal.dot(a), 0.0);
    near(normal.dot(b), 0.0);
    assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
    vector_near(a.project_onto(b) + a.reject_from(b), a);
    vector_near(vec3(1.0, -1.0, 0.0).reflect(Vec3::Y), vec3(1.0, 1.0, 0.0));
    near(Vec2::X.angle_between(Vec2::Y), FRAC_PI_2);
    near(Vec2::Y.angle_between(Vec2::X), -FRAC_PI_2);
    assert!(
        Vec2::from_angle(FRAC_PI_2)
            .rotate(Vec2::X)
            .abs_diff_eq(Vec2::Y, 1.0e-6)
    );
}

#[test]
fn quaternion_rotation_composition_and_inverse() {
    let x = Quat::from_rotation_x(FRAC_PI_2);
    let y = Quat::from_rotation_y(FRAC_PI_2);
    vector_near(x * Vec3::Y, Vec3::Z);
    vector_near(y * Vec3::Z, Vec3::X);
    vector_near((x * y) * Vec3::X, x * (y * Vec3::X));
    let q = Quat::from_euler(EulerRot::XYZ, 0.3, -0.7, 1.2);
    vector_near(
        q.inverse() * (q * vec3(4.0, -2.0, 1.0)),
        vec3(4.0, -2.0, 1.0),
    );
    assert!(((q * 3.0) * (q * 3.0).inverse()).abs_diff_eq(Quat::IDENTITY, 2.0e-6));
    assert!(Quat::from_xyzw(0.0, 0.0, 0.0, 0.0).try_inverse().is_none());
    assert!(Quat::from_axis_angle(Vec3::ZERO, 1.0).is_nan());
}

#[test]
fn rotation_arc_handles_opposites_parallel_and_arbitrary_vectors() {
    for (a, b) in [
        (Vec3::X, Vec3::NEG_X),
        (Vec3::Y, Vec3::Y),
        (vec3(2.0, 3.0, 4.0), vec3(-4.0, 0.2, 1.0)),
    ] {
        let q = Quat::from_rotation_arc(a, b);
        assert!(q.is_normalized());
        vector_near(q * a.normalize(), b.normalize());
    }
    assert!(Quat::from_rotation_arc(Vec3::ZERO, Vec3::Y).is_nan());
}

#[test]
fn slerp_shortest_path_and_equivalent_quaternion_signs() {
    let end = Quat::from_rotation_z(FRAC_PI_2);
    vector_near(
        Quat::IDENTITY.slerp(end, 0.5) * Vec3::X,
        vec3(0.5_f32.sqrt(), 0.5_f32.sqrt(), 0.0),
    );
    for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let q = end.slerp(-end, t);
        assert!(q.is_normalized());
        vector_near(q * Vec3::X, Vec3::Y);
    }
    let near_rotation = Quat::from_rotation_z(0.00001);
    assert!(Quat::IDENTITY.slerp(near_rotation, 0.5).is_finite());
}

#[test]
fn matrix_storage_and_affine_vector_semantics() {
    let t = Mat4::from_translation(vec3(1.0, 2.0, 3.0));
    assert_eq!(t.to_cols_array()[12..16], [1.0, 2.0, 3.0, 1.0]);
    vector_near(t.transform_point3(Vec3::ZERO), vec3(1.0, 2.0, 3.0));
    vector_near(t.transform_vector3(Vec3::X), Vec3::X);
    assert_eq!(t.transpose().transpose(), t);
    assert_eq!(Mat4::from_cols_array(&t.to_cols_array()), t);
    let composed = t * Mat4::from_rotation_z(FRAC_PI_2) * Mat4::from_scale(vec3(2.0, 3.0, 4.0));
    vector_near(composed.transform_point3(Vec3::X), vec3(1.0, 4.0, 3.0));
    near(composed.determinant(), 24.0);
}

#[test]
fn inverse_supports_general_matrices_and_rejects_singular_inputs() {
    let matrix = Mat4::from_cols(
        vec4(2.0, 1.0, 0.0, 0.3),
        vec4(-1.0, 3.0, 2.0, 0.0),
        vec4(0.4, 1.0, 4.0, -0.2),
        vec4(5.0, 2.0, -1.0, 1.0),
    );
    let inverse = matrix.try_inverse().unwrap();
    assert!((matrix * inverse).abs_diff_eq(Mat4::IDENTITY, 2.0e-5));
    assert!((inverse * matrix).abs_diff_eq(Mat4::IDENTITY, 2.0e-5));
    assert!(Mat4::ZERO.try_inverse().is_none());
    assert!(Mat4::from_scale(vec3(1.0, 0.0, 1.0)).inverse().is_nan());
    assert!(Mat4::NAN.try_inverse().is_none());
    let pivot_swap = Mat4::from_cols(Vec4::Y, Vec4::X, Vec4::W, Vec4::Z);
    assert_eq!(pivot_swap.inverse(), pivot_swap);
}

#[test]
fn perspective_depth_and_fov_conventions() {
    let near_plane = 0.5;
    let far = 100.0;
    let gl = Mat4::perspective_rh_gl(FRAC_PI_2, 2.0, near_plane, far);
    near(gl.project_point3(vec3(0.0, 0.0, -near_plane)).z, -1.0);
    near(gl.project_point3(vec3(0.0, 0.0, -far)).z, 1.0);
    near(
        gl.project_point3(vec3(near_plane * 2.0, near_plane, -near_plane))
            .x,
        1.0,
    );
    near(
        gl.project_point3(vec3(near_plane * 2.0, near_plane, -near_plane))
            .y,
        1.0,
    );
    let web = Mat4::perspective_rh(FRAC_PI_2, 1.0, near_plane, far);
    near(web.project_point3(vec3(0.0, 0.0, -near_plane)).z, 0.0);
    near(web.project_point3(vec3(0.0, 0.0, -far)).z, 1.0);
    let left = Mat4::perspective_lh(FRAC_PI_2, 1.0, near_plane, far);
    near(left.project_point3(vec3(0.0, 0.0, near_plane)).z, 0.0);
    near(left.project_point3(vec3(0.0, 0.0, far)).z, 1.0);
    let reverse = Mat4::perspective_infinite_reverse_rh(FRAC_PI_2, 1.0, near_plane);
    near(reverse.project_point3(vec3(0.0, 0.0, -near_plane)).z, 1.0);
    assert!(reverse.project_point3(vec3(0.0, 0.0, -1.0e8)).z < 1.0e-7);
}

#[test]
fn look_at_and_orthographic_conventions() {
    let eye = vec3(4.0, 2.0, 7.0);
    let target = vec3(-1.0, 1.0, 0.0);
    let view = Mat4::look_at_rh(eye, target, Vec3::Y);
    vector_near(view.transform_point3(eye), Vec3::ZERO);
    let forward = view.transform_point3(target);
    near(forward.x, 0.0);
    near(forward.y, 0.0);
    assert!(forward.z < 0.0);
    let ortho = Mat4::orthographic_rh_gl(-2.0, 6.0, -1.0, 3.0, 0.5, 10.0);
    vector_near(
        ortho.project_point3(vec3(-2.0, -1.0, -0.5)),
        vec3(-1.0, -1.0, -1.0),
    );
    vector_near(ortho.project_point3(vec3(6.0, 3.0, -10.0)), Vec3::ONE);
    assert!(Mat4::look_at_rh(eye, eye, Vec3::Y).is_nan());
}

#[test]
fn seeded_transform_properties_and_decomposition() {
    let mut seed = 73_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) as u32 as f64 / u32::MAX as f64) as f32
    };
    for index in 0..256 {
        let translation = vec3(
            random() * 20.0 - 10.0,
            random() * 20.0 - 10.0,
            random() * 20.0 - 10.0,
        );
        let q = Quat::from_euler(EulerRot::XYZ, random() * PI, random() * PI, random() * PI);
        let scale = vec3(
            (random() + 0.1) * if index % 2 == 0 { -1.0 } else { 1.0 },
            random() + 0.1,
            random() + 0.1,
        );
        let matrix = Mat4::from_scale_rotation_translation(scale, q, translation);
        let inverse = matrix.try_inverse().unwrap();
        assert!((matrix * inverse).abs_diff_eq(Mat4::IDENTITY, 2.0e-5));
        let (s, r, t) = matrix.try_to_scale_rotation_translation().unwrap();
        assert!(Mat4::from_scale_rotation_translation(s, r, t).abs_diff_eq(matrix, 2.0e-5));
        let point = vec3(random(), random(), random());
        vector_near(
            inverse.transform_point3(matrix.transform_point3(point)),
            point,
        );
        near((q * point).length(), point.length());
    }
    let mut shear = Mat4::IDENTITY;
    shear.y_axis.x = 0.2;
    assert!(shear.try_to_scale_rotation_translation().is_none());
}

#[test]
fn rectangle_edges_do_not_count_as_penetration() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(!a.overlaps(&Rect::new(10.0, 0.0, 3.0, 3.0)));
    assert!(!a.overlaps(&Rect::new(5.0, 5.0, 0.0, 3.0)));
    assert!(a.overlaps(&Rect::new(9.0, 9.0, 3.0, 3.0)));
    assert_eq!(
        a.intersect(Rect::new(9.0, 9.0, 3.0, 3.0)),
        Some(Rect::new(9.0, 9.0, 1.0, 1.0))
    );
    assert!(a.contains(vec2(10.0, 10.0)));
}

#[test]
#[should_panic(expected = "invalid perspective frustum")]
fn perspective_rejects_zero_aspect() {
    let _ = Mat4::perspective_rh(FRAC_PI_2, 0.0, 0.1, 100.0);
}
