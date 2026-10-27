# ferravex-ecs

Original entity/component storage using only Rust's standard library. The crate forbids unsafe code and has no dependencies.

```rust
use ferravex_ecs::{Entity, World};
let mut world = World::new();
let ship = world.spawn((100_i32, 2.5_f32));
for (entity, mut health, speed) in &mut world.query::<(Entity, &mut i32, &f32)>() {
    assert_eq!(entity, ship);
    *health -= *speed as i32;
}
```

Supported operations include generational entities; zero-to-four-component tuple spawn/insert; typed removal; shared/mutable component guards; matching queries with up to four accesses, including `Entity`; and live-entity iteration. Mutable query bindings use `mut` because results are `RefMut<T>` guards. `get::<&T>` and `get::<&mut T>` preserve the existing Ferravex example call shape; `get_mut::<T>` is also available.

Use `get_many::<Q, N>([entities...])` to borrow known entities together without scanning the world. It returns a fixed array in the supplied order, using the same component or tuple signatures as queries. All requested borrows must succeed before any guards reach the caller:

```rust
use ferravex_ecs::{Entity, World};
let mut world = World::new();
let donor = world.spawn((10_i32, "donor"));
let receiver = world.spawn((2_i32, "receiver"));
{
    let [(from_id, mut from), (to_id, mut to)] = world
        .get_many::<(Entity, &mut i32), 2>([donor, receiver])
        .unwrap();
    assert_eq!((from_id, to_id), (donor, receiver));
    *from -= 3;
    *to += 3;
}
assert_eq!(*world.get::<&i32>(receiver).unwrap(), 5);
```

`get_many` validates the query signature first (`ConflictingQuery` for mutable aliases within a tuple), then every entity in input order (`NoSuchEntity` for invalid or stale generations), then borrows components in entity and tuple order. A missing component returns `MissingComponent`; incompatible existing guards or duplicate entities requesting mutable access return `BorrowConflict`. Repeated entities are valid for shared or identity-only access. Failure releases all guards acquired by that call, including partial tuples, without changing components. Empty arrays succeed for valid signatures. The helper uses no heap allocation, requires stack space proportional to the array size, and performs only the requested lookups; it does not scan entity slots or compare every pair of handles. Use a one-element array to retrieve several components of a single entity. Handles remain world-local, so a matching identity from another world cannot be distinguished.

Each component type has a sparse `Vec<Option<RefCell<T>>>` indexed by entity slot. Generations invalidate deleted handles. Exhausted generations retire their slots instead of wrapping. Structural changes require exclusive access to the world, so outstanding component guards prevent deletion or storage relocation at compile time. Per-component guards reject incompatible overlapping queries at runtime. Query validation and iteration do not allocate; bundle validation currently uses a small temporary vector. Matching scans entity slots and performs typed storage lookups (keyed directly by `TypeId`, without rehashing), so this initial implementation is not an archetype/chunk scheduler and does not claim hecs-equivalent throughput. Run `cargo run --release -p ferravex-ecs --example query_bench` for a local 4096-entity query workload.

`try_spawn`, `try_query`, `try_iter`, and component access return explicit errors. Infallible query iteration panics on borrow conflicts, never silently skips a conflicted entity. Failed tuple borrows release earlier acquired guards. Despawn retires its slot if a component destructor panics before cleanup completes, preventing inherited leftover components on reuse. Handles are local to one world; do not transfer them between worlds.

Validation: unit regressions cover entity reuse, generation exhaustion, matching mutable queries, overlapping queries, guards retained beyond iterators, duplicate bundle rejection, component drops, panic recovery, partial tuple failure, insertion/removal, and 2,000 reuse cycles. Checked batch-access regressions cover ordered mutation, shared and mutable duplicates, stale handles before and after slot reuse, missing components, existing guards, rollback of partial borrows, and empty input. Runnable doctests check query and batch-access usage; a compile-fail doctest checks that structural deletion is rejected while a component guard remains live.

```sh
cargo test -p ferravex-ecs --offline --locked -j 2
cargo clippy -p ferravex-ecs --all-targets --offline --locked -j 2 -- -D warnings
```
