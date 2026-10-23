//! Distinct weighted offers: public contracts, exact fixtures and allocation checks.
use ferravex_play::gameplay::{
    LoopError,
    loot::{
        DistinctDrawError, DistinctDrawReport, DistinctDrawStatus, MAX_DISTINCT_DRAWS,
        MAX_DISTINCT_SAMPLES, RewardRng, RewardTable,
    },
    progression::Progression,
    upgrades::Upgrades,
    waves::{WaveSequence, WaveSpec},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}

struct CountingAllocator;
// SAFETY: all operations delegate unchanged pointers/layouts to System. The
// allocation-free TLS counter observes calls on the measured test thread only.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations_during<T>(action: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|count| count.set(None));
        }
    }
    ALLOCATIONS.with(|count| {
        assert_eq!(count.get(), None);
        count.set(Some(0));
    });
    let reset = Reset;
    let result = action();
    let count = ALLOCATIONS.with(|count| count.get().unwrap());
    drop(reset);
    (result, count)
}

fn report(written: usize, samples_used: usize, status: DistinctDrawStatus) -> DistinctDrawReport {
    DistinctDrawReport {
        written,
        samples_used,
        status,
    }
}

#[test]
fn legacy_raw_choose_and_roll_golden_sequences_and_advancement_are_unchanged() {
    let mut rng = RewardRng::new(0);
    assert_eq!(
        std::array::from_fn::<_, 4, _>(|_| rng.next_u64()),
        [
            0xe220_a839_7b1d_cdaf,
            0x6e78_9e6a_a1b9_65f4,
            0x06c4_5d18_8009_454f,
            0xf88b_b8a8_724c_81ec,
        ]
    );
    let sparse = RewardTable::new([1, 0, 3]).unwrap();
    assert_eq!(
        [0, 1, 2, 3, 4, 5, 6, 7, u32::MAX].map(|ticket| sparse.choose(ticket)),
        [0, 2, 2, 2, 0, 2, 2, 2, 2]
    );
    let mut rng = RewardRng::new(0);
    assert_eq!(
        std::array::from_fn::<_, 10, _>(|_| sparse.roll(&mut rng)),
        [2, 2, 0, 2, 0, 2, 0, 2, 0, 2]
    );
    // A non-power-of-two total pins the legacy multiply-high mapping too.
    let seven = RewardTable::new([1; 7]).unwrap();
    let mut rng = RewardRng::new(42);
    let mut mirror = rng;
    assert_eq!(
        std::array::from_fn::<_, 12, _>(|_| seven.roll(&mut rng)),
        [5, 1, 1, 2, 0, 6, 1, 5, 2, 4, 1, 3]
    );
    for _ in 0..12 {
        mirror.next_u64();
    }
    assert_eq!(rng.next_u64(), mirror.next_u64());
}

#[test]
fn seeded_fixture_replays_indices_and_exact_rng_consumption() {
    let table = RewardTable::new([2, 0, 7, 1, 3]).unwrap();
    let mut a = RewardRng::new(42);
    let mut b = a;
    let mut mirror = a;
    let mut left = [99; 6];
    let mut right = left;
    let result = table.roll_distinct_into(&mut a, 4, &mut left, 16).unwrap();
    assert_eq!(result, report(4, 4, DistinctDrawStatus::Complete));
    assert_eq!(left, [3, 4, 0, 2, 99, 99]);
    assert_eq!(
        table.roll_distinct_into(&mut b, 4, &mut right, 16),
        Ok(result)
    );
    assert_eq!(left, right);
    for _ in 0..result.samples_used {
        mirror.next_u64();
    }
    assert_eq!(a.next_u64(), mirror.next_u64());
    assert_eq!(b.next_u64(), mirror_next_after(42, 4));
    // Every original ticket retains its original result after the new operation.
    assert_eq!(
        std::array::from_fn::<_, 13, _>(|t| table.choose(t as u32)),
        [0, 0, 2, 2, 2, 2, 2, 2, 2, 3, 4, 4, 4]
    );
}

fn mirror_next_after(seed: u64, count: usize) -> u64 {
    let mut rng = RewardRng::new(seed);
    for _ in 0..count {
        rng.next_u64();
    }
    rng.next_u64()
}

