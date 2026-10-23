use ferravex_math::segments::{
    ClosestPoints, SegmentError, closest_point_segment2, closest_point_segment3, closest_segments2,
    closest_segments3,
};
use ferravex_math::{Vec2, Vec3, vec2, vec3};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} (tolerance {tolerance})"
    );
}

fn consistent<const N: usize>(
    hit: ClosestPoints<N>,
    a: [f32; N],
    b: [f32; N],
    c: [f32; N],
    d: [f32; N],
) {
    assert!((0. ..=1.).contains(&hit.first_parameter));
    assert!((0. ..=1.).contains(&hit.second_parameter));
    assert!(hit.first_parameter.is_sign_positive());
    assert!(hit.second_parameter.is_sign_positive());
    for i in 0..N {
        assert!(hit.first[i].is_finite() && hit.second[i].is_finite());
        let expected_first =
            f64::from(a[i]) * (1. - hit.first_parameter) + f64::from(b[i]) * hit.first_parameter;
        let expected_second =
            f64::from(c[i]) * (1. - hit.second_parameter) + f64::from(d[i]) * hit.second_parameter;
        assert_eq!(hit.first[i], expected_first);
        assert_eq!(hit.second[i], expected_second);
    }
    let squared: f64 = (0..N).map(|i| (hit.first[i] - hit.second[i]).powi(2)).sum();
    near(hit.distance, squared.sqrt(), squared.sqrt() * 1e-15);
    assert!(hit.distance.is_finite() && hit.distance >= 0.);
}

#[test]
fn point_projections_clamp_both_ends_and_keep_interior_witnesses() {
    let a = vec2(1., 2.);
    let b = vec2(5., 2.);
    for (point, t, witness, distance) in [
        (vec2(-2., 6.), 0., [1., 2.], 5.),
        (vec2(3., 5.), 0.5, [3., 2.], 3.),
        (vec2(8., 6.), 1., [5., 2.], 5.),
    ] {
        let hit = closest_point_segment2(point, a, b).unwrap();
        assert_eq!(hit.first, point.to_array().map(f64::from));
        assert_eq!(hit.first_parameter, 0.);
        assert_eq!(hit.second_parameter, t);
        assert_eq!(hit.second, witness);
        assert_eq!(hit.distance, distance);
        consistent(
            hit,
            point.to_array(),
            point.to_array(),
            a.to_array(),
            b.to_array(),
        );
    }
    let hit = closest_point_segment3(vec3(2., 4., 3.), Vec3::ZERO, vec3(4., 0., 6.)).unwrap();
    assert_eq!(hit.second, [2., 0., 3.]);
    assert_eq!(hit.second_parameter, 0.5);
    assert_eq!(hit.distance, 4.);
}

#[test]
fn crossing_and_touching_segments_return_actual_shared_witnesses() {
    for (c, d, expected, s, t) in [
        (vec2(1., -2.), vec2(1., 6.), [1., 0.], 0.25, 0.25),
        (vec2(4., 0.), vec2(4., 6.), [4., 0.], 1., 0.),
    ] {
        let hit = closest_segments2(Vec2::ZERO, vec2(4., 0.), c, d).unwrap();
        assert_eq!(hit.first, expected);
        assert_eq!(hit.second, expected);
        assert_eq!((hit.first_parameter, hit.second_parameter), (s, t));
        assert_eq!(hit.distance, 0.);
        consistent(hit, [0., 0.], [4., 0.], c.to_array(), d.to_array());
    }
}

#[test]
fn skew_oblique_segments_have_interior_orthogonal_witnesses() {
    let a = vec3(-2., -2., 0.);
    let b = vec3(2., 2., 0.);
    let c = vec3(2., -2., 3.);
    let d = vec3(-2., 2., 3.);
    let hit = closest_segments3(a, b, c, d).unwrap();
    assert_eq!(hit.first, [0., 0., 0.]);
    assert_eq!(hit.second, [0., 0., 3.]);
    assert_eq!((hit.first_parameter, hit.second_parameter), (0.5, 0.5));
    assert_eq!(hit.distance, 3.);
    consistent(hit, a.to_array(), b.to_array(), c.to_array(), d.to_array());
    let swapped = closest_segments3(c, d, a, b).unwrap();
    assert_eq!(swapped.first, hit.second);
    assert_eq!(swapped.second, hit.first);
    assert_eq!(swapped.distance, hit.distance);
}

#[test]
fn constrained_closest_pair_recomputes_projection_at_boundaries() {
    // Independently clamping the infinite-line parameters gives the wrong
    // second witness: after first clamps to its end, second must project again.
    let hit = closest_segments2(Vec2::ZERO, Vec2::X, vec2(2., 1.), vec2(3., -1.)).unwrap();
    assert_eq!(hit.first_parameter, 1.);
    assert_eq!(hit.first, [1., 0.]);
    near(hit.second_parameter, 0.2, 1e-15);
    near(hit.second[0], 2.2, 1e-15);
    near(hit.second[1], 0.6, 1e-15);
    near(hit.distance, 1.8_f64.sqrt(), 1e-15);
}

