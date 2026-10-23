use ferravex_math::curves::{
    CubicBezier3, CurveError, MAX_TESSELLATION_DEPTH, Tessellation, TessellationOptions,
    TessellationStatus,
};
use ferravex_math::{Vec3, vec3};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

fn options(tolerance: f64) -> TessellationOptions {
    TessellationOptions {
        tolerance,
        max_depth: 16,
        max_work: 8191,
    }
}

fn arch() -> CubicBezier3 {
    CubicBezier3::new([
        Vec3::ZERO,
        vec3(0., 4., 0.),
        vec3(4., 4., 0.),
        vec3(4., 0., 0.),
    ])
    .unwrap()
}

// Independent Bernstein evaluation in f64: dense checks do not round the curve
// back to Vec3, which could hide the very endpoint quantization being tested.
fn polynomial(curve: CubicBezier3, t: f64) -> [f64; 3] {
    let p = curve.control_points().map(|p| p.to_array().map(f64::from));
    let s = 1. - t;
    std::array::from_fn(|i| {
        s.powi(3) * p[0][i]
            + 3. * s * s * t * p[1][i]
            + 3. * s * t * t * p[2][i]
            + t.powi(3) * p[3][i]
    })
}

fn distance_to_polyline(point: [f64; 3], points: &[Vec3]) -> f64 {
    points
        .windows(2)
        .map(|pair| {
            let a = pair[0].to_array().map(f64::from);
            let b = pair[1].to_array().map(f64::from);
            let delta: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
            let length_squared = delta.iter().map(|x| x * x).sum::<f64>();
            let t = if length_squared == 0. {
                0.
            } else {
                ((0..3).map(|i| (point[i] - a[i]) * delta[i]).sum::<f64>() / length_squared)
                    .clamp(0., 1.)
            };
            let difference: [f64; 3] = std::array::from_fn(|i| point[i] - (a[i] + t * delta[i]));
            difference[0].hypot(difference[1]).hypot(difference[2])
        })
        .fold(f64::INFINITY, f64::min)
}

fn check_bounds(result: Tessellation, opts: TessellationOptions, capacity: usize) {
    assert!(result.written <= capacity);
    assert!(result.work <= opts.max_work);
    assert!(result.subdivisions <= result.work);
    assert!(result.depth_reached <= opts.max_depth);
    assert!((0. ..=1.).contains(&result.covered_until));
    assert!(result.max_error_bound.is_finite());
    assert!((0. ..=opts.tolerance).contains(&result.max_error_bound));
    if result.written < 2 {
        assert_eq!(result.covered_until, 0.);
        assert_eq!(result.max_error_bound, 0.);
    }
    if result.status == TessellationStatus::Complete {
        assert!(result.written >= 2);
        assert_eq!(result.covered_until, 1.);
        assert_eq!(result.work, 2 * result.written - 3);
        assert_eq!(result.subdivisions, result.written - 2);
    } else {
        assert!(result.covered_until < 1.);
    }
}

#[test]
fn endpoints_position_and_raw_tangents_follow_the_authored_cubic() {
    let curve = arch();
    assert_eq!(curve.position(0.).unwrap(), Vec3::ZERO);
    assert_eq!(curve.position(1.).unwrap(), vec3(4., 0., 0.));
    assert_eq!(curve.position(0.5).unwrap(), vec3(2., 3., 0.));
    assert_eq!(curve.tangent(0.).unwrap(), [0., 12., 0.]);
    assert_eq!(curve.tangent(0.5).unwrap(), [6., 0., 0.]);
    assert_eq!(curve.tangent(1.).unwrap(), [0., -12., 0.]);
    for step in 0..=64 {
        let t = f64::from(step) / 64.;
        assert_eq!(
            curve.position(t).unwrap(),
            Vec3::from_array(polynomial(curve, t).map(|x| x as f32))
        );
    }
}

