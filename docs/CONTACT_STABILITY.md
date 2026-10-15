# Persistent contacts and sleep islands

`ferravex-physics` now retains contact anchors and impulses across substeps and
frames. The rotating rigid solver uses the same implementation in native and
Wasm builds. Marble Run, Salvage Pinball, Siege Foundry and both physics labs
adopt it through their existing PhysicsWorld calls; no game-specific solver fork
or third-party physics dependency is involved.

## Contact lifetime and warm starting

Collider pairs have canonical handle order, independent of their order along the
broadphase X axis. Each manifold stores up to eight contact points with monotonically
assigned identities and local anchors on both bodies. Fresh geometry is matched
one-to-one against nearby anchors with aligned normals. The defaults are a 0.05-unit
anchor matching distance and a normal dot product above 0.95.

Warm starting applies the previous normal/tangent impulse before the sequential
velocity solve. Tangent impulses are projected into the current contact plane and
clamped to the friction cone. The cone is re-clamped even at zero slip if the normal
load decreases. Cached impulses scale with the substep duration when its ratio is
between 0.5 and 2; larger changes start cold. Energetic impacts and fast separating
contacts also start cold so an old support/bounce impulse is not replayed.

Close separated anchors may retain their identity within `contact_break_distance`
(default 0.02), but have zero support/friction impulse and do not emit an active
ContactPair. Existing begin/end events remain the union of observed overlaps in
the outer step, including transient substep and sensor overlaps. Sensors never
receive impulses, enter the manifold cache, or join sleep islands.

External body pose and gravity-scale edits, collider friction/restitution/sensor/layer
edits, removed generations and changed world gravity invalidate affected cached
impulses. Removing a body immediately discards its cache entries. Inserting a cloned body
creates fresh collider ownership and wakes it; removing the clone cannot remove
the original body's colliders or contacts. The mutation
APIs also wake affected previous contact neighbors before the next integration.
Velocity/force changes still use the physical solver with the requested wake flag.
Read-only `contact_manifolds()` exposes last-substep point identities, anchors and
impulses for diagnostics; a pose edit invalidates those observations on the next step.

## Connected sleep and wake

Dynamic bodies connected by solid contacts form deterministic islands. A static
floor is a boundary, so independent stacks resting on it remain separate islands.
Explicit wake requests, joining an awake island, moving kinematic boundaries,
mutated supports and new impacts propagate wake through connected dynamic bodies.
The previous contact graph handles removed/moved supports before integration;
new contacts handle impacts before velocity solving.

All members must be quiet before an island sleeps. Default thresholds are 0.04
units/second linear speed, 0.05 radians/second angular speed and 0.8 seconds of
shared quiet time. Applied force/torque and moving kinematic contacts prevent
sleep. Islands under gravity also require support against gravity from a fixed
or stationary kinematic boundary. Quiet free bodies can sleep in zero gravity.
Sleep freezes the complete island; warm starting and constraint iterations skip
sleeping pairs. Their unchanged anchors can be refreshed without running narrowphase
geometry again. The character motor is a separate kinematic query system.

## Configuration and diagnostics

```rust
use ferravex_physics::{SolverSettings, d3::PhysicsWorld};
let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
world.set_solver_settings(SolverSettings {
    velocity_iterations: 12,
    position_iterations: 8,
    warm_start: true,
    allow_sleep: true,
    max_cached_manifolds: 2048,
    ..Default::default()
})?;
# Ok::<(), &'static str>(())
```

Those are also the defaults. Changing settings validates them transactionally,
clears cached impulses and wakes the world; reapplying identical settings is a no-op.
Velocity iterations accept 1–64 and positional iterations 0–32. Tune for scene
scale, mass ratios and workload rather than assuming every count is equivalent.
A twelve-box stress case remains upright at eight velocity/eight positional
iterations; four velocity iterations retain more residual motion and are not
claimed equivalent to the default quality.

The cache grows lazily into reused double buffers. At its default 2048-manifold
limit, eight points per pair occupy approximately 2.3 MiB across both full buffers
on this Linux 64-bit build; ordinary small scenes allocate less. Other world,
contact/constraint and broadphase storage is additional. A full cache falls back
to cold solving for extra pairs, counting them in `cache_dropped`; no physical
contacts are omitted. Setting the limit to zero disables retention. The limit
bounds the cache, not total dense-scene collision work or all world memory.

`StepStats` includes persisted/warm-started point counts, narrowphase reuse,
cache drops, velocity-constraint visits, final cached manifold count, island counts
and sleeping bodies. Work counters sum substeps; island/cache/body counts describe
the final substep. A steady four-box scene makes zero successful Rust allocations
per step after warmup, both awake and sleeping, as checked by a thread-local
allocation regression test. New contacts or larger scenes can grow scratch buffers.

## Verification and limits

```sh
cargo test -p ferravex-physics --offline --locked
cargo run --release -p ferravex-physics --example rigid_bench --offline --locked
cargo run --release -p ferravex-physics --example rigid_bench --offline --locked -- --cold --no-sleep
```

The benchmark reports JSON timing quantiles, first/last 120-tick cost, actual
constraint visits, cache reuse and sleep counts for the existing 128-box scene.
Use `--iterations N`, `--no-sleep`, `--cold` and `--ticks N` for controlled local
comparisons. Tests cover tall-stack warm/cold behavior, point identities and
separation, canonical normal ordering, independent islands, removed/teleported
supports, mutation invalidation, friction limits, changing timesteps, bounded-cache
fallback, allocation reuse and the existing impact/rolling/CCD scenarios.

The supported rigid shape matrix is unchanged. This is a sequential impulse
solver with finite iterations; extreme mass ratios, tall thin stacks and large
steps still need appropriate settings/testing. Mesh terrain, box CCD, joints and
compound colliders are separate work. There is no cross-platform floating-point
bitwise determinism promise. See [performance](PERFORMANCE.md) for observed costs
and retained regression flags rather than inferring portable frame rates.