#[test]
fn parallel_ties_are_lexicographic_including_reversed_overlap() {
    for (c, d, expected_s, expected_t, first, second) in [
        (vec2(2., 3.), vec2(6., 3.), 0.5, 0., [2., 0.], [2., 3.]),
        (vec2(6., 3.), vec2(2., 3.), 0.5, 1., [2., 0.], [2., 3.]),
        (vec2(-2., 3.), vec2(6., 3.), 0., 0.25, [0., 0.], [0., 3.]),
        (vec2(2., 0.), vec2(6., 0.), 0.5, 0., [2., 0.], [2., 0.]),
    ] {
        let hit = closest_segments2(Vec2::ZERO, Vec2::X * 4., c, d).unwrap();
        assert_eq!(hit.first_parameter, expected_s);
        assert_eq!(hit.second_parameter, expected_t);
        assert_eq!(hit.first, first);
        assert_eq!(hit.second, second);
        consistent(hit, [0., 0.], [4., 0.], c.to_array(), d.to_array());
    }
    let separated = closest_segments3(Vec3::ZERO, Vec3::X, Vec3::X * 3., Vec3::X * 4.).unwrap();
    assert_eq!(separated.first, [1., 0., 0.]);
    assert_eq!(separated.second, [3., 0., 0.]);
    assert_eq!(separated.distance, 2.);
}

#[test]
fn identical_and_reversed_identical_segments_select_first_start() {
    let a = vec3(1., -2., 3.);
    let b = vec3(5., 6., 7.);
    for (c, d, t) in [(a, b, 0.), (b, a, 1.)] {
        let hit = closest_segments3(a, b, c, d).unwrap();
        assert_eq!(hit.first, a.to_array().map(f64::from));
        assert_eq!(hit.first, hit.second);
        assert_eq!((hit.first_parameter, hit.second_parameter), (0., t));
        assert_eq!(hit.distance, 0.);
    }
}

#[test]
fn point_segments_are_valid_and_do_not_need_normalization() {
    let p = vec3(2., 3., 0.);
    let point = closest_point_segment3(p, Vec3::ZERO, Vec3::ZERO).unwrap();
    assert_eq!(point.second_parameter, 0.);
    assert_eq!(point.distance, 13_f64.sqrt());
    let first = closest_segments3(p, p, Vec3::ZERO, Vec3::X * 4.).unwrap();
    assert_eq!((first.first_parameter, first.second_parameter), (0., 0.5));
    assert_eq!(first.second, [2., 0., 0.]);
    assert_eq!(first.distance, 3.);
    let second = closest_segments3(Vec3::ZERO, Vec3::X * 4., p, p).unwrap();
    assert_eq!((second.first_parameter, second.second_parameter), (0.5, 0.));
    assert_eq!(second.first, first.second);
    assert_eq!(second.second, first.first);
    let both = closest_segments3(p, p, Vec3::ZERO, Vec3::ZERO).unwrap();
    assert_eq!((both.first_parameter, both.second_parameter), (0., 0.));
    assert_eq!(both.distance, 13_f64.sqrt());
    let same = closest_segments2(Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, Vec2::ZERO).unwrap();
    assert_eq!(same.distance, 0.);
    assert_eq!((same.first_parameter, same.second_parameter), (0., 0.));
}

#[test]
fn subnormal_segments_and_separations_do_not_collapse() {
    let tiny = f32::from_bits(1);
    let p = vec2(tiny, tiny);
    let hit = closest_point_segment2(p, Vec2::ZERO, vec2(2. * tiny, 0.)).unwrap();
    assert_eq!(hit.second_parameter, 0.5);
    assert_eq!(hit.second, [f64::from(tiny), 0.]);
    assert_eq!(hit.distance, f64::from(tiny));
    let pair = closest_segments3(
        vec3(-tiny, 0., 0.),
        vec3(tiny, 0., 0.),
        vec3(0., -tiny, tiny),
        vec3(0., tiny, tiny),
    )
    .unwrap();
    assert_eq!((pair.first_parameter, pair.second_parameter), (0.5, 0.5));
    assert_eq!(pair.distance, f64::from(tiny));
}

