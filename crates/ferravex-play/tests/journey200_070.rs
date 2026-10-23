use ferravex_math::Vec3;
use ferravex_play::ai::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

fn candidate(id: u64) -> Candidate {
    Candidate {
        id,
        position: Vec3::Z,
        groups: 1,
    }
}

fn observer() -> Observer {
    Observer {
        position: Vec3::ZERO,
        forward: Vec3::Z,
    }
}

fn sight() -> Sight {
    Sight::new(10., std::f32::consts::FRAC_PI_2, 1).unwrap()
}

fn select(
    selector: &mut TargetSelector,
    candidates: impl IntoIterator<Item = Candidate>,
    priority: impl FnMut(Candidate) -> f32,
) -> TargetSelectionReport<Candidate> {
    selector
        .select_visible(
            candidates,
            observer(),
            sight(),
            &mut WorkBudget::new(16, 16, 0),
            &mut Unoccluded,
            priority,
        )
        .unwrap()
}

#[test]
fn margins_are_checked_and_reset_reacquires_by_score_then_id() {
    for margin in [-1., f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
        assert!(matches!(
            TargetSelector::new(margin),
            Err(AiError::InvalidConfiguration)
        ));
    }
    let mut selector = TargetSelector::new(0.).unwrap();
    let first = select(&mut selector, [candidate(9)], |_| -10.);
    assert_eq!(first.reason, TargetSelectionReason::Acquired);
    assert_eq!(first.previous, None);
    assert_eq!(first.previous_score, None);
    let tied = select(&mut selector, [candidate(2), candidate(9)], |_| -10.);
    assert_eq!(tied.target.unwrap().id, 9);
    assert_eq!(tied.best, Some((2, -10.)));
    assert_eq!(tied.reason, TargetSelectionReason::RetainedByHysteresis);
    let better = select(&mut selector, [candidate(9), candidate(2)], |c| {
        if c.id == 2 { -9.5 } else { -10. }
    });
    assert_eq!(better.target.unwrap().id, 2);
    assert_eq!(better.reason, TargetSelectionReason::SwitchMarginExceeded);
    selector.reset();
    assert_eq!(selector.current(), None);
    let acquired = select(&mut selector, [candidate(9), candidate(2)], |_| -10.);
    assert_eq!(acquired.target.unwrap().id, 2);
    assert_eq!(acquired.reason, TargetSelectionReason::Acquired);
    let empty = select(&mut selector, [], |_| unreachable!());
    assert_eq!(empty.previous, Some(2));
    assert_eq!(empty.reason, TargetSelectionReason::NoEligibleTarget);
    assert!(empty.target.is_none());
    assert!(empty.best.is_none());
    assert!(empty.selected_score.is_none());
    assert!(!empty.incomplete);
    assert_eq!(selector.current(), None);
}

#[test]
fn oscillating_scores_retain_current_until_margin_is_strictly_exceeded() {
    let mut selector = TargetSelector::new(0.25).unwrap();
    select(&mut selector, [candidate(9)], |_| 1.);
    for tick in 0..32 {
        let challenger_score = if tick % 2 == 0 { 1.125 } else { 0.875 };
        let report = select(&mut selector, [candidate(2), candidate(9)], |c| {
            if c.id == 2 { challenger_score } else { 1. }
        });
        assert_eq!(report.target.unwrap().id, 9);
        assert_eq!(report.previous, Some(9));
        assert_eq!(report.previous_score, Some(1.));
        assert_eq!(report.selected_score, Some(1.));
        assert_eq!(
            report.reason,
            if tick % 2 == 0 {
                TargetSelectionReason::RetainedByHysteresis
            } else {
                TargetSelectionReason::RetainedBest
            }
        );
    }
    let boundary = select(&mut selector, [candidate(9), candidate(2)], |c| {
        if c.id == 2 { 1.25 } else { 1. }
    });
    assert_eq!(boundary.target.unwrap().id, 9);
    assert_eq!(boundary.reason, TargetSelectionReason::RetainedByHysteresis);
    let switched = select(&mut selector, [candidate(2), candidate(9)], |c| {
        if c.id == 2 { 1.5 } else { 1. }
    });
    assert_eq!(switched.target.unwrap().id, 2);
    assert_eq!(switched.reason, TargetSelectionReason::SwitchMarginExceeded);
    assert_eq!(selector.current(), Some(2));
}

