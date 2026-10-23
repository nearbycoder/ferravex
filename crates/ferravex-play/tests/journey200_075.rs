use ferravex_play::particles::{
    RibbonFacing, RibbonSource, RibbonTriangles, Trail, TrailConfig, TrailError, TrailSample,
    TrailStats,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

struct CountingAllocator;

fn record_allocation(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = ALLOCATIONS.try_with(|count| {
            if let Some(current) = count.get() {
                count.set(Some(current + 1));
            }
        });
    }
}

// SAFETY: Every operation forwards the original layout and pointer to System.
// Recording uses only thread-local cells and ignores TLS destruction failures.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        record_allocation(pointer);
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record_allocation(pointer);
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        record_allocation(pointer);
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn sample(x: f32) -> TrailSample {
    TrailSample {
        position: [x, 0., 0.],
        color: [1.; 4],
        width: 2.,
    }
}

fn trail(capacity: usize) -> Trail {
    Trail::new(TrailConfig {
        capacity,
        lifetime: 2.,
        spacing: 1.,
        teleport_distance: 8.,
        ..Default::default()
    })
    .unwrap()
}

fn positions(trail: &Trail) -> Vec<f32> {
    trail.iter().map(|p| p.position[0]).collect()
}

fn assert_no_bridge(trail: &Trail, old_end: f32, new_start: f32) {
    let triangles = RibbonTriangles::new(
        trail,
        RibbonFacing::Plane {
            normal: [0., 0., 1.],
        },
        2.,
    )
    .unwrap();
    let mut count = 0;
    for triangle in triangles {
        let xs = triangle.vertices.map(|v| v.position[0]);
        assert!(xs.iter().all(|x| *x <= old_end) || xs.iter().all(|x| *x >= new_start));
        count += 1;
    }
    assert!(count > 0, "the old segment still produces geometry");
}

#[test]
fn break_segment_preserves_tip_and_separates_short_resumes() {
    let mut trail = trail(8);
    trail.update(0., Some(sample(0.))).unwrap();
    trail.update(0.25, Some(sample(0.75))).unwrap();
    let before = trail.iter().collect::<Vec<_>>();
    assert_eq!(trail.break_segment().sampled, 1);
    assert_eq!(trail.iter().collect::<Vec<_>>(), before);
    assert_eq!(trail.break_segment(), TrailStats::default());

    trail.update(0.25, Some(sample(0.875))).unwrap();
    trail.update(0., Some(sample(1.375))).unwrap();
    assert_eq!(positions(&trail), [0., 0.75, 0.875, 1.375]);
    assert_eq!(
        trail.iter().map(|p| p.break_before).collect::<Vec<_>>(),
        [true, false, true, false]
    );
    assert_eq!(trail.stats().teleports, 0);
    assert_no_bridge(&trail, 0.75, 0.875);
}

#[test]
fn teleport_preserves_tip_resets_spacing_and_breaks_short_jumps() {
    let mut trail = trail(8);
    trail.update(0., Some(sample(0.))).unwrap();
    trail.update(0.25, Some(sample(0.75))).unwrap();
    let before = trail.iter().collect::<Vec<_>>();
    assert_eq!(
        trail.teleport(sample(0.875)).unwrap(),
        TrailStats {
            sampled: 2,
            teleports: 1,
            ..Default::default()
        }
    );
    assert_eq!(trail.iter().take(2).collect::<Vec<_>>(), before);
    assert!(trail.point(2).unwrap().break_before);
    let stats = trail.update(0.25, Some(sample(1.375))).unwrap();
    assert_eq!(stats.sampled, 0, "old spacing carry must be discarded");
    assert_eq!(positions(&trail), [0., 0.75, 0.875, 1.375]);
    assert_eq!(trail.stats().teleports, 1);
    assert_no_bridge(&trail, 0.75, 0.875);
}

#[test]
fn teleport_at_same_position_starts_a_segment_even_when_empty() {
    let mut trail = trail(8);
    assert_eq!(trail.teleport(sample(0.)).unwrap().sampled, 1);
    assert_eq!(trail.teleport(sample(0.)).unwrap().sampled, 1);
    assert_eq!(positions(&trail), [0., 0.]);
    assert!(trail.iter().all(|p| p.break_before));
    assert_eq!(trail.stats().teleports, 2);
    trail.update(0.25, Some(sample(0.5))).unwrap();
    assert!(!trail.iter().last().unwrap().break_before);
}

