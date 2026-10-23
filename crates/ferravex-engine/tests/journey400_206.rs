use ferravex_engine::commands::{CommandFailurePolicy, FallibleCommandApplyOutcome};
use ferravex_engine::ecs::{Entity, Error, SpawnError, World};
use ferravex_engine::ecs_commands::{
    AppliedEntityCommand, BufferError, EcsCommandBuffer, EcsCommandErrorKind, EnqueueError,
    EntityCommandKind, SpawnToken, TokenError,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

const POLICIES: [CommandFailurePolicy; 2] = [
    CommandFailurePolicy::StopOnError,
    CommandFailurePolicy::Continue,
];

fn prepared(capacity: usize) -> World {
    let mut world = World::new();
    world.prepare_entities(capacity).unwrap();
    world.prepare_component::<u32>(capacity).unwrap();
    world.prepare_component::<bool>(capacity).unwrap();
    world
}

fn spawn(commands: &mut EcsCommandBuffer, id: u64, value: u32) -> SpawnToken {
    commands.spawn(id, (value,)).unwrap()
}

#[test]
fn query_authoring_has_no_mutations_until_flush_and_acceptance_drives_feedback() {
    let mut world = prepared(4);
    let old = world.spawn((0_u32,));
    let mut commands = EcsCommandBuffer::new(4, 1);
    let token;
    {
        let query = world.query::<(Entity, &u32)>();
        for (entity, health) in &query {
            if *health == 0 {
                commands.despawn(10, entity).unwrap();
            }
        }
        token = spawn(&mut commands, 11, 7);
        commands.insert(12, &token, (true,)).unwrap();
        commands.remove::<u32>(13, &token).unwrap();
        assert_eq!(query.iter().count(), 1);
        assert_eq!(*world.get::<&u32>(old).unwrap(), 0);
    }
    let mut report = commands
        .apply_report(&mut world, 4, CommandFailurePolicy::Continue, |_| {
            panic!("unexpected failure")
        })
        .unwrap();
    assert_eq!(report.commands.applied, 4);
    let entity = report.take_spawn(&token).unwrap();
    assert_eq!(
        report.accepted(),
        [
            AppliedEntityCommand {
                id: 10,
                kind: EntityCommandKind::Despawn,
                entity: old
            },
            AppliedEntityCommand {
                id: 11,
                kind: EntityCommandKind::Spawn,
                entity
            },
            AppliedEntityCommand {
                id: 12,
                kind: EntityCommandKind::Insert,
                entity
            },
            AppliedEntityCommand {
                id: 13,
                kind: EntityCommandKind::Remove,
                entity
            },
        ]
    );
    assert!(!world.contains(old));
    assert!(!world.has::<u32>(entity));
    assert!(*world.get::<&bool>(entity).unwrap());
}

#[test]
fn partial_flushes_keep_claimed_and_unclaimed_mappings_and_never_reexecute_spawns() {
    let mut world = prepared(3);
    let mut commands = EcsCommandBuffer::new(6, 2);
    let first = spawn(&mut commands, 0, 3);
    let second = spawn(&mut commands, u64::MAX, 4);
    commands.insert(2, &first, (true,)).unwrap();
    commands.remove::<u32>(3, &first).unwrap();
    commands.despawn(4, &second).unwrap();
    assert_eq!(commands.take_spawn(&first), Err(TokenError::Pending));
    let zero = commands
        .apply_report(&mut world, 0, POLICIES[0], |_| unreachable!())
        .unwrap();
    assert_eq!(
        zero.commands,
        FallibleCommandApplyOutcome {
            remaining: 5,
            ..Default::default()
        }
    );
    assert!(zero.accepted().is_empty());
    let mut report = commands
        .apply_report(&mut world, 1, POLICIES[0], |_| unreachable!())
        .unwrap();
    let entity = report.take_spawn(&first).unwrap();
    assert_eq!(report.take_spawn(&first), Err(TokenError::AlreadyTaken));
    assert_eq!(report.take_spawn(&second), Err(TokenError::Pending));
    let report = commands
        .apply_report(&mut world, 1, POLICIES[0], |_| unreachable!())
        .unwrap();
    assert_eq!(report.commands.remaining, 3);
    assert_eq!(commands.remaining_capacity(), 3);
    assert_eq!(commands.remaining_spawn_capacity(), 0);
    assert_eq!(
        commands.spawn(9, ()).unwrap_err().reason,
        EnqueueError::SpawnCapacity
    );
    let report = commands
        .apply_report(&mut world, usize::MAX, POLICIES[0], |_| unreachable!())
        .unwrap();
    assert_eq!((report.commands.applied, report.commands.remaining), (3, 0));
    assert!(!world.has::<u32>(entity));
    assert!(world.has::<bool>(entity));
    let removed = commands.take_spawn(&second).unwrap();
    assert!(!world.contains(removed)); // Mapping denotes acceptance, not liveness.
    assert_eq!(world.len(), 1);
    assert_eq!(
        commands
            .apply_report(&mut world, 99, POLICIES[0], |_| unreachable!())
            .unwrap()
            .commands
            .started,
        0
    );
}

#[test]
fn foreign_reset_and_destroyed_buffer_tokens_cannot_be_revived() {
    let mut commands = EcsCommandBuffer::new(4, 2);
    let token = spawn(&mut commands, 1, 1);
    assert_eq!(token, token.clone());
    let mut other = EcsCommandBuffer::new(4, 2);
    let other_token = spawn(&mut other, 1, 1);
    assert_ne!(token, other_token);
    assert_eq!(other.take_spawn(&token), Err(TokenError::ForeignBuffer));
    assert_eq!(
        other.insert(2, &token, (7_u32,)).unwrap_err().reason,
        EnqueueError::Token(TokenError::ForeignBuffer)
    );
    assert_eq!(
        other.remove::<u32>(2, &token),
        Err(EnqueueError::Token(TokenError::ForeignBuffer))
    );
    assert_eq!(
        other.despawn(2, &token),
        Err(EnqueueError::Token(TokenError::ForeignBuffer))
    );
    commands.reset().unwrap();
    let fresh = spawn(&mut commands, 1, 1);
    assert_ne!(fresh, token);
    assert_eq!(commands.take_spawn(&token), Err(TokenError::StaleEpoch));
    assert_eq!(
        commands.despawn(3, &token),
        Err(EnqueueError::Token(TokenError::StaleEpoch))
    );
    drop(commands);
    for _ in 0..64 {
        let mut replacement = EcsCommandBuffer::new(2, 1);
        assert_eq!(
            replacement.despawn(1, &fresh),
            Err(EnqueueError::Token(TokenError::ForeignBuffer))
        );
    }
}

#[test]
fn stale_existing_targets_fail_without_touching_recycled_entities_and_retain_insert_input() {
    let mut world = prepared(2);
    let old = world.spawn((1_u32,));
    world.despawn(old).unwrap();
    let replacement = world.spawn((2_u32,));
    assert_eq!(old.id(), replacement.id());
    let mut commands = EcsCommandBuffer::new(3, 0);
    commands.insert(1, old, (99_u32,)).unwrap();
    commands.remove::<u32>(2, old).unwrap();
    commands.despawn(3, old).unwrap();
    let mut errors = Vec::new();
    let report = commands
        .apply_report(&mut world, 3, POLICIES[1], |e| errors.push(e))
        .unwrap();
    assert_eq!((report.commands.failed, report.commands.applied), (3, 0));
    assert!(report.accepted().is_empty());
    for error in &errors {
        assert_eq!(
            error.error.kind,
            EcsCommandErrorKind::Ecs(Error::NoSuchEntity(old))
        );
    }
    assert_eq!(
        errors.remove(0).error.into_bundle::<(u32,)>().unwrap(),
        (99,)
    );
    assert_eq!(*world.get::<&u32>(replacement).unwrap(), 2);
    assert!(world.contains(replacement));
}

#[test]
fn missing_and_duplicate_component_errors_use_checked_ecs_semantics() {
    let mut world = prepared(2);
    let entity = world.spawn((4_u32,));
    let mut commands = EcsCommandBuffer::new(3, 0);
    commands.remove::<bool>(1, entity).unwrap();
    commands.insert(2, entity, (5_u32, 6_u32)).unwrap();
    commands.insert(3, entity, (true,)).unwrap();
    let mut failures = Vec::new();
    let report = commands
        .apply_report(&mut world, 3, POLICIES[1], |e| failures.push(e))
        .unwrap();
    assert_eq!((report.commands.failed, report.commands.applied), (2, 1));
    assert!(matches!(
        failures[0].error.kind,
        EcsCommandErrorKind::Ecs(Error::MissingComponent(_))
    ));
    assert!(matches!(
        failures[1].error.kind,
        EcsCommandErrorKind::Ecs(Error::DuplicateComponent(_))
    ));
    assert!(
        failures
            .pop()
            .unwrap()
            .error
            .into_bundle::<(u32, u32)>()
            .is_err()
    );
    assert_eq!(*world.get::<&u32>(entity).unwrap(), 4);
    assert!(world.has::<bool>(entity));
}

struct Probe {
    drops: Rc<Cell<usize>>,
    panic: bool,
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        assert!(!self.panic, "component destructor panic");
    }
}
fn probe(drops: &Rc<Cell<usize>>, panic: bool) -> Probe {
    Probe {
        drops: Rc::clone(drops),
        panic,
    }
}

