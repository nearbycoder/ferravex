//! Allocation regressions through the registered production schedule API.
use ferravex_engine::systems::{ScheduleRunError, SystemControl, SystemSchedule};
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

// SAFETY: Every operation forwards unchanged arguments/results to System.
// Instrumentation uses allocation-free thread locals and tolerates teardown.
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
fn allocation_instrumentation_has_a_positive_control() {
    assert!(
        allocations_during(|| {
            black_box(Box::new(black_box(17_u64)));
        }) > 0
    );
}

#[derive(Default)]
struct Context {
    terminal: Option<(usize, bool)>,
    invoked: usize,
    predicates: usize,
}

#[test]
fn full_capacity_first_and_repeated_runs_do_not_allocate_even_on_stop_or_error() {
    let mut schedule = SystemSchedule::<Context, usize>::default();
    for index in 0..256 {
        let callback = move |context: &mut Context| {
            context.invoked += 1;
            match context.terminal {
                Some((stop, fail)) if index == stop => {
                    if fail {
                        Err(index)
                    } else {
                        Ok(SystemControl::Stop)
                    }
                }
                _ => Ok(SystemControl::Continue),
            }
        };
        let name = index.to_string();
        if index % 2 == 0 {
            schedule.register(&name, callback).unwrap();
        } else {
            schedule.register_flush(&name, callback).unwrap();
        }
        // Exercise a stateful predicate without mutating the shared context.
        let mut calls = 0;
        schedule
            .set_run_if(&name, move |context| {
                calls += 1;
                black_box((calls, context.predicates));
                index != 1 && index != 3
            })
            .unwrap();
    }
    schedule.prepare().unwrap();
    let allocations = allocations_during(|| {
        for terminal in [
            None,
            Some((0, false)),
            Some((0, true)),
            Some((128, false)),
            Some((128, true)),
            Some((255, false)),
            Some((255, true)),
        ] {
            let mut context = Context {
                terminal,
                ..Context::default()
            };
            let result = schedule.run(&mut context);
            let report = match result {
                Ok(report) => {
                    assert!(!terminal.is_some_and(|(_, fail)| fail));
                    report
                }
                Err(ScheduleRunError::Callback {
                    name,
                    error,
                    report,
                    ..
                }) => {
                    assert_eq!(Some((error, true)), terminal);
                    assert_eq!(name.parse::<usize>().unwrap(), error);
                    report
                }
                Err(ScheduleRunError::NotPrepared) => panic!("prepared schedule rejected"),
            };
            let visited = terminal.map_or(256, |(index, _)| index + 1);
            let skipped = if visited == 1 { 0 } else { 2 };
            assert_eq!(report.scheduled, 256);
            assert_eq!(report.skipped, skipped);
            assert_eq!(report.invoked, visited - skipped);
            assert_eq!(report.invoked, context.invoked);
            assert_eq!(
                report.control,
                if terminal.is_none() {
                    SystemControl::Continue
                } else {
                    SystemControl::Stop
                }
            );
            schedule.prepare().unwrap(); // Cached no-op, no fresh graph resolution.
            assert_eq!(black_box(schedule.prepared_names().unwrap().count()), 256);
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn unprepared_rejection_and_prepared_empty_run_do_not_allocate() {
    let mut schedule = SystemSchedule::<(), ()>::default();
    assert_eq!(
        allocations_during(|| {
            assert_eq!(schedule.run(&mut ()), Err(ScheduleRunError::NotPrepared));
        }),
        0
    );
    schedule.prepare().unwrap();
    assert_eq!(
        allocations_during(|| {
            assert_eq!(schedule.run(&mut ()).unwrap().invoked, 0);
            assert_eq!(schedule.prepared_names().unwrap().count(), 0);
            schedule.clear();
            assert_eq!(schedule.run(&mut ()), Err(ScheduleRunError::NotPrepared));
        }),
        0
    );
}
