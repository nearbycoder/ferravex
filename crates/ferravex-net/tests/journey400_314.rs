//! Public precision, endpoint, freshness, capacity and reconnect regressions.
use ferravex_net::history::{
    ClockSync, History, HistoryBracket as Bracket, HistoryEndpointPolicy as Endpoints,
    HistoryQueryError as QueryError,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;

struct CountAlloc;
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
fn count_allocation() {
    if TRACK.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }
}
// SAFETY: Every allocation request is forwarded unchanged to System. The
// const-initialized, thread-local counters do not allocate or track other tests.
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: GlobalAlloc's caller supplies a valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: GlobalAlloc's caller supplies a valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller supplies the System allocation and valid new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies the original System allocation and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

#[test]
#[should_panic]
fn zero_capacity_still_panics() {
    History::<u8>::new(0);
}

#[test]
#[should_panic]
fn capacity_above_4096_still_panics() {
    History::<u8>::new(4097);
}

#[test]
fn invalid_queries_precede_empty_stale_and_clamping() {
    let mut history = History::new(1);
    for occupied in [false, true] {
        if occupied {
            assert!(history.push(0, "first"));
        }
        for endpoints in [Endpoints::Reject, Endpoints::Clamp] {
            for fraction in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                -1.0,
                -f64::MIN_POSITIVE,
                1.0 + f64::EPSILON,
            ] {
                assert_eq!(
                    history.bracket_precise(0, fraction, endpoints, Some(u64::MAX)),
                    Err(QueryError::InvalidFraction)
                );
            }
            assert_eq!(
                history.bracket_precise(u64::MAX, 1.0, endpoints, Some(u64::MAX)),
                Err(QueryError::TickOverflow)
            );
            if !occupied {
                for (tick, fraction) in [(0, 0.0), (0, 1.0), (u64::MAX, 0.5)] {
                    assert_eq!(
                        history.bracket_precise(tick, fraction, endpoints, Some(u64::MAX)),
                        Ok(Bracket::Empty)
                    );
                }
            }
        }
        assert_eq!(history.len(), usize::from(occupied));
    }
    assert_eq!(history.iter().collect::<Vec<_>>(), [(0, &"first")]);
}

#[test]
fn one_sample_has_exact_rejected_or_explicitly_clamped_outcomes() {
    assert_eq!(Endpoints::default(), Endpoints::Reject);
    for tick in [0, 10, u64::MAX] {
        let mut history = History::new(1);
        assert!(history.push(tick, "only"));
        for endpoints in [Endpoints::Reject, Endpoints::Clamp] {
            for fraction in [0.0, -0.0] {
                assert_eq!(
                    history.bracket_precise(tick, fraction, endpoints, Some(tick)),
                    Ok(Bracket::Exact {
                        sample: (tick, &"only")
                    })
                );
            }
        }
        assert_eq!(
            history.bracket_precise(tick, 0.25, Endpoints::Reject, None),
            Ok(Bracket::AfterNewest {
                newest: (tick, &"only")
            })
        );
        assert_eq!(
            history.bracket_precise(tick, 0.25, Endpoints::Clamp, None),
            Ok(Bracket::Clamped {
                sample: (tick, &"only")
            })
        );
        if tick > 0 {
            assert_eq!(
                history.bracket_precise(tick - 1, 0.75, Endpoints::Reject, None),
                Ok(Bracket::BeforeOldest {
                    oldest: (tick, &"only")
                })
            );
            assert_eq!(
                history.bracket_precise(tick - 1, 0.75, Endpoints::Clamp, None),
                Ok(Bracket::Clamped {
                    sample: (tick, &"only")
                })
            );
            assert_eq!(
                history.bracket_precise(tick - 1, 1.0, Endpoints::Reject, None),
                Ok(Bracket::Exact {
                    sample: (tick, &"only")
                })
            );
        }
    }
}

#[test]
fn adjacent_large_ticks_and_extremes_preserve_sample_identity() {
    // Non-Clone states prove queries borrow actual history rather than cloning it.
    struct Pose(f64);
    for base in [0, 1 << 53, (1 << 53) + 1, u64::MAX - 2] {
        let mut history = History::new(3);
        for i in 0..3 {
            assert!(history.push(base + i, Pose(i as f64 * 4.0)));
        }
        for (tick, pose) in history.iter() {
            let Bracket::Exact { sample } = history
                .bracket_precise(tick, 0.0, Endpoints::Reject, None)
                .unwrap()
            else {
                panic!("expected exact tick {tick}");
            };
            assert_eq!(sample.0, tick);
            assert!(std::ptr::eq(sample.1, pose));
        }
        for i in 0..2 {
            for fraction in [f64::from_bits(1), 0.25, 0.5, 1.0 - f64::EPSILON] {
                let Bracket::Interpolated {
                    before: (a_tick, a),
                    after: (b_tick, b),
                    alpha,
                } = history
                    .bracket_precise(base + i, fraction, Endpoints::Reject, None)
                    .unwrap()
                else {
                    panic!("expected adjacent retained samples");
                };
                assert_eq!((a_tick, b_tick), (base + i, base + i + 1));
                assert_eq!(alpha, fraction);
                assert_eq!(a.0 + (b.0 - a.0) * alpha, i as f64 * 4.0 + 4.0 * fraction);
            }
            assert!(matches!(history.bracket_precise(base + i, 1.0,
                Endpoints::Reject, None).unwrap(),
                Bracket::Exact { sample: (tick, _) } if tick == base + i + 1));
        }
    }
}

