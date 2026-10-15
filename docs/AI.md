# AI perception, decisions and navigation

Ferravex supplies renderer-independent AI in `ferravex_play::ai` and reusable
navigation in `ferravex_play::navigation`. `ferravex_engine::ai` adds current-pose
rigid-world visibility. These are first-party Rust systems; no scripting runtime,
behavior-tree dependency, external navigation library or asset download is used.

## Sight and hearing

`Sight::new(range, field_of_view_radians, group_mask)` validates a full field of
view from zero to TAU. `test` takes an observer position/forward, a candidate with
a host ID/position/group bits, a shared `WorkBudget`, and a `VisibilityQuery`.
Range/cone/group rejection happens before the world query. A point at the observer
is detectable without an occlusion query. Finite values are required; distance and
cone calculations use f64 to avoid overflowing valid f32 world coordinates.

The result is `Detected`, `Rejected` or `Deferred`. Deferred means an allowance
was exhausted, not that the target is visible or behind a wall. A returned target
from `select_target` passed sight; that helper chooses the highest finite priority,
then lowest ID on ties. It stops when work is deferred, including with an unbounded
candidate iterator. `incomplete` reports the truncated scan. Hosts should rotate
candidate order if they need fairness over a larger population.

`Hearing` uses both the listener range and the sound's radius. With an occlusion
query, blocked sounds have their radius scaled by an authored 0..1 factor. A
missing query explicitly means unoccluded hearing. Lack of query budget is still
`Deferred`. `NoiseBuffer` retains the latest bounded sounds, ages them in simulation
time, and reports saturation. Publish accepted gameplay sounds, exclude an actor's
own source ID and remove stimuli for invalid/dead sources as appropriate.

`Memory` stores at most 256 tracks, with distinct sight/hearing lifetimes. Call
`advance(dt)` once at the beginning of each simulation tick, then record detected
stimuli. Current sight wins over hearing in the same tick. A new sound may update
a remembered position later, but never sets `visible`. Sight freshness lasts only
until the next advance. A deferred/unscheduled sight check therefore cannot authorize
firing. `best(mask)` prioritizes fresh sight, younger stimuli and lower IDs. At
capacity, a new identity is rejected and counted; existing identities still update.
`forget` handles despawns, identity reuse or invalidated targets. Host IDs should
include a generation when reused.

```rust
use ferravex_play::ai::*;
use ferravex_math::Vec3;
let sight = Sight::new(30., 120_f32.to_radians(), 2).unwrap();
let mut budget = WorkBudget::new(64, 8, 128);
let observer = Observer { position: Vec3::ZERO, forward: Vec3::Z };
let target = Candidate { id: 7, position: Vec3::Z * 8., groups: 2 };
// Replace Unoccluded with the authoritative world's VisibilityQuery.
let result = sight.test(observer, target, &mut budget, &mut Unoccluded).unwrap();
assert_eq!(result, Detection::Detected);
```

`RigidVisibility` uses the rigid world's current collider poses, membership mask,
sensor exclusion and optional ignored observer body. Hosts should omit target-body
occluders or stop queries at a target surface. Strike's separate authored arena
adapter tests static solids; other actors are not sight occluders in that example.
Noise attenuation is geometric/gameplay tuning, not acoustic propagation.

## Decisions and scheduling

A `DecisionGraph` holds up to 64 states and 256 ordered edges. An edge requires all
of its `all` bits, none of its `none` bits and an optional minimum time in the
current state. Source `None` is an interrupt from any state. The first matching
edge wins; matching the current destination holds that state and its timer,
blocking lower-priority edges. This avoids accidental state oscillation. Updates
perform at most one transition; transition overshoot is not applied to the next
state. Timers use f64 and saturate at 3600 seconds. `DecisionState` is small and
independent of the graph, so many agents share one authored decision definition.

`RoundRobin::next(population, allowance)` yields dense agent indices without
allocation. The host keeps the snapshot stable during the call and can reorder it
between ticks. A stable population and allowance visit every index within
ceil(population/allowance) calls. A zero allowance does no work. This scheduler
controls sensing frequency; movement and safety checks can still run every tick.

