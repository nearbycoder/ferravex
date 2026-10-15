# ferravex-dynamics

An original Rust physics foundation with **zero external dependencies**. It uses the standard library and internal array arithmetic. No external solver implementation is bundled.

This is a deliberately limited translational solver. Angular motion, joints and mesh terrain remain future work.

```rust
use ferravex_dynamics::{BodyDesc, PhysicsWorld, Shape};
let mut world = PhysicsWorld::<3>::new([0.0, -9.81, 0.0]);
world.insert(BodyDesc::fixed(Shape::cuboid([20.0, 0.5, 20.0]), [0.0, -0.5, 0.0]))?;
let ball = world.insert(BodyDesc::dynamic(Shape::ball(0.5), [0.0, 3.0, 0.0], 2.0))?;
world.body_mut(ball).unwrap().apply_impulse([2.0, 0.0, 0.0])?;
world.step(1.0 / 120.0)?;
# Ok::<(), ferravex_dynamics::PhysicsError>(())
```

Use `PhysicsWorld<2>` for circles and rectangles, or `PhysicsWorld<3>` for spheres and boxes. All boxes are axis-aligned and use **half extents**. The `d2` and `d3` modules provide type aliases. Shape and motion values are plain `[f32; D]` arrays; no math crate is required.

Implemented behavior:

- Dynamic, static and velocity-driven kinematic bodies; validated mass, gravity scale, damping, force and impulse inputs.
- Circle/sphere, box/box and ball/box narrowphase with contact normals and penetration. A sequential impulse solver combines restitution, Coulomb friction and mass-weighted positional correction. Low-speed contacts suppress restitution so resting objects settle.
- A generation-checked handle arena; removal invalidates all old handles. A generation that would wrap retires its slot.
- Sweep-and-prune broadphase with an adaptive axis, deterministic pair order, bounded candidate count and an independent bound on broadphase checks. It is not a spatial tree and still has quadratic worst cases, which stop at the configured work limit.
- Overlap contact lists, sensor contacts, Started/Stopped transitions, and symmetric collision-layer filters. Removing a contacting body emits one Stopped transition on the next successful step.
- Immediate filtered raycasts and exact sphere/circle casts against boxes, including rounded corners and edges. Query distances are in world units; direction vectors are normalized.
- Opt-in continuous ball collision against **static, solid terrain**. Multiple impacts consume a fixed per-body budget; remaining motion is conservatively discarded if that budget is exhausted. CCD does not cover moving-vs-moving collisions, moving terrain, box casts, or transient sensor crossings.
- A kinematic sphere movement helper with bounded sweep/slide, low-step climbing, downward ground snap and a surface-normal threshold. It uses positive Y as up, does not insert bodies, and does not push dynamic objects or depenetrate invalid spawns. Sloped normals are supported, but available terrain shapes remain spheres and AABBs.

Forces apply for one successful step and then clear. A failed step rolls back all positions and velocities, leaves forces available for retry, and preserves the previous contact/event state. Capacity failures are errors; colliding pairs are not silently discarded. Scratch vectors are reused after their initial growth. The caller provides a fixed timestep; `step` accepts finite durations in `(0, 0.1]` seconds and does not read the clock.

Limits are explicit in `WorldConfig`. Defaults allow 4,096 body slots, 65,536 candidate pairs, 2,000,000 broadphase checks, 12 solver iterations and four CCD impacts per ball per step. Body/query coordinates and velocity components are bounded to ±1,000,000; normal game-scale units and 120 Hz are recommended. `StepStats` exposes pair work, contacts, CCD impacts and budget stops. Results are repeatable for the same executable and insertion order; cross-architecture bitwise determinism is not promised.

**Not implemented:** angular velocity/torque/inertia, rotating colliders, oriented boxes, capsules, arbitrary convex or triangle meshes, joints, constraints beyond contacts, ragdolls, sleeping islands, warm starting, continuous moving-body pairs, a persistent query acceleration tree, parallel solving, rollback serialization, or physics replication. Friction changes linear velocity; it does not simulate rolling. The crate has no allocator-free or production stability claim.

Verification:

```sh
cargo test -p ferravex-dynamics
cargo clippy -p ferravex-dynamics --all-targets -- -D warnings
cargo check -p ferravex-dynamics --target wasm32-unknown-unknown
cargo run --release -p ferravex-dynamics --example benchmark
cargo tree -p ferravex-dynamics
```

Twenty behavior tests and a doctest exercise 2D/3D settling, six-box stacks, impulse/force mass semantics, momentum exchange, friction/restitution, handle reuse, layers, live raycasts, sensor transitions/removal, kinematics, exact corner/edge sweeps, thin-wall CCD, character sliding/stairs, invalid inputs, bounded broadphase work, rollback and 2,000 separated bodies.