#[test]
fn preflight_errors_preserve_every_output_slot_and_rng() {
    let table = RewardTable::new([2, 3, 4]).unwrap();
    for (request, capacity, budget, error) in [
        (
            usize::MAX,
            0,
            usize::MAX,
            DistinctDrawError::RequestTooLarge {
                requested: usize::MAX,
            },
        ),
        (
            MAX_DISTINCT_DRAWS + 1,
            3,
            8,
            DistinctDrawError::RequestTooLarge {
                requested: MAX_DISTINCT_DRAWS + 1,
            },
        ),
        (
            3,
            2,
            usize::MAX,
            DistinctDrawError::OutputTooSmall {
                required: 3,
                provided: 2,
            },
        ),
        (
            1,
            0,
            0,
            DistinctDrawError::OutputTooSmall {
                required: 1,
                provided: 0,
            },
        ),
        (
            3,
            3,
            MAX_DISTINCT_SAMPLES + 1,
            DistinctDrawError::SampleBudgetTooLarge {
                sample_budget: MAX_DISTINCT_SAMPLES + 1,
            },
        ),
        (
            0,
            0,
            usize::MAX,
            DistinctDrawError::SampleBudgetTooLarge {
                sample_budget: usize::MAX,
            },
        ),
    ] {
        let mut out = [91, 92, 93, 94];
        let mut rng = RewardRng::new(3);
        assert_eq!(
            table.roll_distinct_into(&mut rng, request, &mut out[..capacity], budget),
            Err(error)
        );
        assert_eq!(out, [91, 92, 93, 94]);
        assert_eq!(rng.next_u64(), mirror_next_after(3, 0));
    }
    let mut out = [91, 92];
    assert_eq!(
        table.choose_distinct_into(&[], 3, &mut out),
        Err(DistinctDrawError::OutputTooSmall {
            required: 3,
            provided: 2,
        })
    );
    assert_eq!(out, [91, 92]);
    assert_eq!(
        table.choose_distinct_into(&[], 257, &mut []),
        Err(DistinctDrawError::RequestTooLarge { requested: 257 })
    );
}

#[test]
fn empty_requests_buffers_and_zero_budgets_are_explicit() {
    let table = RewardTable::new([1, 0, 3]).unwrap();
    let mut rng = RewardRng::new(42);
    let mut out = [7; 4];
    assert_eq!(
        table.roll_distinct_into(&mut rng, 0, &mut [], 0),
        Ok(report(0, 0, DistinctDrawStatus::Complete))
    );
    assert_eq!(
        table.roll_distinct_into(&mut rng, 0, &mut out, 9),
        Ok(report(0, 0, DistinctDrawStatus::Complete))
    );
    assert_eq!(
        table.roll_distinct_into(&mut rng, 4, &mut out, 0),
        Ok(report(0, 0, DistinctDrawStatus::SampleBudgetExhausted))
    );
    assert_eq!(out, [7; 4]);
    assert_eq!(rng.next_u64(), mirror_next_after(42, 0));
    assert_eq!(
        table.choose_distinct_into(&[], 0, &mut []),
        Ok(report(0, 0, DistinctDrawStatus::Complete))
    );
    assert_eq!(
        table.choose_distinct_into(&[], 2, &mut out),
        Ok(report(0, 0, DistinctDrawStatus::TicketsExhausted))
    );
    assert_eq!(out, [7; 4]);
}

#[test]
fn forced_rejection_and_budget_exhaustion_have_exact_prefixes_and_no_rollback() {
    // W = 2^31 + 1 rejects words below 0x7fff_ffff. Seed 3 produces
    // upper words 0x1d0b14e4 (reject), 0xb3466f8a (index 0), 0x9cebe8a6.
    let table = RewardTable::new([0x8000_0000, 1]).unwrap();
    let mut raw = RewardRng::new(3);
    assert_eq!(
        std::array::from_fn::<_, 3, _>(|_| (raw.next_u64() >> 32) as u32),
        [0x1d0b_14e4, 0xb346_6f8a, 0x9ceb_e8a6]
    );
    for (budget, prefix, expected) in [
        (
            0,
            [99, 99],
            report(0, 0, DistinctDrawStatus::SampleBudgetExhausted),
        ),
        (
            1,
            [99, 99],
            report(0, 1, DistinctDrawStatus::SampleBudgetExhausted),
        ),
        (
            2,
            [0, 99],
            report(1, 2, DistinctDrawStatus::SampleBudgetExhausted),
        ),
        (3, [0, 1], report(2, 3, DistinctDrawStatus::Complete)),
    ] {
        let mut rng = RewardRng::new(3);
        let mut out = [99; 2];
        assert_eq!(
            table.roll_distinct_into(&mut rng, 2, &mut out, budget),
            Ok(expected)
        );
        assert_eq!(out, prefix);
        assert_eq!(rng.next_u64(), mirror_next_after(3, expected.samples_used));
    }
}