#[test]
fn all_nonfinite_controls_and_invalid_parameters_are_rejected() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for slot in 0..4 {
            for axis in 0..3 {
                let mut controls = [Vec3::ZERO; 4];
                controls[slot][axis] = bad;
                assert_eq!(
                    CubicBezier3::new(controls),
                    Err(CurveError::NonFiniteControl)
                );
            }
        }
    }
    for t in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.,
        -f64::MIN_POSITIVE,
        1_f64.next_up(),
        2.,
    ] {
        assert_eq!(arch().position(t), Err(CurveError::InvalidParameter));
        assert_eq!(arch().tangent(t), Err(CurveError::InvalidParameter));
    }
    assert!(arch().position(-0.).is_ok());
    assert!(arch().tangent(-0.).is_ok());
}

#[test]
fn signed_zero_endpoints_survive_sampling_and_tessellation() {
    let start = vec3(-0., 0., -0.);
    let end = vec3(0., -0., 0.);
    let curve = CubicBezier3::new([start, start, end, end]).unwrap();
    let bits = |p: Vec3| p.to_array().map(f32::to_bits);
    assert_eq!(bits(curve.position(-0.).unwrap()), bits(start));
    assert_eq!(bits(curve.position(1.).unwrap()), bits(end));
    let mut output = [Vec3::ONE; 3];
    let result = curve
        .tessellate_into(&mut output, options(f64::from_bits(1)))
        .unwrap();
    assert_eq!(result.status, TessellationStatus::Complete);
    assert_eq!(result.written, 2);
    assert_eq!(result.max_error_bound, 0.);
    assert_eq!(bits(output[0]), bits(start));
    assert_eq!(bits(output[1]), bits(end));
    assert_eq!(output[2], Vec3::ONE);
}

#[test]
fn exact_constants_including_extremes_have_zero_error_and_two_endpoints() {
    for point in [
        Vec3::ZERO,
        Vec3::splat(f32::from_bits(1)),
        Vec3::splat(f32::MAX),
        Vec3::splat(-f32::MAX),
    ] {
        let curve = CubicBezier3::new([point; 4]).unwrap();
        for t in [0., 0.1, 0.5, 0.9, 1.] {
            assert_eq!(curve.position(t).unwrap(), point);
            assert_eq!(curve.tangent(t).unwrap(), [0.; 3]);
        }
        let mut output = [Vec3::ZERO; 2];
        let opts = TessellationOptions {
            max_depth: 0,
            max_work: 1,
            ..options(f64::from_bits(1))
        };
        let result = curve.tessellate_into(&mut output, opts).unwrap();
        assert_eq!(result.status, TessellationStatus::Complete);
        assert_eq!(result.max_error_bound, 0.);
        assert_eq!(output, [point; 2]);
        check_bounds(result, opts, output.len());
    }
}

#[test]
fn straight_curve_accepts_one_chord_but_backtracking_is_not_flattened_away() {
    let straight = CubicBezier3::new([Vec3::ZERO, Vec3::X, Vec3::X * 2., Vec3::X * 3.]).unwrap();
    let mut output = [Vec3::ZERO; 1024];
    let opts = TessellationOptions {
        max_work: 1,
        max_depth: 0,
        ..options(1e-12)
    };
    let result = straight.tessellate_into(&mut output, opts).unwrap();
    assert_eq!(result.status, TessellationStatus::Complete);
    assert_eq!(result.written, 2);
    assert_eq!(&output[..2], &[Vec3::ZERO, Vec3::X * 3.]);
    assert_eq!(straight.tangent(0.5).unwrap(), [3., 0., 0.]);

    let backtrack =
        CubicBezier3::new([Vec3::ZERO, Vec3::X * 4., -Vec3::X * 4., Vec3::ZERO]).unwrap();
    assert_eq!(backtrack.position(0.5).unwrap(), Vec3::ZERO);
    let opts = options(0.001);
    let result = backtrack.tessellate_into(&mut output, opts).unwrap();
    assert_eq!(result.status, TessellationStatus::Complete);
    assert!(output[..result.written].iter().any(|p| p.x > 1.));
    assert!(output[..result.written].iter().any(|p| p.x < -1.));
    for step in 0..=512 {
        assert!(
            distance_to_polyline(
                polynomial(backtrack, f64::from(step) / 512.),
                &output[..result.written]
            ) <= opts.tolerance
        );
    }
}