#[test]
fn failed_spawn_returns_nonclone_bundle_and_dependencies_cannot_target_another_entity() {
    for policy in POLICIES {
        let mut world = World::new(); // Deliberately unprepared.
        let live = world.spawn((1_u32,));
        let drops = Rc::new(Cell::new(0));
        let mut commands = EcsCommandBuffer::new(4, 1);
        let token = commands
            .spawn(1, (probe(&drops, false),))
            .unwrap_or_else(|_| panic!());
        commands
            .insert(2, &token, (probe(&drops, false),))
            .unwrap_or_else(|_| panic!());
        commands.despawn(3, &token).unwrap();
        commands.insert(4, live, (9_u32,)).unwrap();
        let mut errors = Vec::new();
        let report = commands
            .apply_report(&mut world, 4, policy, |e| errors.push(e))
            .unwrap();
        assert_eq!(
            report.commands.failed,
            if policy == POLICIES[0] { 1 } else { 3 }
        );
        assert_eq!(
            report.commands.remaining,
            if policy == POLICIES[0] { 3 } else { 0 }
        );
        assert!(matches!(
            errors[0].error.kind,
            EcsCommandErrorKind::Spawn(SpawnError::EntityCapacity { .. })
        ));
        assert_eq!(
            commands.take_spawn(&token),
            Err(TokenError::DependencyFailed)
        );
        assert_eq!(
            commands.despawn(9, &token),
            Err(EnqueueError::Token(TokenError::DependencyFailed))
        );
        assert_eq!(drops.get(), 0);
        // Wrong downcast preserves the owned error and bundle.
        let error = errors.remove(0).error.into_bundle::<(u32,)>().unwrap_err();
        let owned = error.into_bundle::<(Probe,)>().unwrap();
        assert_eq!(drops.get(), 0);
        drop(owned);
        assert_eq!(drops.get(), 1);
        if policy == POLICIES[0] {
            assert_eq!(*world.get::<&u32>(live).unwrap(), 1);
            commands
                .apply_report(&mut world, 4, POLICIES[1], |e| errors.push(e))
                .unwrap();
        }
        assert_eq!(errors.iter().map(|e| e.id).collect::<Vec<_>>(), [2, 3]);
        for error in &errors {
            assert_eq!(
                error.error.kind,
                EcsCommandErrorKind::Token(TokenError::DependencyFailed)
            );
        }
        drop(errors.remove(0).error.into_bundle::<(Probe,)>().unwrap());
        assert_eq!(drops.get(), 2);
        assert_eq!(*world.get::<&u32>(live).unwrap(), 9);
        assert_eq!(world.len(), 1);
    }
}

