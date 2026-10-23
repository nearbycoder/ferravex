//! Task250 public API regressions and scoped native allocation evidence.
use ferravex_math::Vec2;
use ferravex_play::gameplay::{
    LoopError,
    spawn::{
        SpawnAssessment, SpawnCheckError, SpawnExclusion, SpawnOutcome, SpawnQuery, SpawnSelector,
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::{Cell, RefCell},
    hint::black_box,
};

fn assess(
    selector: &SpawnSelector,
    points: &[Vec2],
    reference: Vec2,
    output: &mut [SpawnAssessment],
) -> Result<Option<usize>, SpawnCheckError<()>> {
    selector.assess_into(
        points,
        reference,
        output,
        |_| Ok(true),
        |_, distance| Ok(Some(distance)),
        |_| Ok(true),
    )
}
fn choose(
    selector: &mut SpawnSelector,
    points: &[Vec2],
    output: &mut [SpawnAssessment],
) -> Result<Option<usize>, SpawnCheckError<()>> {
    selector.choose_checked_into(
        points,
        Vec2::ZERO,
        output,
        |_| Ok(true),
        |_, distance| Ok(Some(distance)),
        |_| Ok(true),
    )
}
fn sentinel() -> SpawnAssessment {
    SpawnAssessment {
        index: usize::MAX,
        distance_squared: Some(123.),
        outcome: SpawnOutcome::Eligible { score: -5. },
    }
}

#[test]
fn legacy_signatures_limits_pruning_and_tie_rotation_are_preserved() {
    for minimum in [f32::NAN, f32::INFINITY, -1., 1_000_001.] {
        assert!(matches!(
            SpawnSelector::new(minimum),
            Err(LoopError::Invalid)
        ));
    }
    let mut selector = SpawnSelector::new(10.).unwrap();
    let points = [20., -20., 30., 25., 40.].map(|x| Vec2::new(x, 0.));
    let mut queried = Vec::new();
    assert_eq!(
        selector.choose(&points, Vec2::ZERO, |i| {
            queried.push(i);
            true
        }),
        Ok(Some(4))
    );
    assert_eq!(queried, [0, 2, 4]);
    // The new diagnostics have deliberately not changed legacy pruning.
    queried.clear();
    assert_eq!(
        selector.choose(&points, Vec2::ZERO, |i| {
            queried.push(i);
            i != 2 && i != 4
        }),
        Ok(Some(3))
    );
    assert_eq!(queried, [0, 2, 3, 4]);
    for (p, reference) in [
        (&[][..], Vec2::ZERO),
        (&[Vec2::X; 257][..], Vec2::ZERO),
        (&[Vec2::new(f32::NAN, 0.)][..], Vec2::ZERO),
        (&[Vec2::X][..], Vec2::new(0., f32::INFINITY)),
    ] {
        assert_eq!(
            selector.choose(p, reference, |_| panic!("invalid request")),
            Err(LoopError::Invalid)
        );
    }
    let ties = [Vec2::X * 10., -Vec2::X * 10.];
    selector.reset();
    assert_eq!(selector.choose(&ties, Vec2::ZERO, |_| true), Ok(Some(0)));
    assert_eq!(selector.choose(&ties, Vec2::ZERO, |_| true), Ok(Some(1)));
    assert_eq!(selector.choose(&ties, Vec2::ZERO, |_| false), Ok(None));
    assert_eq!(selector.choose(&ties, Vec2::ZERO, |_| true), Ok(Some(0)));
}

#[test]
fn preflight_errors_make_no_queries_and_preserve_all_output_and_cursor() {
    let mut selector = SpawnSelector::new(0.).unwrap();
    let ties = [Vec2::X, -Vec2::X];
    let mut output = [sentinel(); 3];
    assert_eq!(selector.choose(&ties, Vec2::ZERO, |_| true), Ok(Some(0)));
    let cases = [
        (&[][..], Vec2::ZERO, 3, SpawnCheckError::EmptyCandidates),
        (
            &[Vec2::X; 257][..],
            Vec2::ZERO,
            3,
            SpawnCheckError::CandidateCapacity { count: 257 },
        ),
        (
            &ties[..],
            Vec2::new(f32::NAN, 0.),
            3,
            SpawnCheckError::InvalidReference,
        ),
        (
            &ties[..],
            Vec2::new(0., 1_000_001.),
            3,
            SpawnCheckError::InvalidReference,
        ),
        (
            &ties[..],
            Vec2::new(f32::MAX, 0.),
            0,
            SpawnCheckError::InvalidReference,
        ),
        (
            &ties[..],
            Vec2::ZERO,
            1,
            SpawnCheckError::OutputTooSmall {
                required: 2,
                provided: 1,
            },
        ),
        (
            &ties[..],
            Vec2::ZERO,
            0,
            SpawnCheckError::OutputTooSmall {
                required: 2,
                provided: 0,
            },
        ),
    ];
    for (points, reference, capacity, expected) in cases {
        let result = selector.choose_checked_into::<()>(
            points,
            reference,
            &mut output[..capacity],
            |_| panic!("availability"),
            |_, _| panic!("score"),
            |_| panic!("obstruction"),
        );
        assert_eq!(result, Err(expected));
        assert_eq!(output, [sentinel(); 3]);
        let mut diagnostics = [SpawnAssessment::EMPTY; 2];
        assert_eq!(
            assess(&selector, &ties, Vec2::ZERO, &mut diagnostics),
            Ok(Some(1))
        );
    }
}

#[test]
fn first_exclusions_skip_later_callbacks_and_keep_source_identity_and_tail() {
    let points = [
        Vec2::new(f32::NAN, 0.),
        Vec2::X,
        Vec2::X * 10.,
        Vec2::X * 20.,
        Vec2::X * 30.,
        Vec2::X * 40.,
    ];
    let selector = SpawnSelector::new(10.).unwrap();
    let log = RefCell::new(Vec::new());
    let mut output = [sentinel(); 7];
    let result = selector.assess_into(
        &points,
        Vec2::ZERO,
        &mut output,
        |i| {
            log.borrow_mut().push((i, SpawnQuery::Availability));
            Ok::<_, ()>(i != 2)
        },
        |i, d| {
            log.borrow_mut().push((i, SpawnQuery::Score));
            Ok((i != 3).then_some(d))
        },
        |i| {
            log.borrow_mut().push((i, SpawnQuery::Obstruction));
            Ok(i != 4)
        },
    );
    assert_eq!(result, Ok(Some(5)));
    assert_eq!(
        *log.borrow(),
        [
            (2, SpawnQuery::Availability),
            (3, SpawnQuery::Availability),
            (3, SpawnQuery::Score),
            (4, SpawnQuery::Availability),
            (4, SpawnQuery::Score),
            (4, SpawnQuery::Obstruction),
            (5, SpawnQuery::Availability),
            (5, SpawnQuery::Score),
            (5, SpawnQuery::Obstruction),
        ]
    );
    for (i, reason) in [
        SpawnExclusion::InvalidGeometry,
        SpawnExclusion::TooClose,
        SpawnExclusion::Unavailable,
        SpawnExclusion::Authored,
        SpawnExclusion::Obstructed,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(output[i].index, i);
        assert_eq!(output[i].outcome, SpawnOutcome::Excluded(reason));
        assert_eq!(
            output[i].distance_squared,
            (i != 0).then(|| points[i].length_squared())
        );
    }
    assert_eq!(output[5].index, 5);
    assert_eq!(output[5].outcome, SpawnOutcome::Eligible { score: 1600. });
    assert_eq!(output[6], sentinel());
}

#[test]
fn zero_and_all_invalid_and_unavailable_candidates_never_fabricate_fallbacks() {
    let mut selector = SpawnSelector::new(10.).unwrap();
    let mut output = [sentinel(); 4];
    let invalid = [
        Vec2::new(f32::INFINITY, 0.),
        Vec2::new(0., f32::NEG_INFINITY),
        Vec2::new(f32::MAX, 0.),
        Vec2::new(0., -1_000_001.),
    ];
    assert_eq!(
        selector.choose_checked_into::<()>(
            &invalid,
            Vec2::ZERO,
            &mut output,
            |_| panic!(),
            |_, _| panic!(),
            |_| panic!()
        ),
        Ok(None)
    );
    for (i, entry) in output.iter().enumerate() {
        assert_eq!(
            *entry,
            SpawnAssessment {
                index: i,
                ..SpawnAssessment::EMPTY
            }
        );
    }
    let close = [Vec2::ZERO; 4];
    assert_eq!(
        selector.choose_checked_into::<()>(
            &close,
            Vec2::ZERO,
            &mut output,
            |_| panic!(),
            |_, _| panic!(),
            |_| panic!()
        ),
        Ok(None)
    );
    assert!(
        output
            .iter()
            .all(|a| a.outcome == SpawnOutcome::Excluded(SpawnExclusion::TooClose))
    );
    let points = [Vec2::X * 20.; 4];
    assert_eq!(
        selector.choose_checked_into::<()>(
            &points,
            Vec2::ZERO,
            &mut output,
            |_| Ok(false),
            |_, _| panic!(),
            |_| panic!()
        ),
        Ok(None)
    );
    assert_eq!(choose(&mut selector, &points, &mut output), Ok(Some(0)));
}

#[test]
fn exact_minimum_is_eligible_and_moving_reference_recomputes_every_distance() {
    let mut selector = SpawnSelector::new(10.).unwrap();
    let points = [
        Vec2::new(10., 0.),
        Vec2::new(-10., 0.),
        Vec2::new(9.999, 0.),
    ];
    let mut output = [SpawnAssessment::EMPTY; 3];
    for _ in 0..3 {
        assert_eq!(
            assess(&selector, &points, Vec2::ZERO, &mut output),
            Ok(Some(0))
        );
        assert_eq!(output[0].distance_squared, Some(100.));
        assert_eq!(output[1].outcome, SpawnOutcome::Eligible { score: 100. });
        assert_eq!(
            output[2].outcome,
            SpawnOutcome::Excluded(SpawnExclusion::TooClose)
        );
    }
    assert_eq!(choose(&mut selector, &points, &mut output), Ok(Some(0)));
    for _ in 0..3 {
        assert_eq!(
            assess(&selector, &points, Vec2::ZERO, &mut output),
            Ok(Some(1))
        );
    }
    assert_eq!(
        assess(&selector, &points, Vec2::new(-10., 0.), &mut output),
        Ok(Some(0))
    );
    assert_eq!(output[0].distance_squared, Some(400.));
    assert_eq!(
        output[1].outcome,
        SpawnOutcome::Excluded(SpawnExclusion::TooClose)
    );
    // Pure assessment with a moving reference did not consume the old tie.
    assert_eq!(choose(&mut selector, &points, &mut output), Ok(Some(1)));
    selector.reset();
    assert_eq!(
        choose(&mut selector, &points[..1], &mut output),
        Ok(Some(0))
    );
    // A successful selection in a one-element list leaves its cursor at zero.
    assert_eq!(choose(&mut selector, &points, &mut output), Ok(Some(0)));
    selector.reset();
    assert_eq!(choose(&mut selector, &points, &mut output), Ok(Some(0)));
}

#[test]
fn equal_scores_rotate_in_existing_cursor_order_and_output_stays_source_order() {
    let mut selector = SpawnSelector::new(0.).unwrap();
    let points = [Vec2::X, Vec2::X * 2., Vec2::X * 3.];
    let mut output = [sentinel(); 4];
    for expected in [0, 1, 2, 0] {
        let log = RefCell::new(Vec::new());
        assert_eq!(
            selector.choose_checked_into(
                &points,
                Vec2::ZERO,
                &mut output,
                |i| {
                    log.borrow_mut().push((i, 0));
                    Ok::<_, ()>(true)
                },
                |i, _| {
                    log.borrow_mut().push((i, 1));
                    Ok(Some(-10.))
                },
                |i| {
                    log.borrow_mut().push((i, 2));
                    Ok(true)
                },
            ),
            Ok(Some(expected))
        );
        let expected_log: Vec<_> = (0..3)
            .flat_map(|offset| {
                let i = (expected + offset) % 3;
                [(i, 0), (i, 1), (i, 2)]
            })
            .collect();
        assert_eq!(*log.borrow(), expected_log);
        for (i, assessment) in output[..3].iter().enumerate() {
            assert_eq!(assessment.index, i);
            assert_eq!(assessment.outcome, SpawnOutcome::Eligible { score: -10. });
        }
        assert_eq!(output[3], sentinel());
    }
}

#[test]
fn authored_scores_override_distance_and_allow_all_finite_extremes() {
    let mut selector = SpawnSelector::new(0.).unwrap();
    let points = [Vec2::X * 50., Vec2::X * 20., Vec2::X];
    let mut output = [sentinel(); 3];
    for (scores, expected) in [
        ([f32::MIN, -1., f32::MAX], 2),
        ([0., -0., f32::MIN], 0),
        ([f32::MIN; 3], 1),
    ] {
        assert_eq!(
            selector.choose_checked_into(
                &points,
                Vec2::ZERO,
                &mut output,
                |_| Ok::<_, ()>(true),
                |i, _| Ok(Some(scores[i])),
                |_| Ok(true)
            ),
            Ok(Some(expected))
        );
    }
    assert_eq!(
        selector.choose_checked_into(
            &points,
            Vec2::ZERO,
            &mut output,
            |_| Ok::<_, ()>(true),
            |_, _| Ok(None),
            |_| panic!()
        ),
        Ok(None)
    );
    assert!(
        output
            .iter()
            .all(|a| a.outcome == SpawnOutcome::Excluded(SpawnExclusion::Authored))
    );
    // Distance-zero candidates are valid at a zero minimum.
    assert_eq!(
        choose(&mut selector, &[Vec2::ZERO], &mut output),
        Ok(Some(0))
    );
    assert_eq!(output[0].distance_squared, Some(0.));
}

#[test]
fn callback_failures_are_attributed_and_do_not_publish_partial_work_or_rotate() {
    let points = [Vec2::X, -Vec2::X, Vec2::Y];
    for phase in [
        SpawnQuery::Availability,
        SpawnQuery::Score,
        SpawnQuery::Obstruction,
    ] {
        let mut selector = SpawnSelector::new(0.).unwrap();
        assert_eq!(selector.choose(&points, Vec2::ZERO, |_| true), Ok(Some(0)));
        let mut output = [sentinel(); 4];
        let log = RefCell::new(Vec::new());
        let query = |i, stage| {
            log.borrow_mut().push((i, stage));
            if i == 2 && stage == phase {
                Err("query failed")
            } else {
                Ok(true)
            }
        };
        assert_eq!(
            selector.choose_checked_into(
                &points,
                Vec2::ZERO,
                &mut output,
                |i| query(i, SpawnQuery::Availability),
                |i, d| query(i, SpawnQuery::Score).map(|_| Some(d)),
                |i| query(i, SpawnQuery::Obstruction)
            ),
            Err(SpawnCheckError::Callback {
                index: 2,
                query: phase,
                error: "query failed"
            })
        );
        assert_eq!(output, [sentinel(); 4]);
        // Candidate 1's external query effects remain, candidate 0 was not queried.
        assert_eq!(
            &log.borrow()[..3],
            &[
                (1, SpawnQuery::Availability),
                (1, SpawnQuery::Score),
                (1, SpawnQuery::Obstruction),
            ]
        );
        assert_eq!(log.borrow().last(), Some(&(2, phase)));
        assert!(!log.borrow().iter().any(|(i, _)| *i == 0));
        assert_eq!(choose(&mut selector, &points, &mut output), Ok(Some(1)));
    }
}

#[test]
fn invalid_scores_abort_before_obstruction_and_preserve_output_and_cursor() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut selector = SpawnSelector::new(0.).unwrap();
        let points = [Vec2::X, -Vec2::X, Vec2::Y];
        assert_eq!(selector.choose(&points, Vec2::ZERO, |_| true), Ok(Some(0)));
        let mut output = [sentinel(); 4];
        let mut clear_calls = Vec::new();
        assert_eq!(
            selector.choose_checked_into(
                &points,
                Vec2::ZERO,
                &mut output,
                |_| Ok::<_, ()>(true),
                |i, d| Ok(Some(if i == 2 { value } else { d })),
                |i| {
                    clear_calls.push(i);
                    Ok(true)
                }
            ),
            Err(SpawnCheckError::InvalidScore { index: 2 })
        );
        assert_eq!(clear_calls, [1]);
        assert_eq!(output, [sentinel(); 4]);
        assert_eq!(choose(&mut selector, &points, &mut output), Ok(Some(1)));
    }
}

