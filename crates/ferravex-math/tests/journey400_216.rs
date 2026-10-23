use ferravex_math::affine::{AffineError, AffineSliceError, PreparedAffine3D};
use ferravex_math::{Mat4, MatrixError, Vec3, Vec4, vec3, vec4};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

fn prepare(model: Mat4) -> PreparedAffine3D {
    PreparedAffine3D::try_new(model).unwrap()
}

fn sheared(mirror: bool) -> Mat4 {
    Mat4::from_cols(
        vec4(if mirror { -2.0 } else { 2.0 }, 0.0, 0.0, 0.0),
        vec4(1.0, 3.0, 0.0, 0.0),
        vec4(0.5, -0.25, 0.5, 0.0),
        vec4(10.0, -5.0, 8.0, 1.0),
    )
}

fn close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, 2.0e-6),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn identity_translation_and_vector_distinction() {
    let identity = prepare(Mat4::IDENTITY);
    let v = vec3(2.0, -3.0, 4.0);
    assert_eq!(identity.transform_point(v), Ok(v));
    assert_eq!(identity.transform_vector(v), Ok(v));
    close(identity.transform_normal(v).unwrap(), v.normalize());
    let translation = vec3(20.0, -50.0, 30.0);
    let p = prepare(Mat4::from_translation(translation));
    assert_eq!(p.transform_point(v), Ok(v + translation));
    assert_eq!(p.transform_vector(v), Ok(v));
    assert_eq!(p.normal_matrix(), Mat4::IDENTITY);
    assert!(!p.is_reflected());
    assert_eq!(p.transform_vector(Vec3::ZERO), Ok(Vec3::ZERO));
}

#[test]
fn model_and_normal_cache_follow_existing_inverse_transpose() {
    for mirrored in [false, true] {
        let model = sheared(mirrored);
        let p = prepare(model);
        assert_eq!(p.model(), model);
        let expected = model.checked_inverse().unwrap().transpose();
        let normal_matrix = p.normal_matrix();
        for axis in Vec3::AXES {
            close(
                normal_matrix.transform_vector3(axis),
                expected.transform_vector3(axis),
            );
        }
        assert_eq!(normal_matrix.w_axis, Vec4::W);
        assert_eq!(normal_matrix.row(3), Vec4::W);
        assert_eq!(p.is_reflected(), mirrored);
    }
}

#[test]
fn shear_nonuniform_scale_and_mirrors_keep_normals_perpendicular() {
    let tangents = [vec3(1.0, 2.0, -1.0), vec3(-2.0, 0.5, 3.0)];
    let normal = tangents[0].cross(tangents[1]);
    for mirrored in [false, true] {
        let p = prepare(sheared(mirrored));
        let a = p.transform_vector(tangents[0]).unwrap();
        let b = p.transform_vector(tangents[1]).unwrap();
        let n = p.transform_normal(normal).unwrap();
        assert!(n.is_normalized());
        assert!(n.dot(a).abs() < 2.0e-6);
        assert!(n.dot(b).abs() < 2.0e-6);
        let winding = a.cross(b).normalize();
        assert!(winding.dot(n) * if mirrored { -1.0 } else { 1.0 } > 0.99999);
        // Positive rescaling of an authored normal must not change orientation.
        close(p.transform_normal(normal * 32.0).unwrap(), n);
        close(p.transform_normal(-normal).unwrap(), -n);
    }
}

#[test]
fn rotations_and_even_reflections_preserve_orientation() {
    let rotation = Mat4::from_rotation_y(0.75) * Mat4::from_rotation_z(-0.2);
    let p = prepare(rotation * Mat4::from_scale(vec3(-2.0, -3.0, 4.0)));
    assert!(!p.is_reflected());
    for axis in Vec3::AXES {
        close(
            p.transform_vector(axis).unwrap().normalize(),
            p.transform_normal(axis).unwrap(),
        );
    }
}

#[test]
fn preparation_rejects_nonfinite_and_every_nonaffine_row_component() {
    for column in 0..4 {
        for row in 0..4 {
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut model = Mat4::IDENTITY;
                model[column][row] = invalid;
                assert_eq!(
                    PreparedAffine3D::try_new(model),
                    Err(AffineError::NonFiniteInput { input: "model" })
                );
            }
        }
        let mut model = Mat4::IDENTITY;
        model[column][3] = if column == 3 {
            1.000001
        } else {
            f32::from_bits(1)
        };
        assert_eq!(
            PreparedAffine3D::try_new(model),
            Err(AffineError::NonAffine)
        );
    }
    let mut signed_zero = Mat4::IDENTITY;
    signed_zero.x_axis.w = -0.0;
    signed_zero.y_axis.w = -0.0;
    signed_zero.z_axis.w = -0.0;
    assert!(PreparedAffine3D::try_new(signed_zero).is_ok());
    assert_eq!(
        PreparedAffine3D::try_new(Mat4::perspective_rh_gl(1.0, 1.0, 0.1, 100.0)),
        Err(AffineError::NonAffine)
    );
}