#[test]
fn partial_calls_restart_the_pool_and_keep_rng_advancement() {
    let table = RewardTable::new([0x8000_0000, 1]).unwrap();
    let mut rng = RewardRng::new(3);
    let mut out = [99; 2];
    assert_eq!(
        table.roll_distinct_into(&mut rng, 2, &mut out, 2),
        Ok(report(1, 2, DistinctDrawStatus::SampleBudgetExhausted))
    );
    assert_eq!(out, [0, 99]);
    // No continuation token exists: the next call can offer the same index.
    assert_eq!(
        table.roll_distinct_into(&mut rng, 1, &mut out, 1),
        Ok(report(1, 1, DistinctDrawStatus::Complete))
    );
    assert_eq!(out, [0, 99]);
    assert_eq!(rng.next_u64(), mirror_next_after(3, 3));
    // Restart means reconstructing the run RNG, not rewinding it on exhaustion.
    let mut replay = RewardRng::new(3);
    assert_eq!(
        table.roll_distinct_into(&mut replay, 2, &mut out, 3),
        Ok(report(2, 3, DistinctDrawStatus::Complete))
    );
    assert_eq!(out, [0, 1]);
}

#[test]
fn exhaustion_capacity_and_tail_contracts_cover_sparse_and_all_positive_pools() {
    let sparse = RewardTable::new([0, 2, 0, 1]).unwrap();
    let mut out = [99; 2];
    // Two slots suffice for a request of four because only two can be offered.
    assert_eq!(
        sparse.choose_distinct_into(&[0, 0, u32::MAX], 4, &mut out),
        Ok(report(2, 2, DistinctDrawStatus::TableExhausted))
    );
    assert_eq!(out, [1, 3]);
    assert_eq!(
        sparse.roll_distinct_into(&mut RewardRng::new(42), 4, &mut out, 8),
        Ok(report(2, 2, DistinctDrawStatus::TableExhausted))
    );
    assert!(out.contains(&1) && out.contains(&3));
    let positive = RewardTable::new([1, 2, 3]).unwrap();
    let mut out = [99; 5];
    assert_eq!(
        positive.choose_distinct_into(&[0; 4], 4, &mut out),
        Ok(report(3, 3, DistinctDrawStatus::TableExhausted))
    );
    assert_eq!(out, [0, 1, 2, 99, 99]);
    assert_eq!(
        positive.choose_distinct_into(&[0; 3], 3, &mut out),
        Ok(report(3, 3, DistinctDrawStatus::Complete))
    );
    assert_eq!(
        positive.roll_distinct_into(&mut RewardRng::new(42), 4, &mut out, 3),
        Ok(report(3, 3, DistinctDrawStatus::TableExhausted))
    );
    assert_eq!(&out[3..], &[99; 2]);
}

#[test]
fn ticket_prefixes_stop_at_input_end_and_map_modulo_the_remaining_total() {
    let table = RewardTable::new([1, 0, 3]).unwrap();
    let mut out = [77; 4];
    assert_eq!(
        table.choose_distinct_into(&[u32::MAX], 3, &mut out),
        Ok(report(1, 1, DistinctDrawStatus::TicketsExhausted))
    );
    assert_eq!(out, [2, 77, 77, 77]);
    assert_eq!(
        table.choose_distinct_into(&[u32::MAX, u32::MAX], 3, &mut out),
        Ok(report(2, 2, DistinctDrawStatus::TableExhausted))
    );
    assert_eq!(out, [2, 0, 77, 77]);
}