#[test]
fn maximum_capacity_and_coordinate_extremes_have_bounded_finite_work() {
    let mut selector = SpawnSelector::new(1e6).unwrap();
    let points = [Vec2::new(1e6, 1e6); SpawnSelector::MAX_CANDIDATES];
    let reference = Vec2::new(-1e6, -1e6);
    let mut output = [sentinel(); SpawnSelector::MAX_CANDIDATES + 1];
    let mut calls = [0; 3];
    for expected in [0, 1] {
        let [available, score, clear] = &mut calls;
        assert_eq!(
            selector.choose_checked_into(
                &points,
                reference,
                &mut output,
                |_| {
                    *available += 1;
                    Ok::<_, ()>(true)
                },
                |_, d| {
                    *score += 1;
                    Ok(Some(d))
                },
                |_| {
                    *clear += 1;
                    Ok(true)
                }
            ),
            Ok(Some(expected))
        );
    }
    assert_eq!(calls, [512; 3]);
    for (i, entry) in output[..256].iter().enumerate() {
        assert_eq!(entry.index, i);
        assert_eq!(entry.distance_squared, Some(8e12));
        assert_eq!(entry.outcome, SpawnOutcome::Eligible { score: 8e12 });
    }
    assert_eq!(output[256], sentinel());
    selector.reset();
    assert_eq!(selector.choose(&points, reference, |_| true), Ok(Some(0)));
    // With a shorter candidate list the pre-existing cursor wraps modulo its length.
    let small = [Vec2::X * 1e6; 3];
    assert_eq!(choose(&mut selector, &small, &mut output), Ok(Some(1)));
    // Move the existing shared cursor to 255, then assess a shorter list.
    assert_eq!(
        selector.choose(&points, reference, |i| i == 254),
        Ok(Some(254))
    );
    assert_eq!(
        assess(&selector, &small, Vec2::ZERO, &mut output),
        Ok(Some(0))
    );
    assert_eq!(choose(&mut selector, &small, &mut output), Ok(Some(0)));
}