#[test]
fn sparse_full_width_gaps_use_local_subtraction_and_approximate_weights() {
    let mut history = History::new(2);
    history.push(0, 0.0);
    history.push(u64::MAX, 1.0);
    for (tick, fraction) in [(0, 0.5), (1, 0.0), (1 << 63, 0.5), (u64::MAX - 1, 0.5)] {
        let bracket = history
            .bracket_precise(tick, fraction, Endpoints::Reject, None)
            .unwrap();
        let Bracket::Interpolated {
            before: (0, _),
            after: (u64::MAX, _),
            alpha,
        } = bracket
        else {
            panic!("expected wide-gap interpolation: {bracket:?}");
        };
        assert!(alpha.is_finite() && (0.0..=1.0).contains(&alpha));
        if tick == 0 {
            assert!(alpha > 0.0);
        }
        if tick == u64::MAX - 1 {
            // Rounded weight is NOT exact identity.
            assert_eq!(alpha, 1.0);
        }
    }
    history.clear();
    history.push((1 << 53) + 1, 2.0);
    history.push((1 << 53) + 5, 10.0);
    assert_eq!(
        history.bracket_precise((1 << 53) + 3, 0.25, Endpoints::Reject, None),
        Ok(Bracket::Interpolated {
            before: ((1 << 53) + 1, &2.0),
            after: ((1 << 53) + 5, &10.0),
            alpha: 0.5625,
        })
    );
}

#[test]
fn stale_is_only_a_supplied_inclusive_newest_cutoff_not_target_age() {
    let mut history = History::new(2);
    history.push(10, "a");
    history.push(20, "b");
    for endpoints in [Endpoints::Reject, Endpoints::Clamp] {
        for (tick, fraction) in [(0, 0.0), (10, 0.0), (15, 0.5), (u64::MAX, 0.5)] {
            assert_eq!(
                history.bracket_precise(tick, fraction, endpoints, Some(21)),
                Ok(Bracket::Stale {
                    newest: (20, &"b"),
                    minimum_newest_tick: 21,
                })
            );
        }
        for cutoff in [None, Some(0), Some(20)] {
            assert_eq!(
                history.bracket_precise(10, 0.0, endpoints, cutoff),
                Ok(Bracket::Exact { sample: (10, &"a") })
            );
        }
    }
    assert_eq!(
        history.bracket_precise(u64::MAX, 0.0, Endpoints::Reject, None),
        Ok(Bracket::AfterNewest { newest: (20, &"b") })
    );
    assert!(history.push(u64::MAX, "max"));
    assert_eq!(
        history.bracket_precise(u64::MAX, 0.0, Endpoints::Reject, Some(u64::MAX)),
        Ok(Bracket::Exact {
            sample: (u64::MAX, &"max")
        })
    );
}

#[test]
fn bounded_eviction_and_rejected_order_work_across_wrapped_storage() {
    for capacity in [1, 3, 4096] {
        let mut history = History::new(capacity);
        for tick in 0..(capacity * 3) as u64 {
            assert!(history.push(tick, tick));
            let expected_len = capacity.min(tick as usize + 1);
            assert_eq!(history.len(), expected_len);
            let first = tick + 1 - expected_len as u64;
            assert_eq!(history.first_tick(), Some(first));
            for rejected in [tick, tick.saturating_sub(1), 0] {
                assert!(!history.push(rejected, u64::MAX));
            }
            assert_eq!(history.len(), expected_len);
            assert_eq!(history.first_tick(), Some(first));
            for sample_tick in [first, tick] {
                assert_eq!(
                    history.bracket_precise(sample_tick, 0.0, Endpoints::Reject, None),
                    Ok(Bracket::Exact {
                        sample: (sample_tick, &sample_tick)
                    })
                );
            }
            if expected_len > 1 {
                assert_eq!(
                    history.bracket_precise(first, 0.5, Endpoints::Reject, None),
                    Ok(Bracket::Interpolated {
                        before: (first, &first),
                        after: (first + 1, &(first + 1)),
                        alpha: 0.5,
                    })
                );
            }
            if first > 0 {
                assert!(matches!(
                    history
                        .bracket_precise(first - 1, 0.5, Endpoints::Reject, None)
                        .unwrap(),
                    Bracket::BeforeOldest { .. }
                ));
            }
        }
        assert!(history.iter().all(|(tick, state)| tick == *state));
        assert!(history.push(u64::MAX, u64::MAX));
        assert!(!history.push(0, 0)); // No implicit tick wrap or epoch reset.
    }
}