#[test]
fn singular_and_inverse_range_errors_reuse_checked_inverse() {
    for scale in [
        vec3(0.0, 1.0, 1.0),
        vec3(1.0, 0.0, 1.0),
        vec3(1.0, 1.0, 0.0),
    ] {
        let m = Mat4::from_scale(scale);
        assert_eq!(
            PreparedAffine3D::try_new(m),
            Err(AffineError::Matrix(m.checked_inverse().unwrap_err()))
        );
    }
    let dependent = Mat4::from_cols(Vec4::X, Vec4::X, Vec4::Z, Vec4::W);
    assert!(matches!(
        PreparedAffine3D::try_new(dependent),
        Err(AffineError::Matrix(MatrixError::Singular { .. }))
    ));
    let tiny = Mat4::from_scale(vec3(f32::from_bits(1), 1.0, 1.0));
    assert_eq!(
        PreparedAffine3D::try_new(tiny),
        Err(AffineError::Matrix(MatrixError::ResultOutOfRange))
    );
}

#[test]
fn reflection_sign_survives_determinant_underflow_and_overflow() {
    for scale in [1.0e-20, 1.0e20] {
        for mirrored in [false, true] {
            let m = Mat4::from_scale(vec3(if mirrored { -scale } else { scale }, scale, scale));
            let p = prepare(m);
            if scale < 1.0 {
                assert_eq!(m.determinant().abs(), 0.0);
            } else {
                assert!(m.determinant().is_infinite());
            }
            assert_eq!(p.is_reflected(), mirrored);
            assert_eq!(
                p.transform_normal(Vec3::X),
                Ok(if mirrored { -Vec3::X } else { Vec3::X })
            );
        }
    }
}

#[test]
fn forward_queries_do_not_require_representable_inverse_translation() {
    let m = Mat4::from_translation(vec3(f32::MAX, 0.0, 0.0))
        * Mat4::from_scale(vec3(1.0e-20, 1.0, 1.0));
    assert_eq!(m.checked_inverse(), Err(MatrixError::ResultOutOfRange));
    let p = prepare(m);
    assert_eq!(p.transform_point(Vec3::ZERO), Ok(vec3(f32::MAX, 0.0, 0.0)));
    assert_eq!(p.transform_normal(Vec3::X), Ok(Vec3::X));
}

#[test]
fn normal_wide_rescaling_handles_extreme_and_subnormal_inputs() {
    let p = prepare(Mat4::from_scale(vec3(1.0e-20, 2.0e-20, 4.0e-20)));
    assert!(
        p.normal_matrix()
            .transform_vector3(Vec3::splat(f32::MAX))
            .to_array()
            .into_iter()
            .all(f32::is_infinite)
    );
    let expected = vec3(1.0, 0.5, 0.25).normalize();
    close(p.transform_normal(Vec3::splat(f32::MAX)).unwrap(), expected);
    let q = prepare(Mat4::from_scale(vec3(1.0e20, 2.0e20, 4.0e20)));
    let tiny = Vec3::splat(f32::from_bits(1));
    assert_eq!(q.normal_matrix().transform_vector3(tiny), Vec3::ZERO);
    close(q.transform_normal(tiny).unwrap(), expected);
    assert_eq!(
        prepare(Mat4::IDENTITY).transform_normal(-Vec3::X * f32::from_bits(1)),
        Ok(-Vec3::X)
    );
}

#[test]
fn finite_inputs_use_wide_products_and_reject_unrepresentable_outputs() {
    let m = Mat4::from_cols(
        vec4(2.0, 0.0, 0.0, 0.0),
        vec4(-2.0, 1.0, 0.0, 0.0),
        Vec4::Z,
        Vec4::W,
    );
    let p = prepare(m);
    let v = vec3(f32::MAX, f32::MAX, 1.0);
    assert!(!m.transform_vector3(v).is_finite());
    assert_eq!(p.transform_vector(v), Ok(vec3(0.0, f32::MAX, 1.0)));
    assert_eq!(p.transform_point(v), Ok(vec3(0.0, f32::MAX, 1.0)));
    assert_eq!(
        p.transform_point(Vec3::X * f32::MAX),
        Err(AffineError::ResultOutOfRange)
    );
    assert_eq!(
        p.transform_vector(Vec3::X * f32::MAX),
        Err(AffineError::ResultOutOfRange)
    );
    let q = prepare(Mat4::from_translation(Vec3::X * f32::MAX));
    assert_eq!(
        q.transform_point(Vec3::X * f32::MAX),
        Err(AffineError::ResultOutOfRange)
    );
    let r = prepare(Mat4::from_scale(Vec3::splat(0.5)));
    assert_eq!(
        r.transform_vector(Vec3::splat(f32::from_bits(1))),
        Ok(Vec3::ZERO)
    );
}