#[test]
fn preparation_and_duplicate_spawn_errors_preserve_inputs_and_allow_explicit_new_spawn() {
    let mut world = prepared(1);
    let mut commands = EcsCommandBuffer::new(2, 2);
    let bad = commands.spawn(1, (1_u32, 2_u32)).unwrap();
    let missing = commands.spawn(2, (String::from("owned"),)).unwrap();
    let mut errors = Vec::new();
    commands
        .apply_report(&mut world, 2, POLICIES[1], |e| errors.push(e))
        .unwrap();
    assert!(matches!(
        errors[0].error.kind,
        EcsCommandErrorKind::Spawn(SpawnError::DuplicateComponent(_))
    ));
    assert!(matches!(
        errors[1].error.kind,
        EcsCommandErrorKind::Spawn(SpawnError::ComponentCapacity { .. })
    ));
    assert!(world.is_empty());
    let (_, owned) = (
        errors.remove(0),
        errors.remove(0).error.into_bundle::<(String,)>().unwrap(),
    );
    assert_eq!(commands.take_spawn(&bad), Err(TokenError::DependencyFailed));
    assert_eq!(
        commands.take_spawn(&missing),
        Err(TokenError::DependencyFailed)
    );
    commands.reset().unwrap();
    world.prepare_component::<String>(1).unwrap();
    let retry = commands.spawn(3, owned).unwrap(); // New command/token, never auto-retry.
    let entity = commands
        .apply_report(&mut world, 1, POLICIES[0], |_| unreachable!())
        .unwrap()
        .take_spawn(&retry)
        .unwrap();
    assert_eq!(&**world.get::<&String>(entity).unwrap(), "owned");
    assert_eq!(commands.take_spawn(&missing), Err(TokenError::StaleEpoch));
}