#[test]
fn finite_extreme_scores_do_not_overflow_margin_comparison() {
    let mut selector = TargetSelector::new(f32::MAX).unwrap();
    select(&mut selector, [candidate(9)], |_| -f32::MAX);
    let held = select(&mut selector, [candidate(9), candidate(2)], |c| {
        if c.id == 9 { -f32::MAX } else { -1. }
    });
    assert_eq!(held.target.unwrap().id, 9);
    let switched = select(&mut selector, [candidate(9), candidate(2)], |c| {
        if c.id == 9 { -f32::MAX } else { f32::MAX }
    });
    assert_eq!(switched.target.unwrap().id, 2);
    assert_eq!(switched.reason, TargetSelectionReason::SwitchMarginExceeded);
}

#[test]
fn masks_range_cone_occlusion_and_nonfinite_scores_defeat_retention() {
    let excluded = [
        Candidate {
            groups: 2,
            ..candidate(9)
        },
        Candidate {
            position: Vec3::Z * 11.,
            ..candidate(9)
        },
        Candidate {
            position: -Vec3::Z,
            ..candidate(9)
        },
    ];
    for previous in excluded {
        let mut selector = TargetSelector::new(f32::MAX).unwrap();
        select(&mut selector, [candidate(9)], |_| 100.);
        let report = select(&mut selector, [previous, candidate(2)], |c| {
            assert_eq!(c.id, 2, "rejected candidate must not be scored");
            -100.
        });
        assert_eq!(report.target.unwrap().id, 2);
        assert_eq!(report.reason, TargetSelectionReason::PreviousIneligible);
        assert_eq!(report.previous_score, None);
    }
    struct Wall;
    impl VisibilityQuery for Wall {
        fn clear(&mut self, _: Vec3, to: Vec3) -> bool {
            to.z > 1.
        }
    }
    let mut selector = TargetSelector::new(f32::MAX).unwrap();
    select(&mut selector, [candidate(9)], |_| 100.);
    let report = selector
        .select_visible(
            [
                candidate(9),
                Candidate {
                    position: Vec3::Z * 2.,
                    ..candidate(2)
                },
            ],
            observer(),
            sight(),
            &mut WorkBudget::new(2, 2, 0),
            &mut Wall,
            |c| {
                assert_eq!(c.id, 2);
                -100.
            },
        )
        .unwrap();
    assert_eq!(report.target.unwrap().id, 2);
    assert_eq!(report.reason, TargetSelectionReason::PreviousIneligible);
    for score in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        select(&mut selector, [candidate(9)], |_| 100.);
        let report = select(&mut selector, [candidate(9), candidate(2)], |c| {
            if c.id == 9 { score } else { -100. }
        });
        assert_eq!(report.target.unwrap().id, 2);
        assert_eq!(report.reason, TargetSelectionReason::PreviousIneligible);
        assert_eq!(report.previous_score, None);
    }
}

#[test]
fn deferred_sight_never_returns_or_scores_an_unverified_incumbent() {
    for (candidates, queries) in [(0, 1), (1, 0)] {
        let mut selector = TargetSelector::new(100.).unwrap();
        select(&mut selector, [candidate(9)], |_| 100.);
        let mut budget = WorkBudget::new(candidates, queries, 0);
        let report = selector
            .select_visible(
                [candidate(9)],
                observer(),
                sight(),
                &mut budget,
                &mut Unoccluded,
                |_| panic!("deferred target was scored"),
            )
            .unwrap();
        assert!(report.incomplete);
        assert!(report.target.is_none());
        assert_eq!(report.previous, Some(9));
        assert_eq!(report.reason, TargetSelectionReason::NoEligibleTarget);
        assert_eq!(selector.current(), None);
        assert_eq!(budget.counts().queries, 0);
    }
    let mut selector = TargetSelector::new(100.).unwrap();
    select(&mut selector, [candidate(9)], |_| 100.);
    let report = selector
        .select_visible(
            [candidate(2), candidate(9)],
            observer(),
            sight(),
            &mut WorkBudget::new(2, 1, 0),
            &mut Unoccluded,
            |c| {
                assert_eq!(c.id, 2);
                -100.
            },
        )
        .unwrap();
    assert!(report.incomplete);
    assert_eq!(report.target.unwrap().id, 2);
    assert_eq!(report.previous_score, None);
    assert_eq!(report.reason, TargetSelectionReason::PreviousIneligible);
}