#[test]
fn one_eligible_candidate_wins_even_if_lower_ranked_than_obstructed_candidates() {
    let mut selector = SpawnSelector::new(10.).unwrap();
    let points = [Vec2::X * 100., Vec2::X * 10., Vec2::X * 200.];
    let mut output = [SpawnAssessment::EMPTY; 3];
    assert_eq!(
        selector.choose_checked_into(
            &points,
            Vec2::ZERO,
            &mut output,
            |_| Ok::<_, ()>(true),
            |_, d| Ok(Some(d)),
            |i| Ok(i == 1)
        ),
        Ok(Some(1))
    );
    assert_eq!(
        selector.choose_checked_into(
            &points,
            Vec2::ZERO,
            &mut output,
            |_| Ok::<_, ()>(true),
            |_, d| Ok(Some(d)),
            |_| Ok(false)
        ),
        Ok(None)
    );
    let log = RefCell::new(Vec::new());
    assert_eq!(
        selector.assess_into(
            &points,
            Vec2::ZERO,
            &mut output,
            |i| {
                log.borrow_mut().push(i);
                Ok::<_, ()>(true)
            },
            |_, d| Ok(Some(d)),
            |_| Ok(true)
        ),
        Ok(Some(2))
    );
    assert_eq!(*log.borrow(), [2, 0, 1]); // No-safe result did not rotate.
}