#[test]
fn automatic_teleport_preserves_tip_with_elapsed_age() {
    let mut trail = trail(8);
    trail.update(0., Some(sample(0.))).unwrap();
    trail.update(0.25, Some(sample(0.75))).unwrap();
    let stats = trail.update(0.5, Some(sample(20.))).unwrap();
    assert_eq!(stats.sampled, 2);
    assert_eq!(stats.teleports, 1);
    assert_eq!(positions(&trail), [0., 0.75, 20.]);
    let points = trail.iter().collect::<Vec<_>>();
    assert_eq!(points[0].color[3], 0.625);
    assert_eq!(points[1].color[3], 0.75);
    assert_eq!(points[1].width, 1.5);
    assert_eq!(points[2].color[3], 1.);
    assert!(points[2].break_before);
    assert_no_bridge(&trail, 0.75, 20.);
}

#[test]
fn automatic_teleport_does_not_revive_expired_tip() {
    let mut trail = trail(8);
    trail.update(0., Some(sample(0.))).unwrap();
    trail.update(0.25, Some(sample(0.75))).unwrap();
    let stats = trail.update(2., Some(sample(20.))).unwrap();
    assert_eq!(stats.expired, 2);
    assert_eq!(stats.sampled, 1);
    assert_eq!(stats.teleports, 1);
    assert_eq!(positions(&trail), [20.]);
    assert!(trail.point(0).unwrap().break_before);
    trail.update(2., None).unwrap();
    assert!(trail.is_empty());
}

#[test]
fn breaks_and_stops_preserve_original_expiration_times() {
    for fade_width in [false, true] {
        let mut trail = Trail::new(TrailConfig {
            lifetime: 1.,
            spacing: 1.,
            fade_width,
            ..Default::default()
        })
        .unwrap();
        trail.update(0., Some(sample(0.))).unwrap();
        trail.update(0.25, Some(sample(0.75))).unwrap();
        trail.break_segment();
        trail.update(0.5, None).unwrap();
        let before = trail.iter().collect::<Vec<_>>();
        for _ in 0..32 {
            assert_eq!(trail.break_segment(), TrailStats::default());
        }
        assert_eq!(trail.iter().collect::<Vec<_>>(), before);
        assert_eq!(trail.update(0.25, None).unwrap().expired, 1);
        let tip = trail.point(0).unwrap();
        assert_eq!(tip.position, sample(0.75).position);
        assert_eq!(tip.color[3], 0.25);
        assert_eq!(tip.width, if fade_width { 0.5 } else { 2. });
        assert_eq!(trail.update(0.25, None).unwrap().expired, 1);
        assert!(trail.is_empty());
    }
}

#[test]
fn stopping_with_elapsed_time_matches_break_then_aging() {
    let mut stopped = trail(8);
    let mut broken = trail(8);
    for trail in [&mut stopped, &mut broken] {
        trail.update(0., Some(sample(0.))).unwrap();
        trail.update(0.25, Some(sample(0.75))).unwrap();
    }
    stopped.update(0.5, None).unwrap();
    broken.break_segment();
    broken.update(0.5, None).unwrap();
    assert_eq!(
        stopped.iter().collect::<Vec<_>>(),
        broken.iter().collect::<Vec<_>>()
    );
    assert_eq!(stopped.stats(), broken.stats());
    for trail in [&mut stopped, &mut broken] {
        trail.update(0., Some(sample(0.875))).unwrap();
        assert!(trail.iter().last().unwrap().break_before);
    }
}