#[test]
fn stationary_cusp_and_zero_endpoint_derivatives_have_no_fallback_direction() {
    let cusp = CubicBezier3::new([Vec3::ZERO, Vec3::X, Vec3::X, Vec3::ZERO]).unwrap();
    assert_eq!(cusp.position(0.5).unwrap(), Vec3::X * 0.75);
    assert_eq!(cusp.tangent(0.5).unwrap(), [0.; 3]);
    assert!(cusp.tangent(0.5 - 1e-9).unwrap()[0] > 0.);
    assert!(cusp.tangent(0.5 + 1e-9).unwrap()[0] < 0.);
    let mut output = [Vec3::ZERO; 256];
    let result = cusp.tessellate_into(&mut output, options(0.001)).unwrap();
    assert_eq!(result.status, TessellationStatus::Complete);
    assert!(output[..result.written].contains(&(Vec3::X * 0.75)));
    assert_eq!(output[0], output[result.written - 1]);

    let ease = CubicBezier3::new([Vec3::ZERO, Vec3::ZERO, Vec3::X, Vec3::X]).unwrap();
    assert_eq!(ease.tangent(0.).unwrap(), [0.; 3]);
    assert_eq!(ease.tangent(1.).unwrap(), [0.; 3]);
    assert_eq!(ease.tangent(0.5).unwrap(), [1.5, 0., 0.]);
}

#[test]
fn an_inflection_with_midpoint_on_chord_still_requires_subdivision() {
    let curve = CubicBezier3::new([
        Vec3::ZERO,
        vec3(1., 4., 0.),
        vec3(2., -4., 0.),
        vec3(3., 0., 0.),
    ])
    .unwrap();
    assert_eq!(curve.position(0.5).unwrap(), vec3(1.5, 0., 0.));
    let mut output = [Vec3::ZERO; 512];
    let opts = options(0.005);
    let result = curve.tessellate_into(&mut output, opts).unwrap();
    assert_eq!(result.status, TessellationStatus::Complete);
    assert!(result.subdivisions > 1);
    for step in 0..=512 {
        assert!(
            distance_to_polyline(
                polynomial(curve, f64::from(step) / 512.),
                &output[..result.written]
            ) <= result.max_error_bound
        );
    }
}

#[test]
fn empty_one_and_exact_fit_buffers_have_explicit_capacity_outcomes() {
    let curve = arch();
    let mut output = [Vec3::ONE; 128];
    let opts = options(0.01);
    let empty = curve.tessellate_into(&mut output[..0], opts).unwrap();
    assert_eq!(empty.status, TessellationStatus::OutputFull);
    assert_eq!((empty.written, empty.work), (0, 0));
    assert!(output.iter().all(|p| *p == Vec3::ONE));
    let one = curve.tessellate_into(&mut output[..1], opts).unwrap();
    assert_eq!(one.status, TessellationStatus::OutputFull);
    assert_eq!((one.written, one.work), (1, 0));
    assert_eq!(output[0], Vec3::ZERO);
    assert!(output[1..].iter().all(|p| *p == Vec3::ONE));
    let full = curve.tessellate_into(&mut output, opts).unwrap();
    assert_eq!(full.status, TessellationStatus::Complete);
    let mut exact = [Vec3::ONE; 128];
    let again = curve
        .tessellate_into(&mut exact[..full.written], opts)
        .unwrap();
    assert_eq!(again, full);
    assert_eq!(&exact[..full.written], &output[..full.written]);
    assert!(exact[full.written..].iter().all(|p| *p == Vec3::ONE));
}

