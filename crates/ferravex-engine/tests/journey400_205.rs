//! Behavioral and allocation checks through the registered production command API.
use ferravex_engine::commands::{
    CommandFailure, CommandFailurePolicy, FallibleCommandApplyOutcome, FallibleCommandBuffer,
    FallibleCommandBufferMetrics,
};
use ferravex_engine::systems::{ScheduleRunError, SystemControl, SystemEntryKind, SystemSchedule};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

const POLICIES: [CommandFailurePolicy; 2] = [
    CommandFailurePolicy::StopOnError,
    CommandFailurePolicy::Continue,
];

#[test]
fn first_middle_last_failure_obeys_policy_budget_and_preserves_tail_without_retry() {
    let ids = [90, 4, 27];
    for policy in POLICIES {
        for failed_index in 0..ids.len() {
            for budget in [0, 1, 2, 3, usize::MAX] {
                let mut commands = FallibleCommandBuffer::new(ids.len());
                for (index, id) in ids.into_iter().enumerate() {
                    assert!(
                        commands
                            .push(id, move |world: &mut Vec<usize>| {
                                world.push(index); // Includes mutation before a domain error.
                                if index == failed_index {
                                    Err(index)
                                } else {
                                    Ok(())
                                }
                            })
                            .is_ok()
                    );
                }
                let mut world = Vec::new();
                let mut errors = Vec::new();
                let report = commands.apply_report(&mut world, budget, policy, |e| errors.push(e));
                let started = budget.min(if policy == CommandFailurePolicy::StopOnError {
                    failed_index + 1
                } else {
                    ids.len()
                });
                let failed = usize::from(failed_index < started);
                assert_eq!(
                    report,
                    FallibleCommandApplyOutcome {
                        started,
                        applied: started - failed,
                        failed,
                        remaining: ids.len() - started,
                    }
                );
                assert_eq!(world, (0..started).collect::<Vec<_>>());
                assert_eq!(errors.len(), failed);
                assert_eq!(commands.metrics().started, started as u64);
                assert_eq!(commands.metrics().applied, (started - failed) as u64);
                assert_eq!(commands.metrics().failed, failed as u64);
                let tail = commands.apply_report(
                    &mut world,
                    usize::MAX,
                    CommandFailurePolicy::Continue,
                    |e| errors.push(e),
                );
                assert_eq!(tail.started, ids.len() - started);
                assert_eq!(tail.remaining, 0);
                assert_eq!(world, [0, 1, 2]);
                assert_eq!(
                    errors,
                    [CommandFailure {
                        id: ids[failed_index],
                        error: failed_index
                    }]
                );
                assert_eq!(commands.metrics().started, 3);
                assert_eq!(commands.metrics().failed, 1);
                assert_eq!(commands.metrics().applied, 2);
            }
        }
    }
}

#[test]
fn continue_delivers_every_error_in_fifo_order_and_errors_consume_budget() {
    let mut commands = FallibleCommandBuffer::new(4);
    for (index, id) in [u64::MAX, 0, u64::MAX, 8].into_iter().enumerate() {
        assert!(commands.push(id, move |_: &mut ()| Err(index)).is_ok());
    }
    let mut errors = Vec::new();
    let report = commands.apply_report(&mut (), 2, CommandFailurePolicy::Continue, |e| {
        errors.push(e)
    });
    assert_eq!(
        report,
        FallibleCommandApplyOutcome {
            started: 2,
            applied: 0,
            failed: 2,
            remaining: 2
        }
    );
    assert_eq!(
        errors,
        [
            CommandFailure {
                id: u64::MAX,
                error: 0
            },
            CommandFailure { id: 0, error: 1 }
        ]
    );
    let report = commands.apply_report(&mut (), usize::MAX, CommandFailurePolicy::Continue, |e| {
        errors.push(e)
    });
    assert_eq!(
        report,
        FallibleCommandApplyOutcome {
            started: 2,
            applied: 0,
            failed: 2,
            remaining: 0
        }
    );
    assert_eq!(
        errors[2..],
        [
            CommandFailure {
                id: u64::MAX,
                error: 2
            },
            CommandFailure { id: 8, error: 3 }
        ]
    );
}

