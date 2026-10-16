# Shared game-quality series

Twenty first-party engine updates, implemented in order with separate pull requests.
Commits and merges use `[skip ci]`; validation runs locally. Systems must work with
native and WebAssembly game hosts, bound runtime work/storage, and retain existing
save/network semantics or explicitly version changed gameplay rules. Physical
platform checks are recorded only when actually performed.

| # | Engine system | State |
| --- | --- | --- |
| 1 | Gameplay tuning profiles | [PR 24](https://github.com/nearbycoder/ferravex/pull/24), merged |
| 2 | Ability system | [PR 25](https://github.com/nearbycoder/ferravex/pull/25), merged |
| 3 | Attributes and status effects | [PR 26](https://github.com/nearbycoder/ferravex/pull/26), merged |
| 4 | Combat feedback controller | [PR 27](https://github.com/nearbycoder/ferravex/pull/27), merged |
| 5 | Attack timelines and telegraphs | [PR 28](https://github.com/nearbycoder/ferravex/pull/28), merged |
| 6 | Squad tactics and cover selection | [PR 29](https://github.com/nearbycoder/ferravex/pull/29), merged |
| 7 | Encounter director | [PR 30](https://github.com/nearbycoder/ferravex/pull/30), merged |
| 8 | Navigation meshes and traversal links | [PR 31](https://github.com/nearbycoder/ferravex/pull/31), merged |
| 9 | Vehicle and flight controllers | [PR 32](https://github.com/nearbycoder/ferravex/pull/32), merged |
| 10 | Racing AI | [PR 33](https://github.com/nearbycoder/ferravex/pull/33), merged |
| 11 | Richer physics materials | [PR 34](https://github.com/nearbycoder/ferravex/pull/34), merged |
| 12 | Fast-body collision robustness | [PR 35](https://github.com/nearbycoder/ferravex/pull/35), merged |
| 13 | Breakable mechanisms | [PR 36](https://github.com/nearbycoder/ferravex/pull/36), merged |
| 14 | Camera anticipation and authored zones | [PR 37](https://github.com/nearbycoder/ferravex/pull/37), merged |
| 15 | Procedural contact animation | Implemented; locally validated |
| 16 | Procedural environment builders | Pending |
| 17 | Acoustic zones and adaptive music | Pending |
| 18 | Editable touch and accessibility layouts | Pending |
| 19 | Objective and puzzle graphs | Pending |
| 20 | Adaptive performance and playtest diagnostics | Pending |

## 1. Gameplay tuning profiles

`ferravex_play::tuning` (also `ferravex_engine::tuning`) separates validated motion,
difficulty and assists from graphics settings. Rally and arena presets preserve
current handling by default. Load/save uses the existing versioned typed save
format; edits validate before replacement and incompatible host rules IDs fail.
`fingerprint()` identifies the complete rules configuration for record/replay
contexts. Applying profiles, hashing and reading values allocate nothing.

Pocket Rally consumes acceleration, braking, speed, steering response and traction;
optional steering assistance follows the actual track tangent. Its reset retains
the profile, and its default record context includes the profile identity. Neon
Swarm consumes speed/input response and difficulty-scaled contact damage, with a
validated custom-profile constructor. Hosts must include custom fingerprints in
custom record contexts and adopt edits at safe reset/checkpoint boundaries.
Assists are opt-in host capabilities; setting an aim or jump-buffer field does
not silently add aim/jump behavior to games that do not implement it.

Local checks: shared profile serialization/invalid-edit/rules/record-identity
regressions, real Rally acceleration and existing complete-race tests, existing
Neon progression, strict Clippy and native/Wasm compilation of changed consumers.

## 2. Abilities and resource reservations

`ferravex_play::ability` / `ferravex_engine::ability` offers fixed-capacity ability
banks (up to 64 configured slots), sequential charge recharge, resource costs and
regeneration. Instant activation is atomic. Begin/commit/cancel supports delayed
abilities, single-use reservations and exactly-once refunds. Hosts supply enabled
conditions, unique generational owner IDs and the actual gameplay effect; tokens
must stay with their originating owner/bank/resource account. Timers use simulation
time and charging work is bounded by slots, without per-update allocation.
Neon Swarm's actual shot and dash actions now use this system; its HUD reads the
shared dash timer. Cancellation, denied activation, resource conservation, stale
and foreign tokens, charge saturation and clock partitioning have local regressions.

## 3. Attributes and status effects

`attributes::EffectSet<N>` composes additive and multiplicative stat modifiers,
refresh/strongest/capped-stack policies, tag immunities and expiry in fixed storage
(up to 32 effects). Periodic effects aggregate elapsed pulses into at most N reports;
refresh preserves pulse phase, and no post-expiry damage is emitted. Hosts apply
the reports through their authoritative rules. `Vitals` supplies eight resistance
channels, flat armor, shields, bounded health damage and healing. Attribute IDs,
effect IDs and resistance channels are host-defined, without renderer or ECS ties.
Orchard Guard now runs real frost slows and seed flinches through the shared set,
while keeping its public HUD timers and collision-driven path progress.
Local regressions cover stacking/expiry, repeated refresh, periodic boundaries,
immunity, capacity, invalid edits, damage conservation and the existing four-wave
Orchard autoplay. Strict Clippy and Wasm checks run locally; CI stays skipped.

## 4. Confirmed-event feedback

`feedback::FeedbackController<N>` routes accepted fire/hit/hurt/landing/dash/pickup/
explosion events to shared sound, particle-count and animation-reaction responses.
It combines the existing seeded camera-shake implementation with flash, rumble and
optional single-player hit-stop envelopes. It never applies damage, ammo changes
or a simulation time scale. Networked mode forcibly disables hit-stop. Motion,
flash and rumble have independent accessibility scales; turning a channel off
also clears its existing output. Platform rumble and animation/particle playback
remain host adapters, not fabricated device support.

Events are bounded per rendered update and deduplicated over an explicit N-event
session history; reliable transport remains responsible for older duplicate
rejection. Neon Swarm now routes accepted sounds through this controller and
shows a brief damage border. Its gameplay clock is unchanged. Local tests cover
bounded bursts, duplicate events, decay, invalid inputs, channel suppression and
online no-hit-stop behavior; Clippy and native/Wasm consumer checks also run.

## 5. Fixed-clock action timelines

`action::ActionTimeline` models windup, active and recovery windows, an authored
windup cancellation cutoff, forced interruption and a single buffered follow-up.
Updates emit at most eight ordered transition reports with action serials and
substep offsets. Gameplay owns hit queries and resource commit; animation consumes
the authoritative phase. `HitWindow<N>` prevents duplicate targets in one action,
rejects stale serials and reports bounded target capacity. Reservations compose
with the ability bank: cancel before commitment refunds, active commitment spends.

Neon enemies now wind up before contact damage, visibly telegraph the windup,
reduce movement while planted/recovering and can be interrupted by an early shot.
Damage still requires real proximity at activation and ordinary player invulnerability/
dash rules. The new combat rules use record context 3. Local tests cover exact
phase boundaries, event order/offsets, partitioning, interruption, buffering,
resource commitment and hit-window deduplication, plus ordinary swarm autoplay.

## 6. Squad coordination and cover

`ai::SquadBoard<C>` shares expiring last-known contacts with revisions and reserves
up to 64 authored cover points with exclusive, expiring leases. Stable roles offer
opposing flank destinations. Cover checks spend the caller's candidate/query budget;
cheap host eligibility checks precede authoritative occlusion. The host still must
validate a complete route and faction membership. Reports must originate in real
perception, never radio echoes. Radio contacts carry no permission to attack.
FPS practice bots now share actual sightings as hearing tracks, choose flank goals,
and seek cover when supporting, reloading or retreating. Cover planning is scheduled
one bot per tick, under the existing combined sensing/path budget. Actual paths and
ordinary authoritative inputs retain collision, ammo and hit rules. Death releases
leases. Local tests cover exclusive leases/expiry, zero-budget deferral, opposing
flanks, contact expiry, hidden silent players, radio without visibility, accepted
normal bot shots and strict native/Wasm checks.

## 7. Encounter pacing

`encounter::EncounterDirector` admits spawns against finite wave credits, a live
actor cap and a fixed-clock cadence, with hysteresis-controlled recovery breaks.
It never catches up with an unbounded spawn burst. Hosts supply danger pressure,
validate safe locations before spending, and perform the actual spawn. It does
not inspect hidden positions or change enemy health. Neon Swarm now runs its real
waves through the director with a 12-enemy live cap and two-second pressure breaks;
wave totals are retained and record context advances to 4. Local checks cover
credit conservation, caps, cooldowns, hysteresis, invalid inputs, normal-input
swarm victory, strict Clippy and Wasm compilation.

## 8. Triangle surfaces and traversal links

`navigation::NavMesh` validates authored triangles and directional walk/jump/drop/
climb links, optionally welding exact shared edges at setup. Incremental Dijkstra
uses reusable bounded scratch space and caller path allowances (one node scan is
at most 2048 comparisons; outgoing edges are charged separately). Routes include
interior points, portal crossings and explicit traversal actions. Hosts execute
traversals and bake suitable agent clearance; this is not automatic world baking
or a shortest geometric funnel. Polygon/link edits invalidate searches and routes;
mesh identities prevent adopting another mesh's result. Crystal Cavern's actual
pilot now plans across the open floor/door surfaces and completes the full puzzle.
Local tests cover budgets, edits, one-way traversal, disabled links and complete
ordinary-input exploration; strict Clippy and Wasm checks pass.

## 9. Shared handling motors

`vehicle` supplies a planar arcade/hover motor with speed-dependent steering,
steering inertia, lateral grip, drag, speed caps and non-reversing brakes, plus an
assisted-flight motor with pitch/bank limits, turn inertia and speed response.
Validated updates allocate nothing and return candidate motion; world collision,
boost accounting and mission authority stay in the host. Dune Skimmer and Sky
Courier now use these motors with their original handling values. These are
arcade controllers, not simulated tire suspensions or aerodynamic airframes.
Local brake/slip/invalid-input/flight-envelope tests and both full race/flight
ordinary-input completions pass, with strict Clippy and Wasm checks.

## 10. Shared racing decisions

`racing::RacingDriver` follows closed polylines with speed-scaled lookahead,
corner/curvature speed limits, bounded perceived traffic spacing and seeded passing
lanes. A stalled driver attempts a short reverse using ordinary controls. Path
work is bounded to 2048 segments and 32 supplied rivals, without update allocation.
The host owns perception, road safety, collision, progress and recovery penalties.
Pocket Rally's actual pilot now consumes this shared driver and still completes
three laps and a save/ghost roundtrip. Traffic and recovery are shared API features;
the current single-car example supplies no rivals. Midpoint validation: 555 local
workspace tests passed, strict workspace Clippy and all-feature Wasm compilation
passed, and the dependency audit confirms zero external Cargo packages.

## 11. Contact materials

`physics::PhysicsMaterial` adds distinct static/sliding friction, symmetric explicit
combination rules and load-limited rolling/spin resistance in the rotating rigid
solver. Angular resistance is a contact torque limit, measured as normal impulse
times a material length; it cannot act on airborne bodies. Existing defaults keep
geometric friction and average restitution with zero rolling resistance. Material
edits invalidate cached impulses. Marble Run's platforms use slightly stronger
static grip and light rolling resistance; course record rules advance to 3.
Local real-body tests verify static holding versus sliding, rolling energy loss,
invalid edit rollback, existing contact/cache/allocation suites and all four marble
course completions. Strict Clippy and Wasm checks pass.

## 12. Fast boxes

CCD-enabled box/box pairs now use a bounded separating-axis translation sweep,
including compound child poses, rotated boxes and relative target translation.
Initial overlaps prevent further inward tunneling while ordinary contacts resolve
the overlap. Sensors and collision layers retain their filters. Siege Foundry's
already-CCD-enabled debris automatically uses the new path. Orientation is frozen
within each of at most eight substeps; this is not universal rotational CCD and
adds no unsupported capsule/cylinder contact pairs. Local 3000-unit/s thin rotated
wall, moving kinematic wall and sensor tests pass, alongside the full existing
physics/cache/allocation and demolition suites, strict Clippy and Wasm checks.

## 13. Load-driven breakable mechanisms

Fixed, hinge, distance and spring joint descriptions accept optional force/torque
break limits. The solver measures conservative absolute constraint-row load sums
in newtons/newton-meters, retains outer-tick peak loads, and removes an overloaded
joint once with a generational break event. Breaking wakes released bodies and
restores collision filtering for subsequent substeps. Event storage is reserved
when joints are authored, bounded by the existing joint capacity. Siege Foundry
now bonds adjacent authored tower chunks with breakable fixed joints; real
projectile contacts release them. This is authored chunk destruction, not arbitrary
mesh fracture. Local tests cover real impulse breakage, once-only events, stale
handle replacement, stable unshot towers, complete ordinary-fire victory, the
physics allocation/contact suites, strict Clippy and Wasm compilation.

## 14. Camera composition

`camera::CameraComposer<N>` adds bounded velocity/acceleration anticipation and
priority-selected authored zones with feathered boundaries and half-life blending.
Zones control focus offset, boom scale and FOV scale for world cameras; first-person
input passes through unchanged. Cuts clear composition history. Marble Run adopts
light bounded anticipation and finish-area framing, explicitly keeping boom/FOV
scales at one so poles do not zoom the camera. Validation also caught and fixed a
slow-ball regression from item 12: balls retain their original minimum sweep
motion threshold, preventing tiny floor sweeps from stopping ordinary rolling.
Local zone/priority/first-person/constant-distance tests, all marble courses and
the physics suite pass, with strict Clippy and Wasm checks.

## 15. Contact animation

`animation::TwoBone` solves bounded analytic limbs with pole controls and clamped
reach; `FootPlant` blends validated support contacts, tracks support identity and
accepts current support displacement without changing a physics body. Hosts own
support queries and animation contact phases. Strike's operators now have actual
knee bones, split upper/lower leg geometry and two bounded downward support rays
per visible actor. IK adjusts only the skin palette after the authored graph.
The current arena adapter supports horizontal axis-aligned top faces; arbitrary
slope normals and moving-support transforms must come from a richer host query.
Milestone validation: 568 local workspace tests, strict Clippy, all-feature Wasm,
zero-external-dependency audit and a viewed native Strike render smoke passed.
Tests include limb lengths, unreachable/degenerate poles, support movement/release
and unchanged authoritative player state under rendered ground correction.