#[test]
fn opposite_extreme_f32_coordinates_keep_finite_f64_distances() {
    let max = f32::MAX;
    let p = Vec3::splat(max);
    let a = Vec3::splat(-max);
    let hit = closest_point_segment3(p, a, a).unwrap();
    assert!(hit.distance > f64::from(max));
    consistent(hit, p.to_array(), p.to_array(), a.to_array(), a.to_array());
    near(hit.distance / f64::from(max), 2. * 3_f64.sqrt(), 1e-15);
    let long = closest_point_segment2(vec2(0., max), vec2(-max, 0.), vec2(max, 0.)).unwrap();
    assert_eq!(long.second, [0., 0.]);
    assert_eq!(long.second_parameter, 0.5);
    assert_eq!(long.distance, f64::from(max));
    let crossing = closest_segments3(
        vec3(-max, -max, 0.),
        vec3(max, max, 0.),
        vec3(max, -max, max),
        vec3(-max, max, max),
    )
    .unwrap();
    assert_eq!(
        (crossing.first_parameter, crossing.second_parameter),
        (0.5, 0.5)
    );
    assert_eq!(crossing.first, [0., 0., 0.]);
    assert_eq!(crossing.second, [0., 0., f64::from(max)]);
    assert_eq!(crossing.distance, f64::from(max));
}

#[test]
fn distance_does_not_underflow_when_f64_witness_separation_is_tiny() {
    // Exact powers of two isolate the interior skew separation, 2^-701.
    // Its square underflows f64, although the distance itself is representable.
    let tiny = f32::from_bits(1);
    let huge = 2_f32.powi(127);
    let hit = closest_segments3(
        Vec3::ZERO,
        vec3(huge, tiny, 0.),
        vec3(tiny, 0., 0.),
        vec3(tiny, huge, tiny),
    )
    .unwrap();
    assert_eq!(hit.first_parameter, 2_f64.powi(-276));
    assert_eq!(hit.second_parameter, 2_f64.powi(-552));
    assert_eq!(hit.first, [f64::from(tiny), 2_f64.powi(-425), 0.]);
    assert_eq!(
        hit.second,
        [f64::from(tiny), 2_f64.powi(-425), 2_f64.powi(-701)]
    );
    assert_eq!(hit.distance, 2_f64.powi(-701));
    assert!(hit.distance > 0.);
}

#[test]
fn witnesses_retain_f64_precision_instead_of_rounding_to_vec() {
    let hit = closest_point_segment2(vec2(1., 0.), Vec2::ZERO, vec2(1., 1.) * 3.).unwrap();
    near(hit.second_parameter, 1. / 6., 1e-16);
    assert_eq!(hit.second, [0.5, 0.5]);
    let hit = closest_point_segment2(vec2(1., 0.), Vec2::ZERO, vec2(1., 2.)).unwrap();
    assert_ne!(hit.second[0], f64::from(hit.second[0] as f32));
    consistent(hit, [1., 0.], [1., 0.], [0., 0.], [1., 2.]);
}

#[test]
fn near_parallel_crossings_are_not_lost_to_a_gram_determinant_or_epsilon() {
    for scale in [1e-30_f32, 1., 1e30] {
        let height = scale * 1e-10;
        let a = vec2(-scale, 0.);
        let b = vec2(scale, 0.);
        let c = vec2(-scale, -height);
        let d = vec2(scale, height);
        let hit = closest_segments2(a, b, c, d).unwrap();
        assert_eq!((hit.first_parameter, hit.second_parameter), (0.5, 0.5));
        assert_eq!(hit.first, [0., 0.]);
        assert_eq!(hit.second, [0., 0.]);
        assert_eq!(hit.distance, 0.);
        consistent(hit, a.to_array(), b.to_array(), c.to_array(), d.to_array());
    }
    let hit = closest_segments3(
        vec3(-1., 0., 0.),
        vec3(1., 0., 0.),
        vec3(-1., -1e-20, 2.),
        vec3(1., 1e-20, 2.),
    )
    .unwrap();
    assert_eq!((hit.first_parameter, hit.second_parameter), (0.5, 0.5));
    assert_eq!(hit.first, [0., 0., 0.]);
    assert_eq!(hit.second, [0., 0., 2.]);
    assert_eq!(hit.distance, 2.);
}

#[test]
fn nonfinite_coordinates_in_every_argument_and_axis_are_rejected() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..3 {
            for slot in 0..4 {
                let mut points = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z];
                points[slot][axis] = bad;
                assert_eq!(
                    closest_segments3(points[0], points[1], points[2], points[3]),
                    Err(SegmentError::NonFiniteInput),
                );
                if axis < 2 {
                    let points = points.map(Vec3::truncate);
                    assert_eq!(
                        closest_segments2(points[0], points[1], points[2], points[3]),
                        Err(SegmentError::NonFiniteInput),
                    );
                }
            }
            for slot in 0..3 {
                let mut points = [Vec3::ZERO, Vec3::X, Vec3::Y];
                points[slot][axis] = bad;
                assert_eq!(
                    closest_point_segment3(points[0], points[1], points[2]),
                    Err(SegmentError::NonFiniteInput),
                );
                if axis < 2 {
                    let points = points.map(Vec3::truncate);
                    assert_eq!(
                        closest_point_segment2(points[0], points[1], points[2]),
                        Err(SegmentError::NonFiniteInput),
                    );
                }
            }
        }
    }
}