struct DropProbe {
    drops: Rc<Cell<usize>>,
    panic: bool,
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        assert!(!self.panic, "payload destructor failed");
    }
}

fn probe(drops: &Rc<Cell<usize>>) -> DropProbe {
    DropProbe {
        drops: Rc::clone(drops),
        panic: false,
    }
}

#[test]
fn zero_capacity_returns_the_original_nonclone_closure_without_execution_or_drop() {
    let drops = Rc::new(Cell::new(0));
    let payload = probe(&drops);
    let mut commands = FallibleCommandBuffer::<usize, ()>::new(0);
    assert!(commands.is_empty());
    assert!(commands.is_full());
    assert_eq!(
        (
            commands.len(),
            commands.capacity(),
            commands.remaining_capacity()
        ),
        (0, 0, 0)
    );
    let rejected = commands
        .push(20, move |world| {
            *world += 1;
            drop(payload);
            Ok(())
        })
        .err()
        .unwrap();
    assert_eq!(drops.get(), 0);
    assert_eq!(commands.metrics().rejected, 1);
    let mut world = 0;
    for policy in POLICIES {
        assert_eq!(
            commands.apply_report(&mut world, usize::MAX, policy, |_| panic!("no errors")),
            FallibleCommandApplyOutcome::default()
        );
    }
    assert_eq!(world, 0);
    rejected(&mut world).unwrap();
    assert_eq!((world, drops.get()), (1, 1));
    assert_eq!(commands.metrics().started, 0);
}

#[test]
fn capacity_rejection_can_be_retried_at_the_tail_with_the_same_id() {
    let drops = Rc::new(Cell::new(0));
    let payload = probe(&drops);
    let mut commands = FallibleCommandBuffer::new(2);
    for value in 1..=2 {
        assert!(
            commands
                .push(value, move |world: &mut Vec<u64>| {
                    world.push(value);
                    Ok(())
                })
                .is_ok()
        );
    }
    assert!(commands.is_full());
    assert_eq!(commands.remaining_capacity(), 0);
    let rejected = commands
        .push(99, move |world| {
            world.push(3);
            drop(payload);
            Err("third")
        })
        .err()
        .unwrap();
    assert_eq!(drops.get(), 0);
    let mut world = Vec::new();
    let first = commands.apply_report(
        &mut world,
        1,
        CommandFailurePolicy::StopOnError,
        |_| unreachable!(),
    );
    assert_eq!((first.applied, first.remaining), (1, 1));
    assert_eq!(commands.remaining_capacity(), 1);
    assert!(commands.push(99, rejected).is_ok());
    let mut failure = None;
    let tail = commands.apply_report(&mut world, 2, CommandFailurePolicy::Continue, |e| {
        failure = Some(e)
    });
    assert_eq!((tail.applied, tail.failed), (1, 1));
    assert_eq!(
        failure,
        Some(CommandFailure {
            id: 99,
            error: "third"
        })
    );
    assert_eq!(world, [1, 2, 3]);
    assert_eq!(drops.get(), 1);
    assert_eq!(
        (commands.metrics().accepted, commands.metrics().rejected),
        (3, 1)
    );
}

#[test]
fn errors_transfer_owned_nonclone_nondebug_payloads_to_the_callback() {
    let drops = Rc::new(Cell::new(0));
    let mut commands = FallibleCommandBuffer::new(2);
    for id in [5, 9] {
        let error = probe(&drops);
        assert!(commands.push(id, move |_: &mut ()| Err(error)).is_ok());
    }
    let mut held = Vec::new();
    let report = commands.apply_report(&mut (), 2, CommandFailurePolicy::Continue, |error| {
        held.push(error)
    });
    assert_eq!((report.started, report.failed), (2, 2));
    assert_eq!((held[0].id, held[1].id), (5, 9));
    assert_eq!(drops.get(), 0);
    drop(commands);
    assert_eq!(drops.get(), 0);
    drop(held);
    assert_eq!(drops.get(), 2);
}