#[test]
fn unbounded_input_stops_at_budget_and_duplicate_ids_use_their_best_score() {
    let mut selector = TargetSelector::new(1.).unwrap();
    select(&mut selector, [candidate(9)], |_| 0.);
    let mut pulled = 0;
    let mut scored = 0;
    let mut budget = WorkBudget::new(3, 3, 0);
    let report = selector
        .select_visible(
            std::iter::repeat(candidate(9)).inspect(|_| pulled += 1),
            observer(),
            sight(),
            &mut budget,
            &mut Unoccluded,
            |_| {
                scored += 1;
                if scored == 2 { 10. } else { 1. }
            },
        )
        .unwrap();
    assert_eq!(pulled, 4);
    assert_eq!(scored, 3);
    assert_eq!(budget.counts().candidates, 3);
    assert_eq!(budget.counts().queries, 3);
    assert_eq!(budget.counts().deferred_candidates, 1);
    assert!(report.incomplete);
    assert_eq!(report.selected_score, Some(10.));
    assert_eq!(report.previous_score, Some(10.));
    assert_eq!(report.reason, TargetSelectionReason::RetainedBest);
}

#[test]
fn selection_errors_preserve_identity_but_not_already_consumed_work() {
    let mut selector = TargetSelector::new(1.).unwrap();
    select(&mut selector, [candidate(9)], |_| 0.);
    let mut budget = WorkBudget::new(3, 3, 0);
    let mut scored = 0;
    let result = selector.select_visible(
        [
            candidate(2),
            Candidate {
                position: Vec3::splat(f32::NAN),
                ..candidate(3)
            },
        ],
        observer(),
        sight(),
        &mut budget,
        &mut Unoccluded,
        |_| {
            scored += 1;
            100.
        },
    );
    assert!(matches!(result, Err(AiError::InvalidInput)));
    assert_eq!(selector.current(), Some(9));
    assert_eq!(budget.counts().queries, 1);
    assert_eq!(scored, 1);
}

#[test]
fn memory_selection_preserves_lifetime_and_prioritizes_fresh_sight() {
    let mut memory = Memory::new(MemorySettings {
        sight_seconds: 1.,
        hearing_seconds: 2.,
        ..Default::default()
    })
    .unwrap();
    let mut selector = TargetSelector::new(f32::MAX).unwrap();
    memory.observe(candidate(9), Sense::Sight).unwrap();
    selector.select_memory(&memory, 1, |_| 100.);
    memory.advance(0.25).unwrap();
    memory.observe(candidate(2), Sense::Hearing).unwrap();
    let held = selector.select_memory(&memory, 1, |t| if t.id == 2 { 101. } else { 100. });
    let track = held.target.unwrap();
    assert_eq!(track.id, 9);
    assert!(!track.visible);
    assert_eq!(track.age, 0.25);
    assert_eq!(track.remaining(), 0.75);
    assert_eq!(held.reason, TargetSelectionReason::RetainedByHysteresis);
    memory.observe(candidate(2), Sense::Sight).unwrap();
    let visible = selector.select_memory(&memory, 1, |t| if t.id == 2 { -100. } else { 100. });
    assert_eq!(visible.target.unwrap().id, 2);
    assert!(visible.target.unwrap().visible);
    assert_eq!(visible.reason, TargetSelectionReason::SwitchedToVisible);
    assert_eq!(memory.get(9).unwrap().remaining(), 0.75);
    assert_eq!(memory.get(9).unwrap().age, 0.25);
    assert!(!visible.incomplete);
    memory.advance(0.75).unwrap();
    assert!(memory.get(9).is_none());
    memory.advance(0.25).unwrap();
    let expired = selector.select_memory(&memory, 1, |_| panic!("expired track scored"));
    assert!(expired.target.is_none());
    assert_eq!(expired.reason, TargetSelectionReason::NoEligibleTarget);
    assert_eq!(selector.current(), None);
}