#[test]
fn stop_continue_policies_at_every_error_position_respect_short_and_zero_budgets() {
    for policy in POLICIES {
        for failure_index in 0..3 {
            for budget in [0, 1, 2, 3, usize::MAX] {
                let mut world = prepared(1);
                let entity = world.spawn((0_u32,));
                let mut commands = EcsCommandBuffer::new(3, 0);
                for index in 0..3 {
                    if index == failure_index {
                        commands.remove::<bool>(index as u64, entity).unwrap();
                    } else {
                        commands
                            .insert(index as u64, entity, (index as u32 + 1,))
                            .unwrap();
                    }
                }
                let mut failed = [None; 1];
                let count = budget.min(3);
                let started = if policy == POLICIES[0] && count > failure_index {
                    failure_index + 1
                } else {
                    count
                };
                let report = commands
                    .apply_report(&mut world, budget, policy, |e| failed[0] = Some(e.id))
                    .unwrap();
                let failures = usize::from(started > failure_index);
                assert_eq!(
                    report.commands,
                    FallibleCommandApplyOutcome {
                        started,
                        applied: started - failures,
                        failed: failures,
                        remaining: 3 - started
                    }
                );
                assert_eq!(report.accepted().len(), started - failures);
                assert_eq!(failed[0], (failures > 0).then_some(failure_index as u64));
                commands
                    .apply_report(&mut world, usize::MAX, POLICIES[1], |_| {})
                    .unwrap();
                assert!(commands.is_empty());
                assert_eq!(
                    *world.get::<&u32>(entity).unwrap(),
                    if failure_index == 2 { 2 } else { 3 }
                );
            }
        }
    }
}

