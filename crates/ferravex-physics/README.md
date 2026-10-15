# ferravex-physics

Optional Rapier 2D and 3D physics for Ferravex. The wrapper owns the simulation and
provides short constructors without concealing Rapier's bodies, colliders,
queries, joints, collision hooks, or event handlers.

```rust
use ferravex_physics::d3::{PhysicsWorld, ColliderBuilder, RigidBodyBuilder, Vector};

let mut physics = PhysicsWorld::new([0.0, -9.81, 0.0]);
physics.add_box([0.0, -0.5, 0.0], [20.0, 0.5, 20.0], false);
let ball = physics.add_ball([0.0, 4.0, 0.0], 0.3, true);
physics.bodies[ball].enable_ccd(true);

// The underlying builders retain materials, rotation, mass, CCD, and damping.
physics.insert(
    RigidBodyBuilder::dynamic().translation(Vector::new(2.0, 4.0, 0.0)),
    ColliderBuilder::ball(0.5).restitution(0.8).friction(0.6),
);

physics.step(1.0 / 60.0);
let position = physics.bodies[ball].translation().to_array();
```

The `d2` module exposes the same API with two-component arrays. Both modules
re-export the Rapier prelude and the complete dependency as `d2::rapier` or
`d3::rapier`. `PhysicsWorld::raw` permits borrowing separate Rapier fields when
implementing controllers or more advanced systems.

## Tick and query rules

- Call `step` at a fixed timestep. Render interpolation belongs to the game loop.
- Distances and gravity use world units; meters are a practical default.
- `add_box` takes half extents. `dynamic = false` creates a fixed body.
- CCD is opt-in for fast bodies; sleeping and the regular solver use Rapier defaults.
- `raycast` normalizes its direction and returns distance in world units. Use
  `raycast_with_filter` to exclude the shooter or apply collision groups.
- After insertion, removal, or manual transform edits, call `refresh_queries`
  before querying, or advance one simulation tick. Queries use the latest
  collision detection state.
- `step_with_events(dt, hooks, events)` forwards custom handlers. Enable Rapier
  `ActiveEvents` on colliders that should emit notifications.

## Feature selection

```toml
ferravex-physics = { path = "../ferravex-physics", default-features = false, features = ["dim3"] }
```

Both dimensions are enabled by default. An optional `deterministic` feature
forwards Rapier's enhanced-determinism mode only to the dimensions already
enabled. It does not make variable-timestep simulation deterministic and does
not replace authoritative networking. No renderer or OS-specific dependencies
are introduced by this crate.

Run `cargo test -p ferravex-physics` for 2D and 3D collision, impulse, raycast,
removal, and continuous-collision checks.