`WorkBudget` shares candidate, occlusion-query and navigation-step allowances among
agents. Counts record consumed work and deferred candidate/query attempts. A query
allowance bounds the number of calls, not the internal cost of an arbitrary host
query. Host broadphase/query implementations must also have reasonable limits.
Construct fresh budgets each simulation tick; deferred work must be revisited by
the host scheduler. All dt-taking AI APIs accept finite 0..60 seconds.

## Reusable search and paths

The original `Grid::path` convenience function remains available and allocates its
return/scratch storage. For game loops, construct `SearchScratch` or `PathPlanner`
once. The incremental search uses deterministic weighted A*, four/eight neighbors,
no diagonal corner cutting and an indexed binary heap with at most one open entry
per cell. Distances, parents, stamps, heap positions and route buffers are reused.
The configured capacity must cover the grid, at most four million cells. Choose a
small level-specific grid to avoid unnecessary resident memory.

`begin` validates endpoints/capacity before replacing an active search. `step`
expands at most the requested work units; each unit handles one popped cell with
up to eight neighbors, or one parent in route reconstruction. `step_budgeted`
charges actual work to the shared allowance. A completed route is exposed in
reverse order to avoid an unbudgeted reversal; `copy_path` reverses into supplied
storage and reports insufficient capacity without changing that storage. Copying
is explicit host work proportional to route length. Grid identity, dimensions and
revision invalidate searches when a different or edited grid is supplied to step,
even after completion. Already copied routes require host invalidation on edits.

`PathPlanner` queues at most 1024 requests, coalesces queued requests for one agent,
reports backpressure and preserves FIFO order. A ready result remains until
`acknowledge`; polling does not drop it. Cancel departed agents, validate their ID/
generation before adopting results, and acknowledge errors as well as successes.
Grid changes produce stale results; the host can resubmit. A single planner shares
scratch across agents; it does not run parallel searches or a worker thread.

`Route3` holds at most 4096 validated world-space points and f64 cumulative lengths.
`set` is transactional and allocation-free within its construction capacity. Repeated
points are supported. `PathCursor` advances exact travel distance across corners and
samples by binary search; it suits authored paths such as Orchard Guard's pests.
`WaypointFollower` advances only when a collision-driven actor reaches a point,
with an explicit maximum number of skipped points per call. Reset cursors when
replacing their route. Collision resolution remains the game's responsibility.
These are grid/polyline tools, not a navmesh, avoidance solver or off-mesh-link system.

## Current games

- Strike uses sight cones/static-solid queries, occluded accepted-shot hearing,
  expiring memory, patrol/investigate/engage/reload/dead decisions and queued routes
  over an inflated XZ grid. Three bots receive sensing slots per tick, with 64
  candidates, 16 world queries and 128 path steps shared per tick. Reused command
  storage submits ordinary `InputFrame`s; Match owns ammo, collision, damage and
  respawn. These are offline practice bots; network bot hosting is not added here.
- Neon Swarm senses up to four pursuers per tick, shares 128 candidate/24 query/192
  path-step budgets, routes around inflated cover and combines movement with
  bounded separation from up to eight nearest peers in a fixed 64-agent snapshot.
  Accepted player shots generate short-lived noise. Movement still passes through
  the game's existing collision resolution. Its three-wave autoplay remains beatable.
- Orchard Guard shares one authored polyline across pests and uses distance cursors
  that carry travel through corners. Towers use bounded range/group selection and
  prefer the farthest-progressed living pest. Its four-wave economy and combat rules
  remain in the game and are exercised to victory locally.

Tests cover cones, masks, occlusion, extreme coordinates, hearing attenuation,
expiry, saturation, deferred scans, priorities, timer transitions, fair scheduling,
incremental/reference A* equality on weighted/disconnected maps, no corner cutting,
grid edits/identity, epoch wrap, queued results/backpressure, path corner travel and
collision-driven waypoint progress. A counting allocator exercises 1000 updates of
the constructed AI/search/follower systems with zero allocations. Whole games still
allocate for their existing ECS/rendering/gameplay work. Available-platform and
performance results are recorded in [PERFORMANCE.md](PERFORMANCE.md).