#[test]
fn zero_and_full_capacities_reject_without_consuming_payloads_and_retries_append() {
    let drops = Rc::new(Cell::new(0));
    let mut zero = EcsCommandBuffer::new(0, 0);
    assert_eq!(
        (
            zero.capacity(),
            zero.remaining_capacity(),
            zero.spawn_capacity(),
            zero.remaining_spawn_capacity()
        ),
        (0, 0, 0, 0)
    );
    let rejected = zero.spawn(0, (probe(&drops, false),)).err().unwrap();
    assert_eq!(rejected.reason, EnqueueError::CommandCapacity);
    assert_eq!(drops.get(), 0);
    let mut map_zero = EcsCommandBuffer::new(1, 0);
    assert_eq!(
        map_zero.spawn(0, ()).unwrap_err().reason,
        EnqueueError::SpawnCapacity
    );
    let mut world = prepared(2);
    world.prepare_component::<Probe>(2).unwrap();
    let mut commands = EcsCommandBuffer::new(1, 2);
    let first = spawn(&mut commands, 1, 1);
    let insert = commands.insert(2, &first, (true,)).unwrap_err();
    assert_eq!(insert.reason, EnqueueError::CommandCapacity);
    commands
        .apply_report(&mut world, 1, POLICIES[0], |_| unreachable!())
        .unwrap();
    let second = commands
        .spawn(3, rejected.bundle)
        .unwrap_or_else(|_| panic!());
    commands
        .apply_report(&mut world, 1, POLICIES[0], |_| unreachable!())
        .unwrap();
    commands.insert(2, &first, insert.bundle).unwrap();
    commands
        .apply_report(&mut world, 1, POLICIES[0], |_| unreachable!())
        .unwrap();
    let first_entity = commands.take_spawn(&first).unwrap();
    let second_entity = commands.take_spawn(&second).unwrap();
    assert!(world.has::<bool>(first_entity));
    assert!(world.has::<Probe>(second_entity));
    assert_eq!(drops.get(), 0);
    world.despawn(second_entity).unwrap();
    assert_eq!(drops.get(), 1);
}

#[test]
fn reset_cancels_queued_work_once_without_despawning_accepted_entities() {
    let drops = Rc::new(Cell::new(0));
    let mut world = prepared(2);
    let mut commands = EcsCommandBuffer::new(3, 3);
    let accepted = spawn(&mut commands, 1, 1);
    commands
        .apply_report(&mut world, 1, POLICIES[0], |_| unreachable!())
        .unwrap();
    let pending = commands
        .spawn(2, (probe(&drops, false),))
        .unwrap_or_else(|_| panic!());
    commands
        .insert(3, &accepted, (probe(&drops, false),))
        .unwrap_or_else(|_| panic!());
    commands.despawn(4, &accepted).unwrap();
    commands.reset().unwrap();
    assert_eq!(drops.get(), 2);
    assert_eq!(world.len(), 1);
    assert!(commands.last_accepted().is_empty());
    assert_eq!(commands.remaining_spawn_capacity(), 3);
    assert_eq!(commands.take_spawn(&pending), Err(TokenError::StaleEpoch));
    assert_eq!(commands.take_spawn(&accepted), Err(TokenError::StaleEpoch));
    commands.reset().unwrap();
    assert_eq!(drops.get(), 2);
    let third = commands
        .spawn(5, (probe(&drops, false),))
        .unwrap_or_else(|_| panic!());
    drop(commands); // Dropping does not invoke queued commands.
    assert_eq!(drops.get(), 3);
    assert_eq!(world.len(), 1);
    drop(third);
}

#[test]
fn callback_panic_preserves_mappings_success_log_and_unvisited_tail() {
    for policy in POLICIES {
        let mut world = prepared(2);
        let mut commands = EcsCommandBuffer::new(3, 1);
        let token = spawn(&mut commands, 1, 1);
        commands.remove::<bool>(2, &token).unwrap();
        commands.insert(3, &token, (true,)).unwrap();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _ = commands.apply_report(&mut world, 3, policy, |_| panic!("callback"));
            }))
            .is_err()
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands
                .last_accepted()
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            [1]
        );
        let entity = commands.take_spawn(&token).unwrap();
        assert!(!world.has::<bool>(entity));
        commands
            .apply_report(&mut world, 3, policy, |_| unreachable!())
            .unwrap();
        assert!(world.has::<bool>(entity));
        assert_eq!(commands.take_spawn(&token), Err(TokenError::AlreadyTaken));
    }
}

