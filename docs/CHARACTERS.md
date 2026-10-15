# Shared character controllers

`ferravex-physics::character` implements an upright 3D capsule motor and a planar
2D rectangle motor. `ferravex-engine::character` exposes the same first-party
module. The motor shares no third-party engine code, does not insert a rigid body,
and does not allocate per-step scratch storage. Position is the center of the
feet/sole; positive Y is up. Shape and movement settings are explicit values.

```rust
use ferravex_physics::character::*;
use ferravex_math::Vec3;
let floor = Obstacle {
    shape: ObstacleShape::Plane { normal: Vec3::Y },
    ..Obstacle::cuboid(1, Vec3::ZERO, Vec3::ONE)
};
let mut character = Character::new(Vec3::ZERO);
let shape = CharacterShape::Capsule { radius: 0.3, height: 1.8 };
let settings = CharacterSettings::default();
character.step(&[floor][..], shape, settings, CharacterInput {
    direction: Vec3::X, jump: false,
}, 1.0 / 120.0)?;
# Ok::<(), CharacterError>(())
```

Use `CharacterShape::Box2 { half: Vec2::new(width/2.0, height/2.0) }` for a
platformer. The box keeps sharp corners. Its feet, velocity and terrain positions
must stay in XY with Z zero, and terrain rotations must stay about Z. For a screen
coordinate system with Y down, map a top-left sprite `(x,y)` to
`(x + width/2, -y - height, 0)`. Emberline demonstrates this mapping, moving platform
carry, forgiving jumps and the same sprite/HUD as before.

## Movement and collision

Call `Character::step` from a fixed clock with a finite timestep in `(0, 0.1]`.
It accelerates toward a clamped horizontal direction, applies gravity, sweeps,
slides, snaps to nearby walkable ground and clips velocity against contacts.
`CharacterStep` reports movement, grounded/support state, steps, jump events,
carried displacement and narrowphase candidate visits. A new jump press remains
buffered for `jump_buffer` seconds; `coyote_time` permits a press briefly after
leaving ground. Include both timers and the held bit when saving/predicting state.
Holding jump never automatically jumps again after landing.

For explicit displacement without motor velocity/gravity, use `move_and_slide`.
Crystal Cavern uses this with a 1.75-unit capsule and its existing tile/gate terrain,
including long sweeps. Strike replaces its separate AABB movement with the shared
capsule at 64 Hz, including the client/server prediction path.

Capsules sweep their entire shaft and rounded ends, including thin obstructions
between the end caps. Segment/box distance uses exact piecewise minimization;
ball/capsule terrain uses closest features. Cylinders use bounded conservative
convex-distance minimization. The planar box uses separating axes against rotated
boxes, and closest-feature tests for circles/capsules. Terrain also supports solid
half-spaces. These are continuous translation queries against each tick's terrain
pose; rapidly moving obstacles are not swept through a stationary character.

Walkable slopes use `min_ground_normal` (upward normal component, default 0.7).
Steep contacts block uphill intent while gravity may slide downhill. Step climbing
requires prior ground contact, an unobstructed capsule/box clearance sweep and a
reachable walkable tread within `step_height`. Capsule rounding at a short box's
edge is classified using that box's authored tread. No step assist is applied in
air. `ground_snap` is disabled while rising. Tune distances to game units: a pixel
platformer needs different speed, gravity and step distances than a meter-scale FPS.

## Terrain and moving supports

A `Terrain` implementation visits current `Obstacle` poses intersecting a swept
`QueryBounds`. It must report every intersecting obstacle in deterministic order,
using stable unique `u64` identities, and support lookup by identity. Slices of
obstacles and both rigid `PhysicsWorld` types implement it. The rigid-world adapter
uses collider index/generation identities, skips sensors and honors the motor's
membership mask. Use a sensor or separate layer for any character proxy body to
avoid self-collision. Changing the rigid body's pose is immediately visible;
there is no query synchronization pass.

Move platforms before stepping characters. The motor stores the support's local
anchor and previous world anchor, applies its translation/rotation, and sweeps the
passenger against other geometry. The character remains upright. A jump inherits
the carrier's measured point velocity. Removed supports or changed generations
release the passenger. Core tests cover translations, rotations, removal, momentum
and the live rigid-world adapter. Multiplayer platforms also require authority
pose history/support replication; Strike's current arena stays static.

The motor resolves only kinematic motion. It does not push dynamic bodies with
impulses, solve crushing forces, prevent all penetrations caused by fast terrain,
or provide mesh/heightfield collision. Author ramps using rotated boxes or planes.
The existing translational `ferravex-dynamics::move_character` sphere helper remains
available for older callers; migrated games use this module.

## Bounds, errors and replay

Settings, shapes and finite coordinates are validated. World positions and motion
vectors are bounded to the documented code limits (coordinates within ±1,000,000,
capsule height at most 1000). Query work has an explicit candidate budget and
1–16 slide/recovery iterations. Sweeps have a fixed 48-iteration cap; exhaustion
returns an error instead of granting passage through geometry. Starting overlaps
are repaired only within `max_recovery`; impossible/crushed states return an error.
A failed `step` leaves all character state unchanged. A caller can report, respawn
or otherwise handle that failure; there is no hidden unbounded retry.

For static deterministic terrain, snapshot the complete `Character` (including
jump and support state) and replay the same fixed inputs. Moving terrain also
needs its exact poses/identities per tick. Strike carries jump timers and held
state in its validated protocol-3 snapshot, while its static arena needs no moving
support history. Old protocol peers must upgrade together. The local suite checks
wire round-trips, buffered landing replay, actual UDP prediction/reconciliation,
long thin-wall sweeps, ceilings, ramps, steps and platformer autoplay completion.
