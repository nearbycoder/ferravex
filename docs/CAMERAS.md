# Shared camera rigs

Ferravex's first-party cameras separate presentation from simulation. They provide
2D follow/dead zones and 3D follow, orbit, first-person and chase views, with
half-life damping, speed offsets, prediction, teleport cuts, obstruction sweeps,
FOV transitions and seeded shake. They do not own a renderer, input device or
physics clock. Pass the interpolated simulation target once per rendered frame.

## Three-dimensional views

```rust
use ferravex_engine::camera::{Rig3, Settings3, Input3, Target3, Mode3,
    NoObstructions};
use ferravex_math::{Vec3, Quat};
let mut rig = Rig3::new(Settings3::default()).unwrap();
let view = rig.update(1. / 60., Input3 {
    target: Target3 { position: Vec3::ZERO, rotation: Quat::IDENTITY,
        velocity: Vec3::Z * 12. },
    mode: Mode3::Chase { distance: 10., height: 4., look_ahead: 6.,
        look_height: 1., speed_distance: 0.03, speed_height: 0.01,
        prediction: 0.05, level: true },
    fov_y: 55_f32.to_radians(),
}, &NoObstructions).unwrap();
// Map view.pose.position/target/up/fov_y into the host's Camera3D.
assert!(view.pose.position.is_finite());
```

| Mode | Convention |
| --- | --- |
| `Follow` | Eye and look offsets are world-space, suitable for isometric views. |
| `Orbit` | Yaw/pitch and distance orbit a target-local pivot; positive pitch raises the eye. |
| `FirstPerson` | Target-local eye offset; world yaw zero faces **-Z**, positive pitch looks up. |
| `Chase` | Target rotation maps vehicle-local **+Z** forward; `level` flattens heading to XZ. Height is world Y. |

`fov_y` is vertical radians. Orbit damping interpolates the shortest yaw arc and
radius around its pivot; it does not interpolate a straight chord through the
subject. Chase can add speed-scaled distance/height and velocity prediction.
First-person look follows the sampled aim exactly, even if optional position
smoothing follows a stepped/corrected eye. Strike and Crystal use zero position
and target half-lives to preserve their existing authoritative aiming behavior.

`Settings3` controls position, target and FOV half-lives: a stationary error halves
each half-life. Zero snaps on a positive timestep. An initialized rig does not
advance smoothing/shake when `dt` is zero. Accepted render intervals are finite
`[0, 0.25]` seconds; the caller handles pauses and long-frame clamping. Exact
exponential response is independent of timestep subdivision for a stationary
goal; following moving targets still depends on their sampled path.

Initialization, a mode change, `cut()` and movement exceeding the optional
teleport threshold snap to the new view and clear old shake. `Frame3.cut` reports
these and obstruction recovery cuts. Explicit `cut()` immediately discards old
shake; an impulse queued after it can provide fresh feedback in the new view. `pose()` returns the last rendered view.
Rig values are copyable complete presentation state; invalid input, settings,
obstacles or query results return an error without partially advancing the rig.
Positions/velocities are bounded to ±1e6 units and orientations must be unit
quaternions. Normal floating-point precision limits still apply at large scales.

## Obstruction handling

Set `Settings3.obstruction` and supply a `CollisionQuery`. It receives a swept
sphere and returns the nearest impact fraction in `[0,1]`, `Some(0)` for a blocked
start, or `None` for a clear segment. Invalid fractions fail the update. Queries
must include every relevant obstacle and must not include the followed body.

`ferravex_engine::camera::RigidObstructions` reads current 3D rigid-world poses:

```rust
use ferravex_engine::camera::{CollisionQuery, RigidObstructions, Sweep};
use ferravex_physics::d3::PhysicsWorld;
use ferravex_math::Vec3;
let mut world = PhysicsWorld::default();
let player = world.add_ball([0., 1., 0.], 0.5, true);
world.add_box([0., 1., 4.], [2., 2., 0.1], false);
let query = RigidObstructions { world: &world, mask: u32::MAX,
    ignore: Some(player) };
assert!(query.sweep(Sweep { start: Vec3::Y,
    end: Vec3::new(0., 1., 8.), radius: 0.25 }).unwrap().is_some());
```

The physics adapter ignores sensors, filters collider membership and excludes
all children of the ignored body. It uses the public 2D/3D `spherecast` API:
current transformed parts, nearest sphere-center impact, radius zero for ray
parity. This is a bounded conservative-advancement query with the same primitive
shape and iteration limits as rigid ball CCD, not a swept-mesh guarantee.

For authored environments, `[Blocker]` supports spheres, oriented boxes and solid
planes, with at most 4096 entries/query. Sphere/plane sweeps are analytical;
boxes inflate their local bounds by the camera radius, conservatively shortening
the boom near corners. Dune uses boxes derived from visible prop bounds, authored
gate volumes, spherical rocks and the ground plane. This can retract before a
non-box mesh's exact surface. Games can provide a more precise query through the
trait or a closure without changing rig behavior.

The boom retracts immediately, leaving padding before an impact, and eases back
out using `release_half_life`. Optional travel sweeps keep normal camera motion
from passing through a thin obstruction. After shake, at most three queries
check the boom, travel and a corrected boom. Moving obstacles or conflicting
travel/boom constraints can require an explicit recovery cut, reported through
`recovered` and `cut`, instead of a continuous path. If the pivot itself starts
inside a solid, `blocked_pivot` reports it and the view remains finite; the game
must fix that invalid target/ignore configuration. The rig does not invent a
free-space target or guarantee escape from arbitrary enclosing geometry.

Choose a sphere radius that covers the near-plane footprint for the intended
FOV/aspect ratio, and set the host's near clip plane accordingly. The rig cannot
infer viewport dimensions or scene collision layers. Camera smoothing never
modifies character collision, steering, aiming or authoritative network state.

## Two-dimensional following

`Follow2Settings` supplies a world-space offset, dead-zone half-size, bounded
velocity look-ahead, half-life and optional `Limits2` for the view center. A target
inside the dead zone does not pan; outside it, the rig follows the nearest edge.
Limits clamp both the desired and final center. For a viewport within a larger
level, derive center limits from level minimum + viewport half-size and level
maximum - viewport half-size. If a viewport exceeds the level, pin that axis to
the level center. Update these limits when the viewport changes. Invalid/inverted
limits fail transactionally. `cut()` and teleport detection handle respawns.

Emberline uses this rig for horizontal framing, capped look-ahead and level edges,
while retaining its original parallax drawing and compact HUD.

## Shake and adoption

`CameraShake` holds at most eight impulses in fixed storage. Each supplies local
translation and angular amplitudes, duration and frequency; seeded smooth noise
and a quadratic decay envelope produce bounded feedback. Completed layers free
their slots. Capacity errors preserve the active layers and seed sequence.
`Settings3.shake_strength = 0` disables its visual effect for reduced motion.
Shake is applied after the unshaken base pose, before obstruction checks, so it
cannot accumulate camera drift. It is presentation feedback and does not change
weapon authority or simulation. `CameraShake` is also usable separately for 2D
view offsets/rotation. Dune impacts and Sky Courier hull hits exercise it.

The seven integrations are Emberline (2D), Marble Run (follow + rigid queries),
Dune Skimmer (chase + authored queries/FOV/impact feedback), Sky Courier (chase +
feedback), Strike and Crystal Cavern (first-person), and the 3D Physics Lab
(orbit). Existing controls and HUDs remain in place. Tests cover framing,
partitioned damping, heading conventions, angle seams/poles, constant-radius
orbits, cuts, thin walls/corners, invalid queries, seeded shake, live compound
filtering and zero heap allocations across all modes/queries/layered shake.