#[test]
fn component_drop_panics_consume_only_current_command_without_claiming_success() {
    for policy in POLICIES {
        for operation in 0..3 {
            let drops = Rc::new(Cell::new(0));
            let mut world = prepared(3);
            let victim = world.spawn((probe(&drops, true),));
            let mut commands = EcsCommandBuffer::new(3, 2);
            let first = spawn(&mut commands, 1, 1);
            match operation {
                0 => commands.remove::<Probe>(2, victim).unwrap(),
                1 => commands.despawn(2, victim).unwrap(),
                _ => commands
                    .insert(2, victim, (probe(&drops, false),))
                    .unwrap_or_else(|_| panic!()),
            }
            let last = spawn(&mut commands, 3, 3);
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = commands
                        .apply_report(&mut world, 3, policy, |_| panic!("not a reported error"));
                }))
                .is_err()
            );
            assert_eq!(commands.len(), 1);
            assert_eq!(commands.last_accepted().len(), 1);
            assert!(commands.take_spawn(&first).is_ok());
            assert_eq!(commands.take_spawn(&last), Err(TokenError::Pending));
            commands
                .apply_report(&mut world, 3, policy, |_| unreachable!())
                .unwrap();
            assert!(commands.take_spawn(&last).is_ok());
            assert!(drops.get() >= 1);
        }
    }
}

#[test]
fn panicking_reset_disables_old_tail_until_explicit_recovery() {
    let mut world = prepared(2);
    let drops = Rc::new(Cell::new(0));
    let mut commands = EcsCommandBuffer::new(3, 2);
    let token = commands
        .spawn(1, (probe(&drops, true),))
        .unwrap_or_else(|_| panic!());
    commands.despawn(2, &token).unwrap();
    commands
        .spawn(3, (probe(&drops, false),))
        .unwrap_or_else(|_| panic!());
    assert!(catch_unwind(AssertUnwindSafe(|| commands.reset())).is_err());
    assert_eq!(drops.get(), 1);
    assert_eq!(commands.len(), 2);
    assert_eq!(
        commands.take_spawn(&token),
        Err(TokenError::ResetIncomplete)
    );
    assert!(matches!(
        commands.apply_report(&mut world, 9, POLICIES[1], |_| unreachable!()),
        Err(BufferError::ResetIncomplete)
    ));
    assert_eq!(
        commands
            .insert(4, Entity::from_bits(0), (1_u32,))
            .unwrap_err()
            .reason,
        EnqueueError::ResetIncomplete
    );
    assert_eq!(
        commands.spawn(5, ()).unwrap_err().reason,
        EnqueueError::ResetIncomplete
    );
    assert_eq!(
        commands.despawn(6, &token),
        Err(EnqueueError::ResetIncomplete)
    );
    commands.reset().unwrap();
    assert_eq!(drops.get(), 2);
    assert!(commands.is_empty());
    assert_eq!(commands.take_spawn(&token), Err(TokenError::StaleEpoch));
    assert!(world.is_empty());
    let fresh = spawn(&mut commands, 7, 7);
    commands
        .apply_report(&mut world, 1, POLICIES[1], |_| unreachable!())
        .unwrap();
    assert!(commands.take_spawn(&fresh).is_ok());
}

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct CountingAllocator;
fn record_allocation() {
    let _ = RECORD.try_with(|record| {
        if record.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get().saturating_add(1)));
        }
    });
}
// SAFETY: Every allocation/deallocation forwards unchanged to System. The
// instrumentation is allocation-free, thread-local and tolerates TLS teardown.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
fn allocations_during(run: impl FnOnce()) -> usize {
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            RECORD.with(|record| record.set(false));
        }
    }
    ALLOCATIONS.with(|count| count.set(0));
    RECORD.with(|record| record.set(true));
    let stop = Stop;
    run();
    drop(stop);
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn allocator_instrumentation_positive_control() {
    assert!(
        allocations_during(|| {
            black_box(Box::new(black_box(42_u64)));
        }) > 0
    );
}

