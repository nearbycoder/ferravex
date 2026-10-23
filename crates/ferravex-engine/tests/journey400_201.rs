//! Allocation checks through the registered core and application APIs.
use ferravex_core::{FixedClock, TickContext, session::GameSession};
use ferravex_engine::app::{App, AppControl, Game};
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

// SAFETY: All allocation operations forward unchanged arguments/results to
// System. The counters are allocation-free thread locals and tolerate teardown.
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
fn scheduling_iteration_manual_steps_and_reset_do_not_allocate() {
    let allocations = allocations_during(|| {
        let mut clock = FixedClock::new(64.0, 4);
        let mut session = GameSession::new(64.0, 4, true);
        for elapsed in [0.0, 0.5 / 64.0, 1.0 / 64.0, 10.5 / 64.0, f64::NAN, f64::MAX] {
            for batch in [
                clock.advance_batch(black_box(elapsed)),
                session.advance_batch(elapsed),
            ] {
                black_box(batch.frame());
                for context in batch.ticks() {
                    black_box(context);
                }
                black_box(batch.ticks().next_back());
                black_box(batch.ticks().nth(2));
            }
        }
        assert!(session.step_paused_batch().is_err());
        session.set_paused(true);
        for _ in 0..4 {
            black_box(session.step_paused_batch().unwrap().ticks().next());
        }
        assert_eq!(session.advance_batch(100.0).ticks().len(), 0);
        session.restart();
        session.reset();
        clock.reset();
    });
    assert_eq!(allocations, 0);
}

struct AllocationProbe {
    calls: u32,
    outcome: Result<AppControl, &'static str>,
}

impl Game for AllocationProbe {
    type Error = &'static str;

    fn fixed_update(&mut self, _: f32) -> Result<AppControl, Self::Error> {
        panic!("context override owns updates");
    }

    fn fixed_update_context(&mut self, context: TickContext) -> Result<AppControl, Self::Error> {
        black_box(context);
        self.calls += 1;
        self.outcome
    }
}

#[test]
fn app_context_dispatch_and_partial_reports_do_not_allocate() {
    for outcome in [
        Ok(AppControl::Continue),
        Ok(AppControl::Stop),
        Err("failed update"),
    ] {
        let allocations = allocations_during(|| {
            let game = AllocationProbe { calls: 0, outcome };
            let mut app = App::new(game, GameSession::new(64.0, 4, true));
            assert_eq!(app.frame_report(0.5 / 64.0).unwrap().executed_updates, 0);
            assert_eq!(app.game().calls, 0);
            let result = app.frame_report(8.0 / 64.0);
            assert_eq!(result.is_err(), outcome.is_err());
            let report = app.last_frame_report().unwrap();
            assert_eq!(report.scheduled_updates, 4);
            assert_eq!(
                report.executed_updates,
                if outcome == Ok(AppControl::Continue) {
                    4
                } else {
                    1
                }
            );
            assert_eq!(report.executed_updates, app.game().calls);
            app.stop().unwrap();
        });
        assert_eq!(allocations, 0);
    }
}
