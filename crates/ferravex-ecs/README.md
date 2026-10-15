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

Each component type has a sparse `Vec<Option<RefCell<T>>>` indexed by entity slot. Generations invalidate deleted handles. Exhausted generations retire their slots instead of wrapping. Structural changes require exclusive access to the world, so outstanding component guards prevent deletion or storage relocation at compile time. Per-component guards reject incompatible overlapping queries at runtime. Query validation and iteration do not allocate; bundle validation currently uses a small temporary vector. Matching scans entity slots and performs typed storage lookups, so this initial implementation is not an archetype/chunk scheduler and does not claim hecs-equivalent throughput.

`try_spawn`, `try_query`, `try_iter`, and component access return explicit errors. Infallible query iteration panics on borrow conflicts, never silently skips a conflicted entity. Failed tuple borrows release earlier acquired guards. Despawn retires its slot if a component destructor panics before cleanup completes, preventing inherited leftover components on reuse. Handles are local to one world; do not transfer them between worlds.

Validation: 15 unit regressions cover entity reuse, generation exhaustion, matching mutable queries, overlapping queries, guards retained beyond iterators, duplicate bundle rejection, component drops, panic recovery, partial tuple failure, insertion/removal, and 2,000 reuse cycles. A runnable doctest checks API usage; a compile-fail doctest checks that structural deletion is rejected while a component guard remains live.

```sh
cargo test -p ferravex-ecs
cargo clippy -p ferravex-ecs --all-targets -- -D warnings
```