#[test]
fn exact_ticket_domains_prove_sequential_weight_proportions_and_index_identity() {
    let table = RewardTable::new([1, 2, 0, 1]).unwrap();
    // Every first-range ticket, then every ticket in its remaining range.
    for (first_ticket, first, seconds) in [
        (0, 0, &[1, 1, 3][..]),
        (1, 1, &[0, 3][..]),
        (2, 1, &[0, 3][..]),
        (3, 3, &[0, 1, 1][..]),
    ] {
        for (second_ticket, &second) in seconds.iter().enumerate() {
            let mut out = [99; 2];
            assert_eq!(
                table.choose_distinct_into(&[first_ticket, second_ticket as u32], 2, &mut out),
                Ok(report(2, 2, DistinctDrawStatus::Complete))
            );
            assert_eq!(out, [first, second]);
            assert_ne!(out[0], out[1]);
        }
    }
    let values = [5, 5, 7];
    let mut out = [99; 3];
    let result = RewardTable::new([1; 3])
        .unwrap()
        .choose_distinct_into(&[0; 3], 3, &mut out)
        .unwrap();
    assert_eq!(result.written, 3);
    assert_eq!(out, [0, 1, 2]);
    assert_eq!(out.map(|id| values[id]), [5, 5, 7]);
}

#[test]
fn u32_max_total_and_single_positive_entries_do_not_overflow_or_skip_samples() {
    let table = RewardTable::new([u32::MAX - 1, 1]).unwrap();
    let mut out = [99; 3];
    assert_eq!(
        table.choose_distinct_into(&[u32::MAX - 1, u32::MAX], 2, &mut out),
        Ok(report(2, 2, DistinctDrawStatus::Complete))
    );
    assert_eq!(out, [1, 0, 99]);
    assert_eq!(
        table.roll_distinct_into(&mut RewardRng::new(0), 2, &mut out, 8),
        Ok(report(2, 2, DistinctDrawStatus::Complete))
    );
    assert_eq!(out, [0, 1, 99]);
    for table in [
        RewardTable::new([0, u32::MAX, 0]).unwrap(),
        RewardTable::new([0, 1, 0]).unwrap(),
    ] {
        let mut rng = RewardRng::new(7);
        assert_eq!(
            table.roll_distinct_into(&mut rng, 3, &mut out[..1], 1),
            Ok(report(1, 1, DistinctDrawStatus::TableExhausted))
        );
        assert_eq!(out, [1, 1, 99]);
        assert_eq!(rng.next_u64(), mirror_next_after(7, 1));
    }
}

#[test]
fn many_seeds_exclude_zeros_and_repeat_no_index() {
    let weights = [0, 1, 7, 0, 100, 2, 50, 0];
    let table = RewardTable::new(weights).unwrap();
    for seed in 0..128 {
        let mut rng = RewardRng::new(seed);
        let mut out = [99; 9];
        let draw = table.roll_distinct_into(&mut rng, 8, &mut out, 32).unwrap();
        assert_eq!(draw.status, DistinctDrawStatus::TableExhausted);
        assert_eq!(draw.written, 5);
        assert!((5..=32).contains(&draw.samples_used));
        for (position, &index) in out[..draw.written].iter().enumerate() {
            assert!(weights[index] > 0);
            assert!(!out[..position].contains(&index));
        }
        assert_eq!(&out[draw.written..], &[99; 4]);
        assert_eq!(rng.next_u64(), mirror_next_after(seed, draw.samples_used));
    }
}

#[test]
fn maximum_request_and_sample_budget_are_bounded_and_allocation_free() {
    let table = RewardTable::new([1; MAX_DISTINCT_DRAWS]).unwrap();
    let mut rng = RewardRng::new(42);
    let mut out = [usize::MAX; MAX_DISTINCT_DRAWS + 2];
    let (result, allocations) = allocations_during(|| {
        black_box(&table).roll_distinct_into(
            black_box(&mut rng),
            MAX_DISTINCT_DRAWS,
            black_box(&mut out),
            MAX_DISTINCT_SAMPLES,
        )
    });
    let result = result.unwrap();
    assert_eq!(allocations, 0);
    assert_eq!(result.status, DistinctDrawStatus::Complete);
    assert_eq!(result.written, MAX_DISTINCT_DRAWS);
    assert!((MAX_DISTINCT_DRAWS..=MAX_DISTINCT_SAMPLES).contains(&result.samples_used));
    let mut seen = [false; MAX_DISTINCT_DRAWS];
    for &index in &out[..result.written] {
        assert!(!seen[index]);
        seen[index] = true;
    }
    assert!(seen.into_iter().all(|present| present));
    assert_eq!(&out[MAX_DISTINCT_DRAWS..], &[usize::MAX; 2]);
    let tickets = [u32::MAX; MAX_DISTINCT_DRAWS];
    let (result, allocations) = allocations_during(|| {
        black_box(&table).choose_distinct_into(
            black_box(&tickets),
            MAX_DISTINCT_DRAWS,
            black_box(&mut out),
        )
    });
    assert_eq!(allocations, 0);
    assert_eq!(result, Ok(report(256, 256, DistinctDrawStatus::Complete)));
    let mut sorted = out[..256].to_vec();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..256).collect::<Vec<_>>());
}

