//! Allocation check of the registered public tracker API. World/fixture creation
//! occurs outside measurement; this measures the tracker and caller-only key
//! resolution, not physics-world allocation behavior.
#![cfg(any(feature = "dim2", feature = "dim3"))]

#[cfg(feature = "dim2")]
use ferravex_physics::d2::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
#[cfg(all(not(feature = "dim2"), feature = "dim3"))]
use ferravex_physics::d3::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
use ferravex_physics::{
    ContactEvent,
    triggers::{
        InputCompleteness, Occupancy, OccupancyEvent, ResolvedPair, SensorOccupancyTracker,
        StartResolution, TriggerError,
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct CountingAllocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = ALLOCATIONS.try_with(|cell| {
            if let Some(count) = cell.get() {
                cell.set(Some(count + 1));
            }
        });
    }
}
// SAFETY: Every operation forwards its identical allocation contract to System.
// The thread-local counter allocates nothing and tolerates TLS destruction.
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
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct StopCounting;
impl Drop for StopCounting {
    fn drop(&mut self) {
        ALLOCATIONS.with(|cell| cell.set(None));
    }
}
fn allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|cell| cell.set(Some(0)));
    let guard = StopCounting;
    run();
    let count = ALLOCATIONS.with(Cell::get).unwrap();
    drop(guard);
    count
}

#[test]
fn construction_updates_errors_removal_reconciliation_and_output_allocate_nothing() {
    // Verify the instrument before relying on its zero-allocation result.
    assert!(
        allocations(|| {
            black_box(vec![black_box(1_u8); 64]);
        }) > 0
    );
    let mut world = PhysicsWorld::default();
    let (_, sensor) = world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::ball(3.).sensor(true),
    );
    let actor = world
        .insert_compound(
            RigidBodyBuilder::fixed(),
            [ColliderBuilder::ball(1.), ColliderBuilder::ball(1.)],
        )
        .unwrap();
    let a = world.bodies[actor].colliders()[0];
    let b = world.bodies[actor].colliders()[1];
    // This fixture uses real world pairs; allocation counting covers only the adapter.
    world.step(1. / 120.);
    assert_eq!(world.contact_events().len(), 2);
    let occupancy = Occupancy {
        sensor: 7_u16,
        actor: 42_u32,
    };
    let snapshot = [
        ResolvedPair {
            collider1: sensor,
            collider2: a,
            occupancy,
        },
        ResolvedPair {
            collider1: sensor,
            collider2: b,
            occupancy,
        },
    ];
    let count = allocations(|| {
        let mut tracker = SensorOccupancyTracker::<2, 1, u16, u32>::new();
        for _ in 0..64 {
            tracker
                .consume_batch(
                    world.contact_events(),
                    InputCompleteness::Complete,
                    |_, _| StartResolution::Track(occupancy),
                )
                .unwrap();
            assert_eq!(tracker.active_pairs(occupancy), 2);
            assert_eq!(tracker.occupants().count(), 1);
            assert_eq!(tracker.events().len(), 1);
            assert_eq!(tracker.remove_actor(42), Err(TriggerError::EventCapacity));
            assert_eq!(
                tracker.pop_event(),
                Some(OccupancyEvent::Entered(occupancy))
            );
            assert_eq!(tracker.remove_actor(42), Ok(1));
            tracker
                .consume(ContactEvent::Started(sensor, a), |_, _| {
                    panic!("retained duplicate")
                })
                .unwrap();
            tracker
                .consume(ContactEvent::Stopped(sensor, a), |_, _| {
                    panic!("stop resolution")
                })
                .unwrap();
            tracker
                .consume(ContactEvent::Stopped(sensor, b), |_, _| {
                    panic!("stop resolution")
                })
                .unwrap();
            assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(occupancy)));
            tracker.mark_incomplete();
            assert_eq!(
                tracker.reconcile(&snapshot, InputCompleteness::Incomplete),
                Err(TriggerError::IncompleteInput)
            );
            tracker
                .reconcile(&snapshot, InputCompleteness::Complete)
                .unwrap();
            assert_eq!(tracker.remove_sensor(7), Ok(1));
            tracker.clear_events();
            tracker.reset();
            black_box(&tracker);
        }
    });
    assert_eq!(count, 0);
}