#[test]
fn expired_masked_forgotten_and_nonfinite_memory_cannot_be_retained() {
    let mut memory = Memory::new(MemorySettings {
        sight_seconds: 0.5,
        hearing_seconds: 1.,
        ..Default::default()
    })
    .unwrap();
    let mut selector = TargetSelector::new(1000.).unwrap();
    memory.observe(candidate(9), Sense::Sight).unwrap();
    selector.select_memory(&memory, 1, |_| 100.);
    memory.observe(candidate(2), Sense::Hearing).unwrap();
    memory.advance(0.5).unwrap();
    let report = selector.select_memory(&memory, 1, |_| -100.);
    assert_eq!(report.target.unwrap().id, 2);
    assert!(!report.target.unwrap().visible);
    assert_eq!(report.reason, TargetSelectionReason::PreviousIneligible);
    memory
        .observe(
            Candidate {
                groups: 2,
                ..candidate(2)
            },
            Sense::Hearing,
        )
        .unwrap();
    memory.observe(candidate(3), Sense::Hearing).unwrap();
    let masked = selector.select_memory(&memory, 1, |t| {
        assert_eq!(t.id, 3);
        -100.
    });
    assert_eq!(masked.target.unwrap().id, 3);
    assert_eq!(masked.reason, TargetSelectionReason::PreviousIneligible);
    memory.forget(3);
    let forgotten = selector.select_memory(&memory, 3, |_| -200.);
    assert_eq!(forgotten.target.unwrap().id, 2);
    assert_eq!(forgotten.reason, TargetSelectionReason::PreviousIneligible);
    let invalid_score = selector.select_memory(&memory, 3, |_| f32::NAN);
    assert!(invalid_score.target.is_none());
    assert_eq!(selector.current(), None);
    let empty_mask = selector.select_memory(&memory, 0, |_| unreachable!());
    assert!(empty_mask.target.is_none());
    memory.clear();
    assert!(selector.select_memory(&memory, 3, |_| 1.).target.is_none());
}

#[test]
fn legacy_selection_keeps_id_ties_nonfinite_filtering_and_work_counts() {
    let mut budget = WorkBudget::new(4, 4, 0);
    let result = select_target(
        [candidate(9), candidate(2), candidate(1)],
        observer(),
        sight(),
        &mut budget,
        &mut Unoccluded,
        |c| if c.id == 1 { f32::INFINITY } else { -1. },
    )
    .unwrap();
    assert_eq!(result.target.unwrap().id, 2);
    assert!(!result.incomplete);
    assert_eq!(budget.counts().candidates, 3);
    assert_eq!(budget.counts().queries, 3);
    let mut memory = Memory::new(MemorySettings::default()).unwrap();
    memory.observe(candidate(9), Sense::Sight).unwrap();
    memory.advance(0.25).unwrap();
    memory.observe(candidate(2), Sense::Hearing).unwrap();
    assert_eq!(memory.best(1).unwrap().id, 2);
    memory.observe(candidate(9), Sense::Sight).unwrap();
    assert_eq!(memory.best(1).unwrap().id, 9);
}

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct CountingAllocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = RECORD.try_with(|record| {
            if record.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        });
    }
}
// SAFETY: Allocation contracts and pointers are forwarded unchanged to System.
// Thread-local counters do not allocate and tolerate thread-local destruction.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        allocated(pointer);
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        allocated(pointer);
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        allocated(pointer);
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn new_selection_paths_allocate_nothing_after_memory_construction() {
    let mut selector = TargetSelector::new(0.25).unwrap();
    let mut memory = Memory::new(MemorySettings::default()).unwrap();
    memory.observe(candidate(9), Sense::Sight).unwrap();
    memory.observe(candidate(2), Sense::Hearing).unwrap();
    ALLOCATIONS.with(|count| count.set(0));
    RECORD.with(|record| record.set(true));
    for _ in 0..128 {
        let visible = select(&mut selector, [candidate(9), candidate(2)], |_| 1.);
        std::hint::black_box(visible);
        let remembered = selector.select_memory(&memory, 1, |t| -(t.age as f32));
        std::hint::black_box(remembered);
    }
    RECORD.with(|record| record.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
}