#[test]
fn prepared_apply_map_claim_errors_rejection_and_reset_add_no_allocations() {
    let mut world = prepared(4);
    let mut commands = EcsCommandBuffer::new(6, 2);
    let first = spawn(&mut commands, 1, 1);
    commands.insert(2, &first, (true,)).unwrap();
    commands.remove::<bool>(3, &first).unwrap();
    commands.remove::<bool>(4, &first).unwrap(); // Missing component error.
    let second = spawn(&mut commands, 5, 5);
    commands.despawn(6, &first).unwrap();
    let count = allocations_during(|| {
        assert_eq!(
            commands
                .insert(9, &first, (black_box(99_u32),))
                .unwrap_err()
                .reason,
            EnqueueError::CommandCapacity
        );
        let report = commands
            .apply_report(&mut world, 0, POLICIES[0], |_| unreachable!())
            .unwrap();
        assert_eq!(report.commands.started, 0);
        let mut report = commands
            .apply_report(&mut world, 2, POLICIES[0], |_| unreachable!())
            .unwrap();
        black_box(report.take_spawn(&first).unwrap());
        let mut error_id = None;
        let report = commands
            .apply_report(&mut world, 9, POLICIES[0], |e| error_id = Some(e.id))
            .unwrap();
        assert_eq!(report.commands.remaining, 2);
        assert_eq!(error_id, Some(4));
        let mut report = commands
            .apply_report(&mut world, 9, POLICIES[1], |_| unreachable!())
            .unwrap();
        black_box(report.take_spawn(&second).unwrap());
        assert_eq!(report.commands.applied, 2);
        commands.reset().unwrap();
    });
    assert_eq!(count, 0);
    // Owned spawn failure recovery uses the existing command box, not a new box.
    let mut world = World::new();
    let token = commands.spawn(7, (black_box(12_u32),)).unwrap();
    commands.insert(8, &token, (black_box(13_u32),)).unwrap();
    let count = allocations_during(|| {
        let mut recovered = [None; 2];
        let mut index = 0;
        commands
            .apply_report(&mut world, 2, POLICIES[1], |failure| {
                recovered[index] = Some(failure.error.into_bundle::<(u32,)>().unwrap().0);
                index += 1;
            })
            .unwrap();
        assert_eq!(recovered, [Some(12), Some(13)]);
        commands.reset().unwrap();
    });
    assert_eq!(count, 0);
}

#[test]
fn resolved_tokens_remain_stale_after_despawn_and_slot_reuse() {
    let mut world = prepared(1);
    let mut commands = EcsCommandBuffer::new(6, 2);
    let old_token = spawn(&mut commands, 1, 1);
    commands.despawn(2, &old_token).unwrap();
    let replacement_token = spawn(&mut commands, 3, 2);
    commands.insert(4, &old_token, (99_u32,)).unwrap();
    commands.remove::<u32>(5, &old_token).unwrap();
    commands.despawn(6, &old_token).unwrap();
    let mut failures = Vec::new();
    let mut report = commands
        .apply_report(&mut world, 6, POLICIES[1], |e| failures.push(e))
        .unwrap();
    assert_eq!((report.commands.applied, report.commands.failed), (3, 3));
    let old = report.take_spawn(&old_token).unwrap();
    let replacement = report.take_spawn(&replacement_token).unwrap();
    assert_eq!(old.id(), replacement.id());
    assert_ne!(old, replacement);
    for failure in failures {
        assert_eq!(
            failure.error.kind,
            EcsCommandErrorKind::Ecs(Error::NoSuchEntity(old))
        );
    }
    assert_eq!(*world.get::<&u32>(replacement).unwrap(), 2);
}

#[test]
fn failed_spawn_followed_by_success_has_separate_mapping_and_dependency_state() {
    let mut world = prepared(1);
    let mut commands = EcsCommandBuffer::new(3, 2);
    let failed = commands.spawn(1, (String::from("unprepared"),)).unwrap();
    let accepted = spawn(&mut commands, 2, 9);
    commands.despawn(3, &failed).unwrap();
    let mut failures = [None, None];
    let mut index = 0;
    let mut report = commands
        .apply_report(&mut world, 3, POLICIES[1], |e| {
            failures[index] = Some(e.id);
            index += 1;
        })
        .unwrap();
    assert_eq!(report.commands.failed, 2);
    assert_eq!(
        report.take_spawn(&failed),
        Err(TokenError::DependencyFailed)
    );
    let entity = report.take_spawn(&accepted).unwrap();
    assert_eq!(failures, [Some(1), Some(3)]);
    assert_eq!(*world.get::<&u32>(entity).unwrap(), 9);
}
