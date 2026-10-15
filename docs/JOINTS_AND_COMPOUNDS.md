# Joints and compound colliders

Ferravex's first-party rigid solver supports fixed, hinge, distance and spring
connections in 2D and 3D. A body can own up to 32 primitive collider parts, each
with its own local translation, rotation, density, material, sensor and layer
settings. The same child poses drive contacts, raycasts, ball CCD and character
queries. Parts on the same body never collide with one another.

## Compound bodies

```rust
use ferravex_physics::d3::{PhysicsWorld, RigidBodyBuilder, ColliderBuilder};
use ferravex_math::{Vec3, Quat};
let mut world = PhysicsWorld::new([0., -9.81, 0.]);
let body = world.insert_compound(
    RigidBodyBuilder::dynamic().translation(Vec3::new(0., 3., 0.)),
    [ColliderBuilder::cuboid(0.8, 0.13, 0.13),
     ColliderBuilder::ball(0.36).translation(Vec3::X * 0.8),
     ColliderBuilder::ball(0.36).translation(-Vec3::X * 0.8)],
).unwrap();
assert_eq!(world.bodies[body].colliders().len(), 3);
```

Density determines each part's mass. Rotated inertia tensors, mass-weighted
centers and the parallel-axis theorem determine the compound's mass properties.
Circles/spheres, boxes, 2D capsules and 3D cylinders have analytical moments.
Density zero excludes a part from mass, but a dynamic body must retain positive
finite mass and invertible inertia. Sensors contribute mass unless their density
is zero. The collision shape-pair matrix remains in the physics README; adding
mass properties does not add missing capsule/box or cylinder/box contacts.

`translation()` stays at the authored body origin; `center_of_mass()` tracks the
world mass center. `linvel()` and builder `linvel` refer to the **mass center**.
Use `velocity_at_point()` for an offset part. Rotation and impulses use the full
symmetric inertia tensor, including off-diagonal terms. There is no gyroscopic
free-rotation correction or cross-platform bitwise determinism guarantee.

`attach_collider`, `remove_collider` and `set_collider_pose` validate before
mutation, preserve the authored origin and its point velocity, recompute mass,
invalidate old impulses and wake connected bodies. The last mass-bearing child
cannot be removed from a dynamic body. Fixed/kinematic anchors may have no shapes
via `insert_body`. Direct `colliders[id].set_density` changes are validated as a
batch by `refresh_mass_properties` or the next step; an invalid all-zero dynamic
mass returns an error from refresh, or panics before step integration. Fix the
configuration before advancing. Removing a body removes all its children and
attached joints; stale generational handles never refer to replacements.

## Connections

```rust
use ferravex_physics::d3::{PhysicsWorld, RigidBodyBuilder, JointBuilder};
use ferravex_physics::JointMotor;
use ferravex_math::Vec3;
let mut world = PhysicsWorld::new([0., -9.81, 0.]);
let anchor = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
let door = world.add_box([0., 2., 0.], [0.2, 1., 0.7], true);
let hinge = world.insert_joint(anchor, door, JointBuilder::hinge()
    .local_anchor1(Vec3::new(0., 3., 0.))
    .local_anchor2(Vec3::Y)
    .limits(-1.2, 1.2)
    .motor(JointMotor::position(0.9, 8., 2., 20.))).unwrap();
world.step(1. / 120.);
assert!(world.joint_angle(hinge).unwrap().is_finite());
```

| Builder | Behavior |
| --- | --- |
| `fixed()` | Locks both local anchors together and preserves relative orientation. |
| `hinge()` | Locks anchors, aligns 3D axes, permits twist, optionally limits/drives it. |
| `distance(length)` | Keeps the anchors the specified distance apart. |
| `spring(length, stiffness, damping)` | Implicit damped spring, with physical stiffness/viscous damping; permits extension under load. |

Anchors are local to authored body origins, not mass centers. Fixed and hinge
connections capture relative orientation at insertion unless `reference_rotation`
is provided. The default hinge axis is Z in 2D, Y in 3D. `axes` configures each
body's local 3D axis; axes must be finite/nonzero. Limits and motor position are
relative to the reference and restricted to [-pi, pi]; this is a single-turn
hinge, not a continuous revolution counter. Hinge motor max torque bounds impulse
by torque × substep duration. Position motors turn angle error into a target
speed, clamped by max speed; limits clamp their position target too.

`world.joints[id].set_motor`, `set_limits`, `set_enabled` and
`set_contacts_enabled` update live mechanisms. Repeating the same value does not
wake/reset it. Changed values clear cached impulses and wake connected islands
on the next step. `joint_angle` and `joint_anchors` expose diagnostics. Removing
a joint wakes its endpoints. Disabling a joint also restores pair collisions.
Multiple active joints between the same two bodies suppress collisions if any
one of them requests suppression.

Joint rows run in the same velocity/position iterations as contacts, with bounded
warm-started impulses. Dynamic links form sleep islands; stationary non-dynamic
anchors provide support without joining unrelated mechanisms. Moving kinematic
anchors and active motors wake the connected island. Springs settle at their
loaded equilibrium, rather than being projected onto a rigid rest length.
`StepStats.joint_rows` and `joint_constraints` record prepared rows and velocity
row visits across all substeps. The default shared solver settings apply.

Limits: 4096 bodies, 16384 colliders, 32 children/body, 4096 joints. At most seven
scalar rows per 3D hinge; row/cache storage is reused after warmup. Adjacent joint
pairs disable collision by default in both discrete contacts and ball CCD.
There are no mesh/convex-hull parts, automatic breakage, rope/cable inequalities,
prismatic joints or articulated-body solver. Box CCD remains substeps only;
fast motor/ball rotation uses at most eight substeps and is not a universal
no-tunneling guarantee. Very high mass ratios or large constraint errors may
need smaller fixed steps or more solver iterations.

## Examples and local checks

Salvage Pinball drives two dynamic capsule flippers with torque-limited hinge
position motors. Its existing ball-launch, flipper-energy, bumper and autoplay
tests still exercise actual contacts. The 2D and 3D physics labs retain their
original falling-body scenes and add hinge/fixed assemblies, distance pendulums
and spring-supported compound dumbbells. Rendered parts match collider poses.

`cargo test -p ferravex-physics --all-features --offline --locked` covers analytic
mass/tensor calculations, child gaps/rays/CCD/character queries, rotated parts,
center-of-mass motion, transactional edits, all joint types, spring equilibrium,
limits/motor torque, mass/timestep changes, filtering, sleep/wake, stale handles
and resource bounds. The allocation test checks zero steady-step heap allocations
for both contact stacks and moving compound/hinge mechanisms. Available Linux
native and browser/Wasm paths are checked locally; no iOS SDK/device test is
claimed.