struct CountingAllocator;
thread_local! { static ALLOCS: Cell<Option<usize>> = const { Cell::new(None) }; }
fn count_alloc() {
    let _ = ALLOCS.try_with(|count| {
        if let Some(n) = count.get() {
            count.set(Some(n + 1));
        }
    });
}
// SAFETY: All allocation/deallocation contracts are forwarded unchanged to System.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_alloc();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_alloc();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_alloc();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn warmed_bounded_paths_including_failures_and_reset_do_not_allocate() {
    let mut selector = SpawnSelector::new(0.).unwrap();
    let points = [Vec2::X; 256];
    let mut output = [SpawnAssessment::EMPTY; 256];
    assert_eq!(choose(&mut selector, &points, &mut output), Ok(Some(0)));
    ALLOCS.with(|count| count.set(Some(0)));
    for _ in 0..100 {
        black_box(assess(&selector, black_box(&points), Vec2::ZERO, &mut output).unwrap());
        black_box(choose(&mut selector, &points, &mut output).unwrap());
        black_box(selector.choose(&points, Vec2::ZERO, |_| true).unwrap());
        black_box(
            selector
                .choose_checked_into(
                    &points,
                    Vec2::ZERO,
                    &mut output,
                    |_| Ok::<_, ()>(true),
                    |_, d| Ok(Some(d)),
                    |_| Ok(false),
                )
                .unwrap(),
        );
        assert!(
            selector
                .choose_checked_into(
                    &points,
                    Vec2::ZERO,
                    &mut output,
                    |_| Ok::<_, ()>(true),
                    |i, d| Ok(Some(if i == 255 { f32::NAN } else { d })),
                    |_| Ok(true)
                )
                .is_err()
        );
        assert!(
            selector
                .choose_checked_into(
                    &points,
                    Vec2::ZERO,
                    &mut output,
                    |i| if i == 255 { Err(()) } else { Ok(true) },
                    |_, d| Ok(Some(d)),
                    |_| Ok(true)
                )
                .is_err()
        );
        assert!(assess(&selector, &points, Vec2::ZERO, &mut output[..255]).is_err());
        selector.reset();
        black_box(selector);
    }
    let allocations = ALLOCS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
}