#[test]
fn clear_and_queue_drop_release_payloads_once_without_invoking_them() {
    let drops = Rc::new(Cell::new(0));
    let mut commands = FallibleCommandBuffer::<(), ()>::new(2);
    for id in 0..2 {
        let payload = probe(&drops);
        assert!(
            commands
                .push(id, move |_| {
                    drop(payload);
                    panic!("discarded command ran")
                })
                .is_ok()
        );
    }
    let rejected_payload = probe(&drops);
    let rejected = commands
        .push(2, move |_| {
            drop(rejected_payload);
            Ok(())
        })
        .err()
        .unwrap();
    assert_eq!(drops.get(), 0);
    drop(rejected);
    assert_eq!(drops.get(), 1);
    commands.clear();
    assert_eq!(drops.get(), 3);
    assert_eq!(commands.metrics().discarded, 2);
    assert_eq!(commands.metrics().started, 0);
    commands.clear();
    assert_eq!(drops.get(), 3);
    let pending = probe(&drops);
    assert!(
        commands
            .push(3, move |_| {
                drop(pending);
                panic!("dropped queue ran")
            })
            .is_ok()
    );
    drop(commands);
    assert_eq!(drops.get(), 4);
}

#[test]
fn reset_metrics_preserves_pending_commands_and_clear_supports_restart() {
    let mut commands = FallibleCommandBuffer::new(3);
    for id in 1..=3 {
        assert!(
            commands
                .push(id, move |world: &mut Vec<u64>| {
                    world.push(id);
                    Err(id)
                })
                .is_ok()
        );
    }
    let mut world = Vec::new();
    let first = commands.apply_report(&mut world, 1, CommandFailurePolicy::StopOnError, |e| {
        assert_eq!(e.id, 1)
    });
    assert_eq!(first.remaining, 2);
    let metrics = commands.metrics();
    assert_eq!(commands.metrics(), metrics);
    assert_eq!(commands.reset_metrics(), metrics);
    assert_eq!(
        commands.metrics(),
        FallibleCommandBufferMetrics {
            high_water_mark: 2,
            ..Default::default()
        }
    );
    let second = commands.apply_report(&mut world, 1, CommandFailurePolicy::StopOnError, |e| {
        assert_eq!(e.id, 2)
    });
    assert_eq!(second.remaining, 1);
    assert_eq!(world, [1, 2]);
    commands.clear();
    assert_eq!(
        commands.metrics(),
        FallibleCommandBufferMetrics {
            started: 1,
            failed: 1,
            discarded: 1,
            high_water_mark: 2,
            ..Default::default()
        }
    );
    assert_eq!(commands.remaining_capacity(), 3);
    assert!(
        commands
            .push(4, |world| {
                world.push(4);
                Ok(())
            })
            .is_ok()
    );
    let last = commands.apply_report(
        &mut world,
        1,
        CommandFailurePolicy::Continue,
        |_| unreachable!(),
    );
    assert_eq!(last.applied, 1);
    assert_eq!(world, [1, 2, 4]);
}

#[test]
fn command_panics_propagate_under_both_policies_and_keep_the_unvisited_tail() {
    for policy in POLICIES {
        for panic_index in 0..3 {
            let drops = Rc::new(Cell::new(0));
            let mut commands = FallibleCommandBuffer::<Vec<usize>, ()>::new(3);
            for index in 0..3 {
                let payload = probe(&drops);
                assert!(
                    commands
                        .push(index as u64, move |world| {
                            world.push(index);
                            let _payload = payload;
                            assert_ne!(index, panic_index, "command panicked after mutation");
                            Ok(())
                        })
                        .is_ok()
                );
            }
            let mut world = Vec::new();
            let result = catch_unwind(AssertUnwindSafe(|| {
                commands.apply_report(&mut world, 3, policy, |_| {
                    panic!("panic was converted to error")
                })
            }));
            assert!(result.is_err());
            assert_eq!(world, (0..=panic_index).collect::<Vec<_>>());
            assert_eq!(drops.get(), panic_index + 1);
            assert_eq!(commands.metrics().started, (panic_index + 1) as u64);
            assert_eq!(commands.metrics().applied, panic_index as u64);
            assert_eq!(commands.metrics().failed, 0);
            let tail = commands.apply_report(&mut world, usize::MAX, policy, |_| unreachable!());
            assert_eq!(tail.started, 2 - panic_index);
            assert_eq!(world, [0, 1, 2]);
            assert_eq!(drops.get(), 3);
        }
    }
}