#[test]
fn invalid_options_leave_all_storage_unchanged_even_with_no_capacity() {
    let mut output = [Vec3::ONE; 8];
    for tolerance in [0., -0., -1., f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        for capacity in [0, 8] {
            assert_eq!(
                arch().tessellate_into(&mut output[..capacity], options(tolerance)),
                Err(CurveError::InvalidTolerance)
            );
            assert_eq!(output, [Vec3::ONE; 8]);
        }
    }
    for depth in [MAX_TESSELLATION_DEPTH + 1, u8::MAX] {
        assert_eq!(
            arch().tessellate_into(
                &mut output,
                TessellationOptions {
                    max_depth: depth,
                    ..options(0.01)
                }
            ),
            Err(CurveError::InvalidDepth)
        );
        assert_eq!(output, [Vec3::ONE; 8]);
    }
}

#[test]
fn work_and_depth_limits_never_emit_an_uncertified_endpoint() {
    let curve = arch();
    let mut output = [Vec3::ONE; 128];
    let no_work = curve
        .tessellate_into(
            &mut output,
            TessellationOptions {
                max_work: 0,
                ..options(0.01)
            },
        )
        .unwrap();
    assert_eq!(no_work.status, TessellationStatus::WorkLimit);
    assert_eq!((no_work.written, no_work.work), (1, 0));
    assert!(output[1..].iter().all(|p| *p == Vec3::ONE));
    let one_work = curve
        .tessellate_into(
            &mut output,
            TessellationOptions {
                max_work: 1,
                ..options(0.01)
            },
        )
        .unwrap();
    assert_eq!(one_work.status, TessellationStatus::WorkLimit);
    assert_eq!(
        (one_work.written, one_work.work, one_work.subdivisions),
        (1, 1, 1)
    );
    assert_eq!(one_work.covered_until, 0.);
    let no_split = curve
        .tessellate_into(
            &mut output,
            TessellationOptions {
                max_depth: 0,
                ..options(0.01)
            },
        )
        .unwrap();
    assert_eq!(no_split.status, TessellationStatus::SubdivisionLimit);
    assert_eq!(
        (no_split.written, no_split.work, no_split.subdivisions),
        (1, 1, 0)
    );
    assert!(output[1..].iter().all(|p| *p == Vec3::ONE));
}

#[test]
fn budget_sweep_respects_strict_bounds_and_preserves_the_same_ordered_prefix() {
    let curve = arch();
    let mut reference = [Vec3::ZERO; 128];
    let full = curve
        .tessellate_into(&mut reference, options(0.01))
        .unwrap();
    assert_eq!(full.status, TessellationStatus::Complete);
    for capacity in 0..=full.written + 1 {
        for max_work in 0..=full.work + 1 {
            for max_depth in [0, 1, 3, 16, MAX_TESSELLATION_DEPTH] {
                let opts = TessellationOptions {
                    max_depth,
                    max_work,
                    ..options(0.01)
                };
                let mut output = [Vec3::ONE; 128];
                let result = curve
                    .tessellate_into(&mut output[..capacity], opts)
                    .unwrap();
                check_bounds(result, opts, capacity);
                assert_eq!(&output[..result.written], &reference[..result.written]);
                assert!(output[result.written..].iter().all(|p| *p == Vec3::ONE));
                if result.written > 1 {
                    let endpoint = polynomial(curve, result.covered_until);
                    let actual = output[result.written - 1].to_array().map(f64::from);
                    assert!((0..3).all(|i| (actual[i] - endpoint[i]).abs() <= opts.tolerance));
                }
            }
        }
    }
}