#[test]
fn clear_drops_occupied_states_forgets_order_and_can_be_repeated() {
    struct Dropped(Rc<Cell<usize>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let mut history = History::new(2);
    history.push(10, Dropped(drops.clone()));
    history.push(20, Dropped(drops.clone()));
    assert!(!history.push(20, Dropped(drops.clone())));
    assert_eq!(drops.get(), 1); // Rejected input still follows existing drop policy.
    history.push(u64::MAX, Dropped(drops.clone()));
    assert_eq!(drops.get(), 2); // Only the oldest accepted state was evicted.
    history.clear();
    assert_eq!(drops.get(), 4);
    assert_eq!(history.first_tick(), None);
    assert!(history.is_empty());
    assert_eq!(history.iter().count(), 0);
    assert!(history.bracket(20.0).is_none());
    assert!(matches!(
        history
            .bracket_precise(20, 0.5, Endpoints::Clamp, None)
            .unwrap(),
        Bracket::Empty
    ));
    history.clear();
    assert_eq!(drops.get(), 4);
    assert!(history.push(0, Dropped(drops.clone())));
    assert!(history.push(1, Dropped(drops.clone())));
    history.clear();
    assert_eq!(drops.get(), 6);
}

#[test]
fn legacy_float_rejection_interpolation_and_clock_smoothing_are_preserved() {
    let mut history = History::new(2);
    assert!(history.bracket(0.0).is_none());
    history.push(10, 2.0);
    history.push(12, 6.0);
    for tick in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 9.9, 12.1] {
        assert!(history.bracket(tick).is_none());
    }
    assert_eq!(history.bracket(10.0), Some((&2.0, &2.0, 0.0)));
    assert_eq!(history.bracket(10.5), Some((&2.0, &6.0, 0.25)));
    assert_eq!(history.bracket(12.0), Some((&6.0, &6.0, 0.0)));
    let mut clock = ClockSync::default();
    assert!(clock.observe(0.0, 10.0, 10.0, 0.0));
    assert!(clock.observe(0.0, 18.0, 18.0, 0.0));
    assert_eq!(clock.offset_seconds, 11.0); // 1/8 smoothing after first sample.
    assert!(!clock.observe(0.0, 0.0, 0.0, 6.0));
    assert_eq!(clock.server_time(2.0), 13.0);
}

#[test]
fn warmed_queries_eviction_rejection_and_occupied_clear_reuse_allocate_nothing() {
    for capacity in [1, 3, 4096] {
        let mut history = History::new(capacity);
        // Warm full storage before counting, with a nonallocating state type.
        for tick in 0..capacity as u64 {
            assert!(history.push(tick, tick));
        }
        ALLOCATIONS.with(|count| count.set(0));
        TRACK.with(|track| track.set(true));
        for epoch in 0..4 {
            for tick in capacity as u64..(capacity * 3) as u64 {
                assert!(history.push(tick, tick));
                assert!(!history.push(tick, 0));
                std::hint::black_box(history.bracket(tick as f64));
                for endpoints in [Endpoints::Reject, Endpoints::Clamp] {
                    for (query_tick, fraction, cutoff) in [
                        (tick, 0.0, None),
                        (tick - 1, 0.5, None),
                        (0, 0.5, None),
                        (tick, 0.5, None),
                        (tick, 0.0, Some(tick + 1)),
                        (tick, f64::NAN, None),
                        (u64::MAX, 1.0, None),
                    ] {
                        let _ = std::hint::black_box(
                            history.bracket_precise(query_tick, fraction, endpoints, cutoff),
                        );
                    }
                }
            }
            history.clear();
            assert!(history.is_empty());
            let _ = std::hint::black_box(history.bracket_precise(0, 0.0, Endpoints::Clamp, None));
            for tick in 0..capacity as u64 {
                assert!(history.push(tick, tick + epoch));
            }
        }
        TRACK.with(|track| track.set(false));
        assert_eq!(ALLOCATIONS.with(Cell::get), 0, "alloc/alloc_zeroed/realloc");
        assert_eq!(history.len(), capacity);
    }
}