#[test]
fn callback_panics_keep_the_tail_and_record_the_domain_failure_before_notification() {
    for policy in POLICIES {
        let drops = Rc::new(Cell::new(0));
        let error = probe(&drops);
        let mut commands = FallibleCommandBuffer::new(3);
        assert!(
            commands
                .push(1, |world: &mut Vec<u8>| {
                    world.push(1);
                    Ok(())
                })
                .is_ok()
        );
        assert!(
            commands
                .push(2, move |world| {
                    world.push(2);
                    Err(error)
                })
                .is_ok()
        );
        assert!(
            commands
                .push(3, |world| {
                    world.push(3);
                    Ok(())
                })
                .is_ok()
        );
        let mut world = Vec::new();
        let result = catch_unwind(AssertUnwindSafe(|| {
            commands.apply_report(&mut world, 3, policy, |failure| {
                assert_eq!(failure.id, 2);
                drop(failure);
                panic!("error callback failed");
            })
        }));
        assert!(result.is_err());
        assert_eq!(world, [1, 2]);
        assert_eq!(drops.get(), 1);
        assert_eq!(
            (
                commands.metrics().started,
                commands.metrics().applied,
                commands.metrics().failed
            ),
            (2, 1, 1)
        );
        assert_eq!(commands.len(), 1);
        let tail = commands.apply_report(&mut world, 3, policy, |_| unreachable!());
        assert_eq!(tail.applied, 1);
        assert_eq!(world, [1, 2, 3]);
    }
}

#[test]
fn panicking_clear_destructor_is_counted_and_preserves_later_commands() {
    let drops = Rc::new(Cell::new(0));
    let mut commands = FallibleCommandBuffer::<Vec<u8>, ()>::new(3);
    for id in 0..2 {
        let payload = DropProbe {
            drops: Rc::clone(&drops),
            panic: id == 1,
        };
        assert!(
            commands
                .push(id, move |_| {
                    drop(payload);
                    panic!("cleared command ran")
                })
                .is_ok()
        );
    }
    assert!(
        commands
            .push(2, |world| {
                world.push(2);
                Ok(())
            })
            .is_ok()
    );
    assert!(catch_unwind(AssertUnwindSafe(|| commands.clear())).is_err());
    assert_eq!(drops.get(), 2);
    assert_eq!(commands.metrics().discarded, 2);
    assert_eq!(commands.metrics().started, 0);
    assert_eq!(commands.len(), 1);
    let mut world = Vec::new();
    let report = commands.apply_report(
        &mut world,
        3,
        CommandFailurePolicy::Continue,
        |_| unreachable!(),
    );
    assert_eq!(report.applied, 1);
    assert_eq!(world, [2]);
}

#[test]
fn world_and_returned_errors_need_not_be_static() {
    let message = String::from("borrowed error");
    let mut world = message.as_str();
    let mut commands = FallibleCommandBuffer::new(1);
    assert!(commands.push(1, |world: &mut &str| Err(*world)).is_ok());
    let mut failure = None;
    let report = commands.apply_report(&mut world, 1, CommandFailurePolicy::StopOnError, |e| {
        failure = Some(e)
    });
    assert_eq!(report.failed, 1);
    assert_eq!(failure.unwrap().error, message.as_str());
}