#[test]
fn repeated_discontinuities_wrap_in_order_with_bounded_point_counts() {
    for capacity in [2, 3, 8] {
        let mut trail = trail(capacity);
        let mut expected = Vec::new();
        for i in 0..256 {
            let base = i as f32 * 2.;
            trail.teleport(sample(base)).unwrap();
            expected.push((base, true));
            if expected.len() > capacity {
                expected.remove(0);
            }
            assert_eq!(
                trail
                    .iter()
                    .map(|p| (p.position[0], p.break_before))
                    .collect::<Vec<_>>(),
                expected
            );
            trail.update(0., Some(sample(base + 0.5))).unwrap();
            expected.push((base + 0.5, false));
            if expected.len() > capacity {
                expected.remove(0);
            }
            let before = trail.iter().collect::<Vec<_>>();
            assert_eq!(trail.break_segment().sampled, 1);
            assert_eq!(trail.iter().collect::<Vec<_>>(), before);
            assert_eq!(trail.break_segment(), TrailStats::default());
            assert_eq!(
                trail
                    .iter()
                    .map(|p| (p.position[0], p.break_before))
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(trail.len() <= capacity);
            assert_eq!(trail.iter().len(), trail.len());
            assert!(trail.point(trail.len()).is_none());
        }
        assert_eq!(trail.stats().teleports, 256);
        assert_eq!(trail.stats().sampled, 512);
        assert_eq!(trail.stats().overwritten, 512 - capacity as u64);
        assert_eq!(trail.update(2., None).unwrap().expired, capacity as u64);
        assert!(trail.is_empty());
    }
}

#[test]
fn discontinuities_and_resets_reuse_preallocated_storage() {
    let mut trail = trail(8);
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for i in 0..1024 {
        let x = i as f32;
        trail.teleport(sample(x)).unwrap();
        trail.update(0.25, Some(sample(x + 0.5))).unwrap();
        trail.break_segment();
        trail.break_segment();
        trail.update(0.25, None).unwrap();
        trail.update(0., Some(sample(x + 0.75))).unwrap();
        trail.update(0.25, Some(sample(x + 20.))).unwrap();
        assert!(trail.len() <= trail.capacity());
        assert!(trail.iter().all(|point| point.valid()));
        if i % 11 == 0 {
            trail.clear();
            trail.reset_stats();
        }
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
}

#[test]
fn clear_resets_all_history_but_not_stats() {
    let mut trail = trail(2);
    let config = trail.config();
    trail.teleport(sample(0.)).unwrap();
    trail.update(0.25, Some(sample(0.75))).unwrap();
    let stats = trail.stats();
    trail.clear();
    trail.clear();
    assert!(trail.is_empty());
    assert_eq!(trail.config(), config);
    assert_eq!(trail.stats(), stats);
    assert_eq!(trail.break_segment(), TrailStats::default());
    let first = trail.update(0., Some(sample(100.))).unwrap();
    assert_eq!(first.sampled, 1);
    assert_eq!(first.teleports, 0);
    assert!(trail.point(0).unwrap().break_before);
    assert_eq!(trail.update(0., Some(sample(100.5))).unwrap().sampled, 0);
    assert_eq!(positions(&trail), [100., 100.5]);
    trail.reset_stats();
    assert_eq!(trail.stats(), TrailStats::default());
    assert_eq!(positions(&trail), [100., 100.5]);
}

#[test]
fn invalid_discontinuity_inputs_leave_state_unchanged() {
    let mut trail = trail(8);
    trail.update(0., Some(sample(0.))).unwrap();
    trail.update(0.25, Some(sample(0.75))).unwrap();
    let before = trail.iter().collect::<Vec<_>>();
    let stats = trail.stats();
    for invalid in [
        sample(f32::NAN),
        sample(f32::INFINITY),
        TrailSample {
            color: [f32::NAN; 4],
            ..sample(1.)
        },
        TrailSample {
            width: -1.,
            ..sample(1.)
        },
        TrailSample {
            width: f32::INFINITY,
            ..sample(1.)
        },
        TrailSample {
            width: 1_000_001.,
            ..sample(1.)
        },
    ] {
        assert_eq!(trail.teleport(invalid), Err(TrailError::InvalidSample));
        assert_eq!(
            trail.update(0.5, Some(invalid)),
            Err(TrailError::InvalidSample)
        );
        assert_eq!(trail.iter().collect::<Vec<_>>(), before);
        assert_eq!(trail.stats(), stats);
    }
    for dt in [-1., f32::NAN, f32::INFINITY, 61.] {
        assert_eq!(
            trail.update(dt, Some(sample(20.))),
            Err(TrailError::InvalidDelta)
        );
        assert_eq!(trail.iter().collect::<Vec<_>>(), before);
        assert_eq!(trail.stats(), stats);
    }
    // A rejected teleport must not drop the accumulated spacing either.
    assert_eq!(trail.update(0., Some(sample(1.))).unwrap().sampled, 1);
}

#[test]
fn sampling_budget_drops_stay_disconnected_after_teleport() {
    let mut trail = Trail::new(TrailConfig {
        capacity: 8,
        lifetime: 2.,
        spacing: 0.25,
        max_per_update: 1,
        ..Default::default()
    })
    .unwrap();
    trail.update(0., Some(sample(0.))).unwrap();
    assert_eq!(
        trail
            .update(0.25, Some(sample(1.125)))
            .unwrap()
            .dropped_budget,
        3
    );
    assert!(trail.point(1).unwrap().break_before);
    let stats = trail.teleport(sample(1.25)).unwrap();
    assert_eq!(stats.dropped_budget, 0);
    assert_eq!(stats.sampled, 2);
    assert_eq!(trail.update(0., Some(sample(1.375))).unwrap().sampled, 0);
    assert_eq!(positions(&trail), [0., 1., 1.125, 1.25, 1.375]);
    assert_no_bridge(&trail, 1.125, 1.25);
}