#[test]
fn independent_queries_keep_no_failure_or_geometry_history() {
    let expected = closest_point_segment2(vec2(2., 3.), Vec2::ZERO, Vec2::X * 4.).unwrap();
    for _ in 0..64 {
        assert!(closest_point_segment2(Vec2::NAN, Vec2::ZERO, Vec2::ZERO).is_err());
        let zero = closest_segments2(Vec2::ONE, Vec2::ONE, Vec2::ONE, Vec2::ONE).unwrap();
        assert_eq!(zero.distance, 0.);
        assert_eq!(
            closest_point_segment2(vec2(2., 3.), Vec2::ZERO, Vec2::X * 4.).unwrap(),
            expected,
        );
    }
    assert_eq!(
        SegmentError::NonFiniteInput.to_string(),
        "segment query coordinates must be finite"
    );
}

#[test]
fn seeded_pairs_satisfy_convex_minimum_conditions_and_witness_consistency() {
    let mut seed = 218_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) as i32 % 1000) as f32 / 32.
    };
    for _ in 0..1000 {
        let [a, b, c, d] = std::array::from_fn(|_| vec3(next(), next(), next()));
        let hit = closest_segments3(a, b, c, d).unwrap();
        consistent(hit, a.to_array(), b.to_array(), c.to_array(), d.to_array());
        let gradient = |from: Vec3, to: Vec3, sign: f64| -> f64 {
            (0..3)
                .map(|i| {
                    sign * (f64::from(to[i]) - f64::from(from[i])) * (hit.first[i] - hit.second[i])
                })
                .sum()
        };
        // These first-order conditions are sufficient for this convex squared
        // distance problem on the parameter square, including endpoint minima.
        for (parameter, derivative) in [
            (hit.first_parameter, gradient(a, b, 1.)),
            (hit.second_parameter, gradient(c, d, -1.)),
        ] {
            if parameter == 0. {
                assert!(derivative >= -1e-9, "{hit:?}: derivative {derivative}");
            } else if parameter == 1. {
                assert!(derivative <= 1e-9, "{hit:?}: derivative {derivative}");
            } else {
                near(derivative, 0., 1e-9);
            }
        }
        let swapped = closest_segments3(c, d, a, b).unwrap();
        near(hit.distance, swapped.distance, 1e-12);
        for i in 0..3 {
            near(hit.first[i], swapped.second[i], 1e-11);
            near(hit.second[i], swapped.first[i], 1e-11);
        }
    }
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
fn allocation_counter_observes_a_real_heap_allocation() {
    assert!(
        allocations(|| {
            black_box(Box::new(218));
        }) > 0
    );
}

#[test]
fn all_query_kinds_degeneracy_errors_and_recovery_allocate_nothing() {
    let calls = allocations(|| {
        for _ in 0..64 {
            black_box(closest_point_segment2(
                black_box(vec2(2., 3.)),
                Vec2::ZERO,
                Vec2::X * 4.,
            ))
            .unwrap();
            black_box(closest_point_segment3(
                black_box(Vec3::splat(f32::MAX)),
                Vec3::splat(-f32::MAX),
                Vec3::ZERO,
            ))
            .unwrap();
            black_box(closest_segments2(Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE)).unwrap();
            black_box(closest_segments3(
                Vec3::NEG_X,
                Vec3::X,
                vec3(0., -1., 2.),
                vec3(0., 1., 2.),
            ))
            .unwrap();
            black_box(closest_segments3(
                Vec3::ONE,
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::ZERO,
            ))
            .unwrap();
            assert_eq!(
                black_box(closest_point_segment2(Vec2::NAN, Vec2::ZERO, Vec2::X,)),
                Err(SegmentError::NonFiniteInput)
            );
            assert_eq!(
                black_box(closest_point_segment3(Vec3::ZERO, Vec3::INFINITY, Vec3::X,)),
                Err(SegmentError::NonFiniteInput)
            );
            assert_eq!(
                black_box(closest_segments2(
                    Vec2::ZERO,
                    Vec2::X,
                    Vec2::NEG_INFINITY,
                    Vec2::ONE,
                )),
                Err(SegmentError::NonFiniteInput)
            );
            assert_eq!(
                black_box(closest_segments3(Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::NAN,)),
                Err(SegmentError::NonFiniteInput)
            );
        }
    });
    assert_eq!(calls, 0);
}