#[test]
fn prepared_flush_propagates_command_identity_and_suppresses_downstream_work() {
    struct Simulation {
        stock: u32,
        commands: FallibleCommandBuffer<u32, &'static str>,
        observed: bool,
    }
    for policy in POLICIES {
        let mut schedule = SystemSchedule::<Simulation, CommandFailure<&'static str>>::default();
        schedule
            .register_flush("flush", move |sim| {
                let mut first_error = None;
                let report = sim
                    .commands
                    .apply_report(&mut sim.stock, 3, policy, |error| {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    });
                assert_eq!(
                    report.remaining,
                    usize::from(policy == CommandFailurePolicy::StopOnError)
                );
                if let Some(error) = first_error {
                    return Err(error);
                }
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule
            .register("observe", |sim| {
                sim.observed = true;
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule.before_checked("flush", "observe").unwrap();
        schedule.prepare().unwrap();
        let mut sim = Simulation {
            stock: 4,
            commands: FallibleCommandBuffer::new(3),
            observed: false,
        };
        for id in [10, 11] {
            assert!(
                sim.commands
                    .push(id, |stock| {
                        *stock = stock.checked_sub(3).ok_or("insufficient stock")?;
                        Ok(())
                    })
                    .is_ok()
            );
        }
        assert!(
            sim.commands
                .push(12, |stock| {
                    *stock += 5;
                    Ok(())
                })
                .is_ok()
        );
        match schedule.run(&mut sim) {
            Err(ScheduleRunError::Callback {
                name,
                kind,
                error,
                report,
            }) => {
                assert_eq!(name, "flush");
                assert_eq!(kind, SystemEntryKind::Flush);
                assert_eq!(
                    error,
                    CommandFailure {
                        id: 11,
                        error: "insufficient stock"
                    }
                );
                assert_eq!(
                    (report.invoked, report.flushes, report.control),
                    (1, 1, SystemControl::Stop)
                );
            }
            result => panic!("expected typed flush error, got {result:?}"),
        }
        assert!(!sim.observed);
        assert_eq!(
            sim.stock,
            if policy == CommandFailurePolicy::StopOnError {
                1
            } else {
                6
            }
        );
        let tail = sim.commands.apply_report(
            &mut sim.stock,
            1,
            CommandFailurePolicy::Continue,
            |_| unreachable!(),
        );
        assert_eq!(
            tail.started,
            usize::from(policy == CommandFailurePolicy::StopOnError)
        );
        assert_eq!(sim.stock, 6);
    }
}

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

// SAFETY: Unchanged arguments/results forward to System. Instrumentation uses
// allocation-free thread locals and tolerates thread-local teardown.
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

#[test]
fn rejection_inspection_apply_reports_errors_and_clear_add_no_allocations() {
    for policy in POLICIES {
        let mut commands = FallibleCommandBuffer::new(4);
        for id in 0..4 {
            assert!(
                commands
                    .push(id, move |world: &mut u64| {
                        *world += id;
                        if id % 2 == 0 { Err(id) } else { Ok(()) }
                    })
                    .is_ok()
            );
        }
        let mut world = 0;
        let mut errors = [None; 2];
        let mut count = 0;
        let capture = black_box([7_u64; 16]); // Boxing this nonzero payload would allocate.
        let allocations = allocations_during(|| {
            let rejected = commands.push(7, move |world| {
                *world += capture[0];
                Ok(())
            });
            assert!(rejected.is_err());
            let _ = black_box(rejected);
            black_box(commands.metrics());
            black_box(commands.reset_metrics());
            assert_eq!((commands.len(), commands.remaining_capacity()), (4, 0));
            let zero = commands.apply_report(&mut world, 0, policy, |_| unreachable!());
            assert_eq!(zero.started, 0);
            let first = commands.apply_report(&mut world, 3, policy, |error| {
                errors[count] = Some((error.id, error.error));
                count += 1;
            });
            assert_eq!(
                first.started,
                if policy == CommandFailurePolicy::StopOnError {
                    1
                } else {
                    3
                }
            );
            let rest = commands.apply_report(
                &mut world,
                usize::MAX,
                CommandFailurePolicy::Continue,
                |error| {
                    errors[count] = Some((error.id, error.error));
                    count += 1;
                },
            );
            assert_eq!(rest.remaining, 0);
            commands.clear();
        });
        assert_eq!(allocations, 0);
        assert_eq!(errors, [Some((0, 0)), Some((2, 2))]);
        assert_eq!(world, 6);
        assert_eq!(commands.metrics().started, 4);
        for id in 0..4 {
            assert!(commands.push(id, move |_| Err(id)).is_ok());
        }
        assert_eq!(allocations_during(|| commands.clear()), 0);
        assert_eq!(commands.metrics().discarded, 4);
    }
    assert_eq!(
        allocations_during(|| {
            let mut zero = FallibleCommandBuffer::<(), ()>::new(0);
            let capture = black_box(7);
            assert!(
                zero.push(1, move |_| {
                    black_box(capture);
                    Ok(())
                })
                .is_err()
            );
            zero.clear();
        }),
        0
    );
}
