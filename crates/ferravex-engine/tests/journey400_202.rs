//! Allocation checks through the registered production interpolation module.
use ferravex_engine::{
    interpolation::{InterpolationError, InterpolationPair},
    math::{Quat, Vec2, Vec3},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

fn count_allocation() {
    let _ = RECORD.try_with(|record| {
        if record.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get().saturating_add(1)));
        }
    });
}

// SAFETY: Every operation delegates unchanged arguments/results to System.
// Allocation-free thread-local counters tolerate thread-local teardown.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations_during(run: impl FnOnce()) -> usize {
    struct StopRecording;
    impl Drop for StopRecording {
        fn drop(&mut self) {
            RECORD.with(|record| record.set(false));
        }
    }
    ALLOCATIONS.with(|count| count.set(0));
    RECORD.with(|record| record.set(true));
    let stop = StopRecording;
    run();
    drop(stop);
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn allocation_counter_observes_allocations() {
    assert!(
        allocations_during(|| {
            black_box(Box::new(black_box(42_u64)));
        }) > 0
    );
}

#[test]
fn construction_commit_sampling_hold_resume_snap_and_alpha_errors_do_not_allocate() {
    let allocations = allocations_during(|| {
        let mut flat = InterpolationPair::new(Vec2::ZERO);
        let mut pose = InterpolationPair::new((Vec3::ZERO, Quat::IDENTITY));
        let flat_blend = |a: &Vec2, b: &Vec2, t| a.lerp(*b, t);
        let pose_blend =
            |a: &(Vec3, Quat), b: &(Vec3, Quat), t| (a.0.lerp(b.0, t), a.1.slerp(b.1, t));
        for tick in 0..512 {
            let position = black_box(tick as f32);
            flat.commit(Vec2::new(position, -position));
            pose.commit((
                Vec3::new(position, 1.0, 2.0),
                Quat::from_rotation_y(position * 0.01),
            ));
            for alpha in [-0.0, 0.0, 0.25, 0.75, 1.0] {
                black_box(flat.sample_with(black_box(alpha), flat_blend).unwrap());
                black_box(pose.sample_with(black_box(alpha), pose_blend).unwrap());
            }
            assert!(flat.hold());
            assert!(pose.hold());
            assert!(!flat.hold());
            let flat_before = flat.sample_with(0.0, flat_blend).unwrap();
            let pose_before = pose.sample_with(0.0, pose_blend).unwrap();
            assert_eq!(flat_before, *flat.current());
            assert_eq!(pose_before.0, pose.current().0);
            for alpha in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
                assert_eq!(
                    flat.sample_with(black_box(alpha), flat_blend),
                    Err(InterpolationError::InvalidAlpha)
                );
                assert_eq!(
                    pose.sample_with(black_box(alpha), pose_blend),
                    Err(InterpolationError::InvalidAlpha)
                );
            }
            assert!(flat.resume());
            assert!(pose.resume());
            assert!(!pose.resume());
            black_box((
                flat.previous(),
                flat.current(),
                flat.alpha(),
                flat.is_held(),
                flat.generation(),
            ));
            black_box((
                pose.previous(),
                pose.current(),
                pose.alpha(),
                pose.is_held(),
                pose.generation(),
            ));
        }
        flat.hold();
        pose.hold();
        assert_eq!(flat.snap(Vec2::ZERO), 1);
        assert_eq!(pose.snap((Vec3::ZERO, Quat::IDENTITY)), 1);
        assert!(!flat.is_held() && !pose.is_held());
        black_box(flat.sample_with(0.0, flat_blend).unwrap());
        black_box(pose.sample_with(0.0, pose_blend).unwrap());
    });
    assert_eq!(allocations, 0);
}