#[test]
fn allocation_counter_is_live_and_error_and_partial_paths_also_do_not_allocate() {
    let (allocated, count) = allocations_during(|| black_box(Box::new(black_box(7_u64))));
    assert!(count > 0);
    drop(allocated);
    let table = RewardTable::new([0x8000_0000, 1]).unwrap();
    let mut out = [99; 2];
    let mut rng = RewardRng::new(3);
    let (partial, count) = allocations_during(|| {
        black_box(&table).roll_distinct_into(black_box(&mut rng), 2, black_box(&mut out), 2)
    });
    assert_eq!(count, 0);
    assert_eq!(
        partial,
        Ok(report(1, 2, DistinctDrawStatus::SampleBudgetExhausted))
    );
    let (error, count) = allocations_during(|| {
        black_box(&table).roll_distinct_into(black_box(&mut rng), 2, black_box(&mut []), 8)
    });
    assert_eq!(count, 0);
    assert_eq!(
        error,
        Err(DistinctDrawError::OutputTooSmall {
            required: 2,
            provided: 0
        })
    );
}

#[test]
fn authored_table_validation_is_preserved() {
    for invalid in [
        RewardTable::new([0; 2]).map(|_| ()),
        RewardTable::new([u32::MAX, 1]).map(|_| ()),
        RewardTable::new([]).map(|_| ()),
        RewardTable::new([1; 257]).map(|_| ()),
    ] {
        assert_eq!(invalid, Err(LoopError::Invalid));
    }
    assert!(RewardTable::new([u32::MAX]).is_ok());
    assert!(RewardTable::new([1; 256]).is_ok());
}

#[test]
fn accepted_wave_rewards_earn_offers_once_and_filter_capped_upgrades() {
    let mut waves = WaveSequence::new(
        [WaveSpec {
            credits: 1,
            interval: 0.125,
            rest: 0.0,
            reward: 10,
        }; 2],
        1,
    )
    .unwrap();
    let mut progression = Progression::new(10, 0, 4).unwrap();
    let caps = [1, 1, 1];
    let mut upgrades = Upgrades::new(caps).unwrap();
    let authored = [3, 2, 1];
    let mut rng = RewardRng::new(7);
    let mut points = 0;
    let mut selected = [usize::MAX; 2];
    for chosen_slot in &mut selected {
        assert!(waves.admit(1, 0));
        assert_eq!(waves.advance(0.125, 0.0, 1).unwrap(), None);
        let clear = waves.advance(0.125, 0.0, 0).unwrap().unwrap();
        points += progression.award(u64::from(clear.reward));
        let weights: [u32; 3] = std::array::from_fn(|id| {
            if upgrades.rank(id).unwrap() < caps[id] {
                authored[id]
            } else {
                0
            }
        });
        let table = RewardTable::new(weights).unwrap();
        let mut offers = [usize::MAX; 3];
        let draw = table
            .roll_distinct_into(&mut rng, 3, &mut offers, 16)
            .unwrap();
        let displayed = &offers[..draw.written];
        for &id in displayed {
            assert!(upgrades.rank(id).unwrap() < caps[id]);
        }
        let chosen = displayed[0]; // Deterministic headless selection policy.
        assert!(points > 0 && displayed.contains(&chosen));
        assert!(upgrades.rank(chosen).unwrap() < caps[chosen]);
        points -= 1;
        upgrades.apply(chosen).unwrap();
        *chosen_slot = chosen;
        // Rest/finished updates do not re-emit the accepted clear or earn points.
        assert_eq!(waves.advance(0.125, 0.0, 0).unwrap(), None);
    }
    assert!(waves.finished());
    assert_ne!(selected[0], selected[1]);
    assert_eq!((progression.level(), points), (3, 0));
    assert_eq!(upgrades.rank(selected[0]), Some(1));
    assert_eq!(upgrades.rank(selected[1]), Some(1));
}
