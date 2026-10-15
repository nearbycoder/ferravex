# Ferravex Physics

An original Rust rigid-body solver used by Marble Run, Salvage Pinball, Siege Foundry, and the 2D/3D physics laboratories. Its dependencies are the workspace's original `ferravex-math` and `ferravex-core` (local telemetry). It contains no Rapier, Parry, nalgebra, glam, or other third-party physics code.

This is separate from `ferravex-dynamics`, the smaller translational array-based solver. This crate implements orientation, angular momentum response, local mass moments transformed into world space, and angular contact velocities.

## Supported behavior

| Shape pair | 2D | 3D |
| --- | --- | --- |
| Circle/circle or sphere/sphere | Dynamic, fixed, kinematic | Dynamic, fixed, kinematic |
| Ball/oriented box | Dynamic, fixed, kinematic | Dynamic, fixed, kinematic |
| Oriented box/oriented box | Dynamic, fixed, kinematic | Dynamic, fixed, kinematic |
| Ball/capsule | Dynamic, fixed, kinematic capsule | Not exposed |
| Ball/capped cylinder | Not exposed | Dynamic, fixed, kinematic cylinder |

Boxes use separating axes and clipped face contact manifolds; edge contacts use closest supporting edges. Balls collide against the actual curved capsule/cylinder surfaces. Capsules and cylinders have analytical dynamic mass properties, with contacts supported only against balls. Box/capsule, box/cylinder, and capsule/cylinder combinations have no collision response.

- Density-derived primitive and compound mass, center of mass, rotated full inertia tensors and the parallel-axis theorem.
- Linear/angular impulses, impulses at a point, force/torque accumulation, gravity scale, and damping. Forces and torques clear after each outer step.
- Persistent local-anchor manifolds, warm-started accumulated normal/tangent impulses, averaged restitution, Coulomb friction, and separate angular positional correction. Restitution is suppressed below impact speed 1 to allow resting stacks.
- Position-driven kinematics calculate linear and angular velocity from the requested next pose. Contact-point angular velocity lets a pinball flipper launch the ball.
- Generation-checked body/collider/joint handles, up to 32 locally transformed collider parts per body, removal, connected sleeping and impact wake-up.
- Sweep-and-prune AABB broadphase, collision membership/filter masks, sensor overlaps, contact begin/end events, and direct current-pose raycasts. Rays exclude sensors and return the nearest solid hit. Filtered spherecasts use current compound poses, ignore all parts of an optional body and return the swept sphere center at the nearest impact; radius zero preserves ray behavior.
- Sphere/circle CCD uses signed-distance conservative sweeps against supported convex shapes. Target translation is included in the relative path. Target rotation is frozen within each substep. Adaptive substeps (maximum 8) reduce rotational motion per query; each sweep has a maximum of 40 iterations. Existing penetration goes to the contact solver. Grazing sweeps can exhaust their iteration budget; this is not a universal no-tunneling guarantee. Boxes use substeps only, not swept time of impact.

The public `d2` and `d3` adapters preserve the small builder/body/contact API used by these games. `raw` exposes this original solver, **not** a Rapier-compatible world. Fixed, hinge, distance and spring joints support connected mechanisms, motors and limits. No mesh collider, hook, ray filter callback, or external plugin API is implied. See [joints and compound bodies](../../docs/JOINTS_AND_COMPOUNDS.md) for builders, lifecycle semantics and limits.

## Using a fixed tick

```rust
use ferravex_physics::d3::{PhysicsWorld, Vector};

let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
world.add_box([0.0, -0.5, 0.0], [10.0, 0.5, 10.0], false);
let ball = world.add_ball([0.0, 2.0, 0.0], 0.5, true);
world.bodies[ball].enable_ccd(true);
world.bodies[ball].apply_impulse(Vector::new(1.0, 0.0, 0.0), true);
for _ in 0..120 {
    world.step(1.0 / 120.0);
}
assert!(world.bodies[ball].translation().y > 0.48);
```

The caller supplies its fixed accumulator (`ferravex-core::FixedClock` in the games). `step` accepts finite intervals in `(0, 0.1]` seconds and panics for invalid intervals. Builders reject invalid dimensions/density, and insertion is limited to 4096 bodies. Stale indexing panics; `bodies.get`/`colliders.get` return `None` instead. `remove_body` returns false for stale handles. There is no cross-platform bitwise determinism promise.

`contact_pair` and `contact_events` cover the union of impacts/overlaps observed during the most recent outer step, including transient substep impacts. `has_any_active_contact` excludes sensors. A removal produces the corresponding stopped event on the next step. This supports scoring a fast bumper hit even if the ball has bounced away before the end of the outer tick.

Solid dynamic contacts now form islands that sleep and wake together. Independent stacks on a shared static floor remain separate. Default quiet time is 0.8 seconds; moving/removed supports, explicit requests and impacts wake connected members. Cached anchors and impulses persist with mutation/generation invalidation and bounded cold fallback. See [contact stability](../../docs/CONTACT_STABILITY.md) for settings, diagnostics, resource costs and limits.

## Validation

`cargo test -p ferravex-physics` exercises inertia, real frictional rolling, restitution, fast ball sweeps against thin rotated geometry, layers/sensors/removal, cylinder geometry, raycasts, box edge landing, stable stacks, and impact wake/toppling. The recovered games retain their original gameplay tests: checkpoint/finish/rolling, flipper/bumper/scoring, and ballistic destruction. The laboratories also check finite simulation, floor containment, spawning caps, and reset.

Run `cargo run --release -p ferravex-physics --example rigid_bench` for a local 128-box workload. Measurements are machine-specific and do not imply a platform-wide performance guarantee. All dependency source is local to this workspace.


## Character motors

[Shared character controllers](../../docs/CHARACTERS.md) documents the first-party
upright capsule and planar rectangle motors, slopes/steps, moving supports and
jump grace/buffering. `PhysicsWorld` implements their terrain query interface with
current collider poses and generational support identities. These kinematic
queries are separate from the rigid-body contact shape matrix above. Strike,
Crystal Cavern and Emberline now use the shared motors.