#[test]
fn partial_prefix_covers_its_reported_domain_and_a_fresh_call_replaces_it() {
    let curve = arch();
    let mut output = [Vec3::ONE; 256];
    let partial = curve
        .tessellate_into(&mut output[..8], options(0.01))
        .unwrap();
    assert_eq!(partial.status, TessellationStatus::OutputFull);
    assert!(partial.covered_until > 0. && partial.covered_until < 1.);
    for step in 0..=256 {
        let t = partial.covered_until * f64::from(step) / 256.;
        assert!(
            distance_to_polyline(polynomial(curve, t), &output[..partial.written])
                <= partial.max_error_bound
        );
    }
    let complete = curve.tessellate_into(&mut output, options(0.01)).unwrap();
    assert_eq!(complete.status, TessellationStatus::Complete);
    let shorter = CubicBezier3::new([Vec3::Z; 4]).unwrap();
    let replaced = shorter.tessellate_into(&mut output, options(0.01)).unwrap();
    assert_eq!(replaced.written, 2);
    assert_eq!(&output[..2], &[Vec3::Z; 2]);
    // Only the reported prefix belongs to the new run, so stale tails are not
    // accidentally fed to Route3 by this API's consumers.
    assert_eq!(output[complete.written - 1], curve.position(1.).unwrap());
    assert!(
        curve
            .tessellate_into(&mut output, options(f64::NAN))
            .is_err()
    );
    let recovered = curve.tessellate_into(&mut output, options(0.01)).unwrap();
    assert_eq!(recovered, complete);
}

#[test]
fn extreme_and_subnormal_controls_remain_finite_with_bounded_work() {
    let max = f32::MAX;
    let curve = CubicBezier3::new([
        vec3(-max, max, 0.),
        vec3(max, max, max),
        vec3(-max, -max, -max),
        vec3(max, -max, 0.),
    ])
    .unwrap();
    for step in 0..=64 {
        let t = f64::from(step) / 64.;
        assert!(curve.position(t).unwrap().is_finite());
        assert!(curve.tangent(t).unwrap().iter().all(|x| x.is_finite()));
    }
    assert!(curve.tangent(0.).unwrap()[0] > f64::from(max));
    let mut output = [Vec3::ZERO; 1024];
    let opts = options(f64::from(max) * 0.01);
    let result = curve.tessellate_into(&mut output, opts).unwrap();
    check_bounds(result, opts, output.len());
    assert_eq!(result.status, TessellationStatus::Complete);
    assert!(output[..result.written].iter().all(|p| p.is_finite()));
    for step in 0..=256 {
        assert!(
            distance_to_polyline(
                polynomial(curve, f64::from(step) / 256.),
                &output[..result.written]
            ) <= result.max_error_bound
        );
    }

    let tiny = f32::from_bits(1);
    let curve =
        CubicBezier3::new([Vec3::ZERO, Vec3::X * tiny, Vec3::Y * tiny, Vec3::Z * tiny]).unwrap();
    assert_eq!(curve.tangent(0.).unwrap(), [3. * f64::from(tiny), 0., 0.]);
    let opts = options(f64::from(tiny) * 4.);
    let result = curve.tessellate_into(&mut output, opts).unwrap();
    assert_eq!(result.status, TessellationStatus::Complete);
    check_bounds(result, opts, output.len());
}

#[test]
fn quantization_and_rounding_limits_cannot_silently_certify_a_tiny_tolerance() {
    let x = 1_f32.next_up();
    let curve = CubicBezier3::new([
        vec3(1., 0., 0.),
        vec3(1., 1., 0.),
        vec3(x, 1., 0.),
        vec3(x, 0., 0.),
    ])
    .unwrap();
    let mut output = [Vec3::ONE; 4096];
    let opts = TessellationOptions {
        max_depth: MAX_TESSELLATION_DEPTH,
        ..options(1e-12)
    };
    let result = curve.tessellate_into(&mut output, opts).unwrap();
    assert_eq!(result.status, TessellationStatus::SubdivisionLimit);
    assert_eq!(result.depth_reached, MAX_TESSELLATION_DEPTH);
    assert!(result.covered_until < 0.5);
    check_bounds(result, opts, output.len());
    // The exact midpoint is halfway between two f32 X coordinates: the rounded
    // sample itself cannot meet the requested parameter-paired error.
    let exact_midpoint = polynomial(curve, 0.5);
    let rounded_midpoint = curve.position(0.5).unwrap();
    assert!((exact_midpoint[0] - f64::from(rounded_midpoint.x)).abs() > opts.tolerance);
}