#[test]
fn nonfinite_scalar_inputs_and_zero_normals_fail_without_fallback() {
    let p = prepare(Mat4::IDENTITY);
    for component in 0..3 {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut v = Vec3::ONE;
            v[component] = invalid;
            assert_eq!(
                p.transform_point(v),
                Err(AffineError::NonFiniteInput { input: "point" })
            );
            assert_eq!(
                p.transform_vector(v),
                Err(AffineError::NonFiniteInput { input: "vector" })
            );
            assert_eq!(
                p.transform_normal(v),
                Err(AffineError::NonFiniteInput { input: "normal" })
            );
        }
    }
    assert_eq!(
        p.transform_normal(Vec3::ZERO),
        Err(AffineError::DegenerateNormal)
    );
    assert_eq!(
        p.transform_normal(Vec3::splat(-0.0)),
        Err(AffineError::DegenerateNormal)
    );
}

type BatchOperation = fn(&PreparedAffine3D, &[Vec3], &mut [Vec3]) -> Result<(), AffineSliceError>;
type ScalarOperation = fn(&PreparedAffine3D, Vec3) -> Result<Vec3, AffineError>;

fn operations() -> [(BatchOperation, ScalarOperation); 3] {
    [
        (
            PreparedAffine3D::transform_points_into,
            PreparedAffine3D::transform_point,
        ),
        (
            PreparedAffine3D::transform_vectors_into,
            PreparedAffine3D::transform_vector,
        ),
        (
            PreparedAffine3D::transform_normals_into,
            PreparedAffine3D::transform_normal,
        ),
    ]
}

#[test]
fn batches_match_scalar_outputs_and_preserve_unused_tail() {
    let p = prepare(sheared(true));
    let input = [Vec3::X, Vec3::Y, vec3(2.0, -4.0, 1.0)];
    for (batch, scalar) in operations() {
        let mut output = [Vec3::splat(-99.0); 5];
        batch(&p, &input, &mut output).unwrap();
        for (actual, value) in output.iter().zip(input) {
            assert_eq!(*actual, scalar(&p, value).unwrap());
        }
        assert_eq!(&output[3..], &[Vec3::splat(-99.0); 2]);
        batch(&p, &[], &mut output).unwrap();
        batch(&p, &[], &mut []).unwrap();
        assert_eq!(&output[3..], &[Vec3::splat(-99.0); 2]);
    }
}

fn bits(values: &[Vec3]) -> Vec<[u32; 3]> {
    values
        .iter()
        .map(|v| v.to_array().map(f32::to_bits))
        .collect()
}

#[test]
fn all_batches_preflight_capacity_and_first_error_without_any_write() {
    let p = prepare(Mat4::from_scale(Vec3::splat(2.0)));
    for (batch, _) in operations() {
        // Sentinel NaN payload and signed zero must retain their exact bits.
        let sentinel = vec3(f32::from_bits(0x7fc01234), -0.0, f32::INFINITY);
        let mut output = [sentinel; 4];
        let before = bits(&output);
        assert_eq!(
            batch(&p, &[Vec3::NAN, Vec3::X], &mut output[..1]),
            Err(AffineSliceError::InsufficientCapacity {
                required: 2,
                available: 1
            })
        );
        assert_eq!(bits(&output), before);
        assert_eq!(
            batch(&p, &[Vec3::X], &mut []),
            Err(AffineSliceError::InsufficientCapacity {
                required: 1,
                available: 0
            })
        );
        let error = batch(
            &p,
            &[Vec3::X, Vec3::Y, Vec3::NAN, Vec3::INFINITY],
            &mut output,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AffineSliceError::Element {
                index: 2,
                source: AffineError::NonFiniteInput { .. }
            }
        ));
        assert_eq!(bits(&output), before);
    }
    for batch in [
        PreparedAffine3D::transform_points_into as BatchOperation,
        PreparedAffine3D::transform_vectors_into,
    ] {
        let mut output = [Vec3::splat(-9.0); 3];
        let before = output;
        assert_eq!(
            batch(&p, &[Vec3::X, Vec3::splat(f32::MAX)], &mut output),
            Err(AffineSliceError::Element {
                index: 1,
                source: AffineError::ResultOutOfRange
            })
        );
        assert_eq!(output, before);
    }
    let mut output = [Vec3::splat(-9.0); 3];
    let before = output;
    assert_eq!(
        p.transform_normals_into(&[Vec3::Z, Vec3::ZERO], &mut output),
        Err(AffineSliceError::Element {
            index: 1,
            source: AffineError::DegenerateNormal
        })
    );
    assert_eq!(output, before);
    p.transform_normals_into(&[Vec3::Z, Vec3::Y], &mut output)
        .unwrap();
    assert_eq!(output, [Vec3::Z, Vec3::Y, Vec3::splat(-9.0)]);
}

#[test]
fn update_failures_retain_complete_cache_then_reset_and_recover() {
    let mut p = prepare(sheared(true));
    let before = p;
    for invalid in [
        Mat4::NAN,
        Mat4::ZERO,
        Mat4::from_scale(vec3(0.0, 1.0, 1.0)),
        Mat4::from_scale(vec3(f32::from_bits(1), 1.0, 1.0)),
    ] {
        assert!(p.try_update(invalid).is_err());
        assert_eq!(p, before);
        assert_eq!(
            p.transform_point(Vec3::ONE),
            before.transform_point(Vec3::ONE)
        );
        assert_eq!(
            p.transform_normal(Vec3::ONE),
            before.transform_normal(Vec3::ONE)
        );
    }
    p.try_update(Mat4::IDENTITY).unwrap();
    assert_eq!(p, prepare(Mat4::IDENTITY));
    assert_eq!(p.transform_point(Vec3::ONE), Ok(Vec3::ONE));
    assert_eq!(p.normal_matrix(), Mat4::IDENTITY);
    assert!(!p.is_reflected());
    p.try_update(sheared(false)).unwrap();
    assert_eq!(p, prepare(sheared(false)));
}

#[test]
fn errors_display_and_chain_existing_matrix_diagnostics() {
    use std::error::Error;
    let error = PreparedAffine3D::try_new(Mat4::from_scale(Vec3::ZERO)).unwrap_err();
    assert!(error.to_string().contains("linear inverse"));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<MatrixError>()
            .is_some()
    );
    let wrapped = AffineSliceError::Element {
        index: 7,
        source: error,
    };
    assert!(wrapped.to_string().contains("input[7]"));
    assert_eq!(
        wrapped.source().unwrap().downcast_ref::<AffineError>(),
        Some(&error)
    );
    for error in [
        AffineError::NonAffine,
        AffineError::DegenerateNormal,
        AffineError::ResultOutOfRange,
        AffineError::NonFiniteInput { input: "point" },
    ] {
        assert!(!error.to_string().is_empty());
        assert!(error.source().is_none());
    }
    let capacity = AffineSliceError::InsufficientCapacity {
        required: 2,
        available: 1,
    };
    assert!(capacity.to_string().contains("2 slots"));
    assert!(capacity.source().is_none());
}

struct Allocator;
thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    let _ = RECORD.try_with(|record| {
        if record.get() {
            let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
        }
    });
}

// SAFETY: All allocation contracts are forwarded unchanged to System; TLS only
// counts this test thread and tolerates teardown. Production math forbids unsafe.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn allocations(action: impl FnOnce()) -> usize {
    struct StopRecording;
    impl Drop for StopRecording {
        fn drop(&mut self) {
            RECORD.set(false);
        }
    }
    CALLS.set(0);
    RECORD.set(true);
    let recording = StopRecording;
    action();
    drop(recording);
    CALLS.get()
}

#[test]
fn counter_control_observes_heap_allocation() {
    assert!(
        allocations(|| {
            black_box(Box::new(42));
        }) > 0
    );
}

#[test]
fn prepare_batch_queries_and_recovery_use_no_heap_storage() {
    let model = sheared(true);
    assert_eq!(
        allocations(|| {
            let mut p = prepare(model);
            let input = [Vec3::X, Vec3::Y, Vec3::Z];
            let mut output = [Vec3::ZERO; 3];
            for _ in 0..128 {
                black_box(p.transform_point(black_box(Vec3::ONE))).unwrap();
                black_box(p.transform_vector(black_box(Vec3::ONE))).unwrap();
                black_box(p.transform_normal(black_box(Vec3::ONE))).unwrap();
                p.transform_points_into(&input, &mut output).unwrap();
                p.transform_vectors_into(&input, &mut output).unwrap();
                p.transform_normals_into(&input, &mut output).unwrap();
                assert!(
                    p.transform_normals_into(&[Vec3::Z, Vec3::ZERO], &mut output)
                        .is_err()
                );
                assert!(p.transform_points_into(&input, &mut output[..2]).is_err());
                assert!(p.try_update(Mat4::from_scale(Vec3::ZERO)).is_err());
                p.try_update(Mat4::IDENTITY).unwrap();
                p.try_update(model).unwrap();
                black_box(output);
            }
        }),
        0
    );
}