#[test]
fn seeded_3d_curves_meet_their_reported_bounds_on_dense_independent_samples() {
    let mut seed = 219_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) as i32 % 1024) as f32 / 32.
    };
    for _ in 0..64 {
        let curve =
            CubicBezier3::new(std::array::from_fn(|_| vec3(next(), next(), next()))).unwrap();
        let opts = options(0.025);
        let mut output = [Vec3::ZERO; 1024];
        let result = curve.tessellate_into(&mut output, opts).unwrap();
        check_bounds(result, opts, output.len());
        assert_eq!(result.status, TessellationStatus::Complete);
        assert_eq!(output[0], curve.control_points()[0]);
        assert_eq!(output[result.written - 1], curve.control_points()[3]);
        for step in 0..=128 {
            assert!(
                distance_to_polyline(
                    polynomial(curve, f64::from(step) / 128.),
                    &output[..result.written]
                ) <= result.max_error_bound
            );
        }
    }
}

#[test]
fn maximum_integer_budgets_are_valid_without_counter_overflow() {
    let mut output = [Vec3::ZERO; 2];
    let opts = TessellationOptions {
        tolerance: f64::MAX,
        max_depth: MAX_TESSELLATION_DEPTH,
        max_work: usize::MAX,
    };
    let result = arch().tessellate_into(&mut output, opts).unwrap();
    assert_eq!(result.status, TessellationStatus::Complete);
    assert_eq!((result.work, result.subdivisions), (1, 0));
    check_bounds(result, opts, output.len());
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

// SAFETY: All allocation contracts are forwarded unchanged to System. TLS
// counts this test thread only and tolerates teardown. Production forbids unsafe.
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
fn allocation_counter_observes_a_real_allocation() {
    assert!(
        allocations(|| {
            black_box(Box::new(219));
        }) > 0
    );
}

#[test]
fn construction_sampling_all_saturation_errors_and_recovery_allocate_nothing() {
    let calls = allocations(|| {
        let curve = black_box(arch());
        let mut output = [Vec3::ZERO; 256];
        for _ in 0..32 {
            let _ = black_box(CubicBezier3::new([Vec3::NAN; 4]));
            let _ = black_box(curve.control_points());
            let _ = black_box(curve.position(0.25));
            let _ = black_box(curve.tangent(0.5));
            let _ = black_box(curve.position(f64::NAN));
            let _ = black_box(curve.tangent(-1.));
            let _ = black_box(curve.tessellate_into(&mut output, options(0.01)));
            let _ = black_box(curve.tessellate_into(&mut output[..0], options(0.01)));
            let _ = black_box(curve.tessellate_into(&mut output[..8], options(0.01)));
            let _ = black_box(curve.tessellate_into(
                &mut output,
                TessellationOptions {
                    max_work: 4,
                    ..options(0.01)
                },
            ));
            let _ = black_box(curve.tessellate_into(
                &mut output,
                TessellationOptions {
                    max_depth: 0,
                    ..options(0.01)
                },
            ));
            let _ = black_box(curve.tessellate_into(&mut output, options(f64::NAN)));
            let _ = black_box(curve.tessellate_into(
                &mut output,
                TessellationOptions {
                    max_depth: u8::MAX,
                    ..options(0.01)
                },
            ));
            let _ = black_box(
                CubicBezier3::new([Vec3::ZERO; 4])
                    .unwrap()
                    .tessellate_into(&mut output, options(0.01)),
            );
            let _ = black_box(curve.tessellate_into(&mut output, options(0.01)));
        }
    });
    assert_eq!(calls, 0);
}
