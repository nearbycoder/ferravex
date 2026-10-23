# Journey400 foundation research: 49 additional candidates

Read-only research, 2026-10-22. These are **proposals, not implementations or reserved task numbers**. `F01`–`F49` are local references for the parent to select and map into 201–400. Only this proposal file is owned by this research assignment.

## Basis and limits

Read `AGENTS.md`, `CONTRIBUTING.md`, `DEPENDENCIES.md`, `docs/AGENT_GUIDE.md`, and the Journey200 task ledger. Inspected current first-party implementations and nearby tests in core, engine, ECS, math, rotating physics, networking and FPS, with targeted checks of play/scene/navigation/camera, graphics/render consumers and example call sites. Read relevant implementation summaries in Journey200 receipts 001, 003, 006, 009, 019, 024, 026, 032, 033, 034, 037, 081 and 084; source takes precedence over receipts. Line references below are discovery anchors in the inspected working tree, not immutable revision links.

No build, test, browser, dependency resolution, formatting, staging, commit or delegation was performed. Existing uncommitted Journey50/Journey200 work is the baseline. This research does not establish combined integration, live game quality, allocation measurements, or platform/determinism guarantees. Proposed API names are design sketches, **not existing callable APIs**.

The following are already present and deliberately excluded as new features:

- Checked sessions/manual stepping; frame reports and percentiles; drift-free timers; scheduler draining/rescheduling; command metrics; pool reuse; snapshot bracketing; spatial query saturation; resource replacement; atomic engine event batches; reader-lag reporting; dependency layers; state traces; asset manifests and transactional asset edits.
- ECS `get_many` and transactional `despawn_many`; checked vectors/matrices/quaternions/colors; translational sweeps; rotating-body definitions, ray/sphere caller-buffer queries, character teleport/recovery, joints/materials/mass, contact lifecycle and sleep/wake inspection.
- Typed actions, schema introspection, scene prefabs/path lookup, route projection, ghost discontinuities, gameplay previews, animation/camera/audio/AI diagnostics, renderer telemetry, and Journey200's browser/game adoption passes.
- Networking already has correlated probes and jitter estimates, wrapped-tick handling, acknowledged snapshot deltas with full recovery, ordered reliable events/backpressure, automatic reconnect, prediction/reconciliation, rewind, and respawn-aware remote interpolation. These are not missing systems.

Compatibility rules for **every** candidate: additive APIs and existing defaults; first-party/std-only graph; no simulation dependence on rendering or wall clocks; bounded storage/work with visible exhaustion; no new serialized fields without an explicit version/migration task. Current packet version is **5**, despite the historical filename/comment `wire_v4.rs`; WebSocket subprotocol is `ferravex.v5`. Do not rename or reinterpret bytes as an incidental cleanup. Preserve authority-owned damage, movement admission, individual reliable ACKs, input coalescing and life epochs.

Implementation agents would own only their assigned source files and a unique regression file. Parent owns registrations/reexports, facade/docs/catalog wiring, demo adoption outside those paths, shared validation and final task numbering. New modules below require an explicit parent registration. Tasks sharing a current file must run serially or receive a parent-approved extraction first; the list is not an invitation to edit the same file concurrently.

## Five highest-value first tasks

1. **F01: fixed-tick context** — removes a documented per-frame/per-tick ID trap from every fixed-step host and makes scheduler/replay/event composition easier.
2. **F02: interpolation history with explicit discontinuities** — turns repeated previous/current/reset/pause handling into a small reusable feature; Marble Run is a concrete first adopter.
3. **F03: disjoint resource borrowing** — makes two-resource gameplay systems straightforward without temporary removal or unsafe code.
4. **F10: optional ECS query members** — a common composition pattern currently cannot be expressed through the sealed query API; useful for heterogeneous gameplay entities.
5. **F21: checked rigid-body teleport/reset** — packages an error-prone multi-field recovery operation for Pinball, Marble Run and the physics labs, distinct from the already implemented character reset.

These five can be assigned with largely separate files if F01 owns core/app and F02 is a new module. Network work should begin with F29/F31/F32, then the more involved F39/F40 extraction after API review; a transport-independent protocol core is high value but not a small first patch.

## Core and engine composition

### F01. Fixed-tick context and absolute scheduled IDs

- **Evidence/gap:** `crates/ferravex-core/src/lib.rs:50` yields only `dt` from `FrameStep::ticks`; `FixedClock::advance` at 148 advances the whole batch counter before execution. `crates/ferravex-engine/src/app.rs:182` gives `Game::fixed_update` only `dt`. The agent guide explicitly warns that reading the clock inside the loop repeats the batch-end tick.
- **Deliverable:** additive tick-batch/context API yielding the scheduled tick ID, index within batch and fixed delta, plus a defaulted contextual `Game` hook forwarding to the existing hook. Keep scheduled and actually executed counts distinct. Handle zero work, early stop, manual stepping, reset and wrapping explicitly without allocating or changing `FrameStep` struct literals.
- **Ownership/dependencies:** core `lib.rs`, `session.rs`, engine `app.rs`; serialize against edits to these files. Builds on J200 001/003, not another report API. Parent wires facade/examples. Verify 0/1/multiple ticks, wrap, paused step, early error and unchanged old `Game` behavior.

### F02. Reusable interpolation history and presentation holds

- **Evidence/gap:** `crates/ferravex-engine/src/snapshots.rs:182` brackets stored samples but deliberately leaves interpolation/discontinuity policy to callers. `examples/marble-run/src/sim.rs:20,128,151` separately maintains previous pose/reset state; `examples/marble-run/src/trial.rs:65,120` preserves presentation alpha while inactive and manually interpolates pose.
- **Deliverable:** a small `InterpolationPair<T>`/pose adapter supporting commit-on-tick, sample-with-interpolator, snap/reset and explicit discontinuity generation. Include an explicit presentation hold/resume operation so an inactive session's zero alpha does not move the image backwards. Two retained values, no clock or input ownership; Vec2/Vec3/Quat examples.
- **Ownership/dependencies:** new `crates/ferravex-engine/src/interpolation.rs`; parent registration in engine `lib.rs`. Independent of F01, usable with its context. Preserve J200 009/084 and existing ghost/network interpolation. Parent-owned Marble Run adoption must cover zero-tick frames, catch-up, pause and respawn.

### F03. Safe disjoint resource access

- **Evidence/gap:** `crates/ferravex-engine/src/resources.rs:266`–295 exposes one `require[_mut]`/`get[_mut]` at a time. `try_update` is transactional replacement of one value, not simultaneous access to distinct stored types.
- **Deliverable:** checked two-/three-resource access or scoped callbacks with mutable and shared combinations, duplicate-TypeId/missing-type errors before invoking user code, and ordinary Rust references. This lets inventory/objective or simulation/feedback systems compose without remove/reinsert choreography. Keep the 256-type cap and avoid cloning or `unsafe`.
- **Ownership/dependencies:** `resources.rs` only, following J200 016. Parent owns API-map adoption. Verify same-type alias rejection, all missing positions, callback non-invocation on failure and capacity/content preservation.

### F04. Prepared sequential system runner

- **Evidence/gap:** `crates/ferravex-engine/src/system_order.rs:251,331` resolves names/layers; its J200 019 receipt explicitly excludes execution. `app.rs:182` remains a monolithic game hook. There is no registered executable schedule connecting this graph to host systems.
- **Deliverable:** `SystemSchedule<C,E>` registering bounded named callbacks against the existing `SystemOrder`, resolving once before use, and executing a stable sequence with explicit error/stop and flush boundaries. Include opt-in run predicates and a preparation error for a missing callable. No parallel executor, automatic borrow inference or new dependency graph.
- **Ownership/dependencies:** new engine `systems.rs`; read/reuse `system_order.rs`, F01 context optional. Parent registers module. Preserve sequential `resolve` order rather than flattening layers, which can differ. Verify dependency order, disabled systems, stop/error tail suppression and reprepare after edits.

### F05. Fallible deferred commands with explicit failure policy

- **Evidence/gap:** `crates/ferravex-engine/src/commands.rs:46` stores `FnOnce(&mut W)`; `apply_report` at 162 counts normal returns. A command's domain failure cannot be propagated except through captured external state; panic accounting from J200 006 is already implemented.
- **Deliverable:** additive fallible command queue/adapter accepting `FnOnce(&mut W) -> Result<(), E>`, bounded application with command ID and error, and explicit stop-on-error or continue policy. Preserve untouched tail order and return rejected closures. Document that commands may partially mutate before returning an error and are never automatically retried.
- **Ownership/dependencies:** `commands.rs` or its explicitly assigned child module; parent registration if separate. J200 006 baseline. Verify failure at first/middle/last command, capacity, zero budget, drop behavior and continued old queue semantics.

### F06. ECS-specific deferred entity authoring

- **Evidence/gap:** ECS structural operations require `&mut World` (`crates/ferravex-ecs/src/lib.rs:166,282`), while queries keep guards alive. Generic `CommandBuffer` solves deferred execution but supplies no typed spawn token or mapping for a newly spawned entity referenced by later commands.
- **Deliverable:** bounded ECS command adapter with spawn tokens local to one buffer epoch, insert/remove/despawn operations targeting existing entities or earlier tokens, and an apply result mapping tokens to real entities. Use checked ECS operations; report invalid/stale dependencies and consume tokens only once. Explicit partial-application policy rather than claiming arbitrary closures are transactional.
- **Ownership/dependencies:** new engine `ecs_commands.rs`, depending on F05 or a shared error outcome. No ECS identity-bit changes. Parent registers it; Neon Swarm's deferred spawn/removal loops are an adoption candidate. Verify queries finish before apply, invalid/cross-buffer tokens, stale handles and partial budgets.

### F07. Owner-scoped tick jobs and cancellation

- **Evidence/gap:** `crates/ferravex-engine/src/scheduler.rs:39,49,75` schedules/reschedules/cancels one `TaskId`; ready draining already exists. There is no lifecycle group that cancels all jobs belonging to a round, scene or despawned actor.
- **Deliverable:** bounded owner/scope tokens and scope cancellation on the existing scheduler, preserving original FIFO ties and returning/discarding payloads under a documented policy. A generation on scope reuse prevents an old scene handle from canceling new jobs. Scope cancellation must not reset unrelated deadlines or reuse task IDs.
- **Ownership/dependencies:** `scheduler.rs`, after J200 005; optional F01 absolute ticks. Parent picks a real delayed-spawn/respawn consumer. Verify canceled scopes, rescheduled members, equal deadlines, reuse, callback/destructor behavior and capacity rejection. This does not duplicate simulation timers.

### F08. Disjoint mutation of pooled objects

- **Evidence/gap:** `crates/ferravex-engine/src/pool.rs:119,125,176` offers single-object access and whole-pool mutable iteration, but no fallible simultaneous access to a selected pair. ECS `get_many` does not apply to this separate generational pool.
- **Deliverable:** checked selected-pair/many borrowing using safe disjoint slot access, preserving caller order and rejecting duplicate/stale handles before returning any references. Include an actual transfer/collision-response example between two live pooled actors; no remove/reinsert handle churn.
- **Ownership/dependencies:** `pool.rs`, preserving J200 007's retain/reuse behavior. Parent chooses a pool consumer; current game need is less directly established than F03/F10. Verify stale generations, same handle twice, caller order, zero/full pool and no slot allocation during access.

### F09. Distance-ranked proximity queries

- **Evidence/gap:** `crates/ferravex-engine/src/spatial.rs:109,148` provides exact rectangular point queries ordered by ID. Point coordinates are private, so a caller cannot use returned IDs alone to perform reusable distance-ranked selection without maintaining duplicate positions.
- **Deliverable:** checked radius and nearest-k point queries returning ID, point and squared distance into caller storage, stable distance/ID ties, explicit total/truncation and the existing cell-visit ceiling. Search all candidates in the bounded region before choosing nearest-k, so early ID truncation cannot hide the closest target.
- **Ownership/dependencies:** `spatial.rs`, after J200 010. No physics broadphase replacement or 3D index. Neon Swarm/Orchard Guard targeting are prospective adoption tasks, not claims of an existing spatial-index migration. Verify radius boundary, co-located ties, zero output, extreme coordinates and query-limit rejection.

## ECS authoring and queries

### F10. Optional query members

- **Evidence/gap:** `crates/ferravex-ecs/src/lib.rs:489`–611 implements sealed access for `&T`, `&mut T`, `Entity` and tuples. Tuple matching requires every member; `Option<&T>`/`Option<&mut T>` is not implemented. Existing tests around 1268 deliberately filter missing components before borrowing.
- **Deliverable:** optional component access returning `None` for absence while retaining entities matched by required accesses. Validate mutable overlap even when one component is optional; a present but conflicting borrow is an error, not `None`. Support iteration and existing `get_many` consistently.
- **Ownership/dependencies:** ECS `lib.rs`; serialize with F11–F14. Builds on J200 024/get_many safety. Parent can adopt a heterogeneous Position query with optional gameplay/presentation data in Neon Swarm. Verify absent/present components, optional-only queries, tuple aliasing and guard release on failure.

### F11. Component presence/absence filters without borrowing values

- **Evidence/gap:** the same ECS `Query` implementation at 559–611 offers conjunction of fetched access types only. `World::has` at 300 can be called manually, but there is no reusable `With<T>`/`Without<T>` filter that avoids fetching a marker component.
- **Deliverable:** additive `query_filtered<Q,F>` with composable bounded all/any/not presence filters. Separate structural predicates from access borrowing; a marker borrowed elsewhere must not make a presence-only filter fail. Deterministic slot order and fallible iteration remain unchanged.
- **Ownership/dependencies:** ECS `lib.rs`, after F10 or a parent-agreed common query refactor. Preserve `tags::TagPredicate`, which operates on tag values rather than component presence. Verify logical combinations, exclusions, no accidental marker borrows, empty filters and mutable-query aliases.

### F12. Practical actor bundles and wider queries

- **Evidence/gap:** `crates/ferravex-ecs/src/lib.rs:439`–469 and 609–612 stop bundle/query tuple implementations at four entries, including `Entity` in query width. A five-component actor or entity-plus-four-component system needs split operations despite the existing generic storage.
- **Deliverable:** extend sealed bundle/query tuples to an explicit small ceiling (eight), keeping compile-time typed access and prevalidation. Remove the per-spawn temporary metadata `Vec` in `validate_bundle` while touching tuple metadata, using bounded stack metadata rather than introducing reflection. Demonstrate an actor with position, velocity, health, faction and presentation data.
- **Ownership/dependencies:** ECS `lib.rs`, coordinate with F10/F11/F13. Existing demos inspected mostly use narrower bundles, so this is an authoring affordance, not a claim that they currently fail. Verify duplicates in late tuple positions, partial-borrow cleanup, empty bundles and old tuple behavior.

### F13. Prepared, bounded batch entity spawning

- **Evidence/gap:** `World::try_spawn` at `crates/ferravex-ecs/src/lib.rs:171` validates one bundle, grows slots and lazily creates component storage in `put` at 417. There is no reserve-by-component/bounded batch insertion API; `despawn_many` only addresses removal.
- **Deliverable:** explicit entity/component capacity preparation and a checked slice/fixed-array batch spawn path with caller-provided handle output. Preflight logical capacity, duplicate component types and output capacity before live entities are added. Define allocation failure/panic limits honestly; user component construction/Drop cannot be promised infallible. Keep ordinary `World::new`/spawn behavior.
- **Ownership/dependencies:** ECS `lib.rs`, after F12's metadata work. One owner must define any new capacity error type without adding variants casually to public exhaustive enums. Neon Swarm waves/projectiles are the clearest current adopter. Verify all-or-none reported errors, free-slot order, output-too-small and warmed prepared storage.

### F14. Explicit selected-component entity duplication

- **Evidence/gap:** ECS `World` at `crates/ferravex-ecs/src/lib.rs:148`–429 has no clone-selected-components operation; `Bundle` is sealed. Scene `Prefab`/`clone_subtree` in `crates/ferravex-play/src/scene.rs:244,761` clones hierarchy transforms, not arbitrary ECS components.
- **Deliverable:** small typed duplication plan for one/few entities, selecting only `Clone` component types, prevalidating all reads and producing a bounded old-to-new entity map. Exclude transient handles by default through explicit selection; leave reference remapping to an explicit host step. Do not add reflection, global serializers or a second scene hierarchy.
- **Ownership/dependencies:** ECS `lib.rs` or a parent-approved child module after F13. **Conditional adoption:** reserve only if entity archetype/editing workflows are wanted; no inspected demo currently requires duplication. Verify missing component/conflicting borrow produces no spawned entities and selected values copy without copying identity. Clone panics are a documented boundary.

## Math building blocks for authoring

### F15. Prepared 3D projection and screen-picking rays

- **Evidence/gap:** `crates/ferravex-math/src/matrix.rs:199,291` already has checked homogeneous projection/inversion. `crates/ferravex-graphics/src/camera.rs:3,14` has clip-depth conventions and a frustum, but no viewport-aware picking-ray helper. Render `Canvas2D::unproject` at `crates/ferravex-render/src/canvas.rs:35` only covers 2D letterboxing.
- **Deliverable:** a prepared 3D projector using an existing `Mat4` and cached inverse, explicit viewport/Y-axis and near/far NDC conventions, checked project/unproject/pick-ray results. Cover perspective and orthographic finite-far cameras first; reject unsupported infinite-far combinations explicitly rather than inventing finite endpoints.
- **Ownership/dependencies:** new math `projection.rs`; parent registers it and adapts graphics `ClipDepth` without introducing a math→graphics dependency. J200 026 is prerequisite. Physics-lab selection and world-space HUD anchors are likely consumers. Verify viewport offsets, behind-camera policy, singular matrices and round trips.

### F16. Prepared affine point/vector/normal transformation

- **Evidence/gap:** `crates/ferravex-math/src/matrix.rs:168` separates points/vectors but has no normal-transform authoring abstraction. `crates/ferravex-graphics/src/raster.rs:205`, render `lib.rs:484` and `ao.rs:205` separately form inverse-transpose transforms. Existing `scene::Transform` already supplies TRS and must not be duplicated.
- **Deliverable:** checked prepared affine transform holding the model and normal transform, with point/vector/normalized-normal and bounded slice operations. Validate the affine row and singular/reflected/nonuniform-scale cases once; define negative-determinant orientation behavior. Reuse matrix inversion and existing Vec/Mat types.
- **Ownership/dependencies:** new math `affine.rs`, parent registration and separate renderer adoption. J200 025/026. No new scene format or alternate Transform hierarchy. Verify normals remain perpendicular after nonuniform scale/shear, mirrored models, zero normals, invalid output and caller-buffer preservation.

### F17. Quaternion swing/twist authoring constraints

- **Evidence/gap:** `crates/ferravex-math/src/quaternion.rs:87,100,170,252` covers direction arcs, axis-angle, slerp and bounded rotate-towards, but not decomposition around an authored twist axis. `physics::JointDesc` has hinge axes/limits; those scalar hinge limits are not a reusable pose constraint.
- **Deliverable:** checked swing/twist decomposition and twist-limit application with reconstruction guarantees, normalized axis handling, explicit singular 180-degree swing result and shortest-angle conventions. Keep it a math operation suitable for aiming, bone alignment and authoring; no new solver joint type.
- **Ownership/dependencies:** `quaternion.rs`, after J200 027. Parent chooses a pose/aim consumer; online weapon behavior changes require separate policy/version decisions. Verify antipodal quaternions, axis reversal, near-singular cases and unchanged valid reconstruction.

### F18. Checked closest-point and segment-pair geometry

- **Evidence/gap:** `crates/ferravex-core/src/collision.rs:49,88` provides segment/AABB and segment/circle hit fractions; `play/navigation/route.rs` has whole-route projection. The checked math surface has no standalone segment projection/pair-distance result for tethers, aim proximity or track gates.
- **Deliverable:** finite 2D/3D point-to-segment and segment-to-segment closest results with clamped parameters, degenerate point segments, stable parallel/tie policy and f64 intermediates for extreme f32 coordinates. Return both witness points and distance, rather than forcing consumers to recreate them.
- **Ownership/dependencies:** new math `segments.rs`, parent registration. Do not replace route progress or collision sweeps. Parent may reuse this in route internals later under that owner's scope. Verify crossing, skew, parallel, identical/zero-length and overflow cases with behavioral geometry assertions.

### F19. Bounded cubic path authoring

- **Evidence/gap:** `crates/ferravex-play/src/navigation/route.rs:4,61,130` stores/samples a piecewise-linear Route3 with arc lengths; the inspected math/play/engine public implementations contain no cubic Bezier/Hermite path authoring primitive. Existing route projection is not missing.
- **Deliverable:** one cubic Bezier3 primitive with checked position/tangent sampling and bounded adaptive tessellation into caller storage, explicit error tolerance, subdivision/work ceiling and saturation outcome. Feed the produced polyline to existing `Route3`; do not build a competing navigation or spline-editor subsystem.
- **Ownership/dependencies:** new math `curves.rs`, parent registration; Route3 adapter/demo under its own owner. Dune Skimmer/Sky Courier authored paths are prospective uses. **Scope choice:** automatic road generation and curve arc-length inversion are excluded. Verify endpoints, cusps, straight/degenerate curves and strict output/work bounds.

## Physics authoring and accepted-event helpers

### F20. Direct overlap and separation queries

- **Evidence/gap:** `crates/ferravex-physics/src/api.rs:411,435` requires a nonzero direction even for zero-distance sphere casts; `RayHit` at 796 reports hit distance/point/normal, not penetration depth. Geometry `distance` already underlies these casts, so another collision kernel is unnecessary.
- **Deliverable:** direction-free point/sphere overlap queries with collider/body, separation or penetration depth and witness/normal into caller buffers. Reuse current-pose filtering, sensor policy and nearest/total conventions. Initially support point/ball probes against existing collider shapes; broad arbitrary-convex overlaps are excluded.
- **Ownership/dependencies:** shared dimensional `api.rs`, internal geometry only if required and assigned; J200 032. Parent registers any new exported result through existing d2/d3 include paths. Verify touching/inside/empty, compound children, stale ignore-body handles and immediate pose edits in both dimensions.

### F21. Atomic rigid-body teleport and motion reset

- **Evidence/gap:** `crates/ferravex-physics/src/api.rs:55`–144 exposes separate asserting pose, velocity, force and next-kinematic setters. Core in `body.rs:375` also retains force/torque and pending kinematic targets. J200 033's `Character::reset/teleport` operates on a different motor type. Marble Run `sim.rs:128` and Pinball recovery manually coordinate state.
- **Deliverable:** checked rigid-body pose/reset operation with explicit preserve/clear velocity policy, clearing accumulated forces and pending kinematic targets when requested, contact-cache invalidation and proper wake propagation. Reject the whole request before mutation; preserve body/collider/joint identity. Return the committed pose for host interpolation snap.
- **Ownership/dependencies:** physics `api.rs`, `body.rs`, and a narrow assigned world helper if neighbor wake is needed; J200 031/037. Parent owns game adoption and F02 snap wiring. Verify dynamic/fixed/kinematic cases, pending target after teleport, attached joints, bad rotation and next-step behavior. This is not collision-safe destination search.

### F22. Handle-preserving collider shape replacement

- **Evidence/gap:** `crates/ferravex-physics/src/world/compounds.rs:60,98,123` can attach/remove a collider or change its local pose; collider geometry is a private `Shape` in `body.rs:150`. No checked operation replaces shape while retaining the collider handle and validating the new compound mass.
- **Deliverable:** replace a collider's geometric description transactionally, retaining its generational identity and chosen material/layer/sensor settings, recomputing valid compound mass and invalidating contact caches. Reject invalid/zero-mass dynamic configurations before changing live state. Define whether body-origin velocity or momentum is preserved, matching existing mass-edit conventions by default.
- **Ownership/dependencies:** `world/compounds.rs` plus the minimum body/api authoring seam, assigned together; J200 031/036. Serialize with F24. Useful for lab editing and breakable geometry, without adding fracture simulation. Verify failed replacement leaves mass/pose/handle unchanged and successful replacement updates queries immediately.

### F23. World-space joint-frame authoring

- **Evidence/gap:** `crates/ferravex-physics/src/api.rs:850` accepts local anchors and d3 axes; `world/joints.rs:235,287,292` inserts joints and reads angle/anchors. Authors must manually transform a visible world pivot/axis into both body-local frames.
- **Deliverable:** checked builder/helper deriving local anchors, axes and reference rotation from two live bodies and a world-space pivot/frame, with a no-initial-snap mode. Reuse `JointDesc::validate`, existing joint kinds and world insertion; do not alter solver limits or motors.
- **Ownership/dependencies:** `world/joints.rs` and dimensional builder code where necessary; J200 034 and existing body transforms. Parent coordinates F24 use and lab adoption. Verify rotated/translated parents, fixed anchors, invalid/stale body handles, degenerate axes and retained initial relative pose in d2/d3.

### F24. Atomic multi-body assembly instantiation

- **Evidence/gap:** `world/compounds.rs:9` makes one body's collider set atomic; `world/joints.rs:235` inserts one joint. No operation validates a whole authored mechanism before multiple body/joint mutations. Physics-lab mechanisms and Pinball table construction compose these individually.
- **Deliverable:** bounded assembly description with local body IDs, collider children and local joint endpoints; preflight counts/geometry/mass/references before insertion, then return local-ID→handle mappings. Preserve world state on ordinary validation/capacity errors. Distinguish recoverable validation from allocation/destructor panic guarantees.
- **Ownership/dependencies:** new physics `assembly.rs`, narrowly assigned private insertion seam in world submodules if necessary. Depends on existing checked descriptors; F23 optional convenience. Parent owns root/dimensional registration. **Larger candidate:** require a design gate for allocation/rollback guarantees. Verify late invalid joint and body/collider/joint capacity before any published insertion.

### F25. Body-level sensor occupancy tracker

- **Evidence/gap:** `crates/ferravex-physics/src/world.rs:34,361` reports collider-pair Started/Stopped, including compound children and removed generations. Games needing one enter/exit per actor must maintain child-pair counts and remember identities after removal; this behavior is not provided by contact lifecycle itself.
- **Deliverable:** bounded opt-in tracker keyed by sensor and logical actor/body, consuming existing pair events plus caller-resolved identity. Emit enter on 0→1 and exit on 1→0, handle multiple child colliders, explicit actor removal/reset and overflow without inventing exits. Keep mapping entries long enough to resolve stop events after collider removal.
- **Ownership/dependencies:** new physics `triggers.rs` or engine adapter chosen by parent; no second collision detector. Use J200 contact/sensor baseline. Parent can adopt checkpoints, pickup volumes or lab sensors. Verify two children, repeated events, removal ordering, mapping reuse and incomplete/truncated input behavior.

### F26. Accepted impact aggregation across substeps

- **Evidence/gap:** `crates/ferravex-physics/src/world/cache.rs:61,111` exposes impulses from the last solved substep only. `world.rs:18` explicitly keeps transient contacts from all substeps. A fast impact can be absent from the final cache, and Started alone cannot quantify impact strength for audio/feedback.
- **Deliverable:** opt-in bounded outer-step impact records accumulated where constraints are solved, with stable pair identity, impact point/normal and documented impulse measure. Exclude warm-start reapplication/resting support from being counted as a new impact; expose dropped-record count and threshold/capacity policy.
- **Ownership/dependencies:** `world.rs` and assigned cache/solver hook, one owner. J200 037/contact lifecycle preserved. Parent adopts Pinball/lab sound or particles from accepted impacts. **Design-sensitive:** choose peak new normal impulse versus accumulated impulse explicitly before implementation. Verify a transient CCD contact, rest, multi-substep repeat and buffer saturation.

### F27. Bounded radial rigid-body impulse application

- **Evidence/gap:** `api.rs:97,105` applies an impulse to one body; engine spatial queries are 2D points and unsuitable for compound-body selection. Physics labs manually iterate bodies and compute mass-scaled kicks (`examples/physics-2d/src/lib.rs:242`, `physics-3d/src/lib.rs:263`).
- **Deliverable:** checked radial impulse request with explicit total impulse/falloff policy, layer filtering, one application per body despite multiple colliders, selected max-body/work bound and caller-buffer report of accepted bodies/impulses. Reuse F20 overlap candidates and existing impulse methods. No damage, particles or audio in the physics operation.
- **Ownership/dependencies:** new physics radial helper plus dimensional registration; F20 prerequisite. Parent decides saturation policy before effects are applied so query truncation cannot silently bias gameplay. Verify compound deduplication, zero-distance direction policy, static/sleeping bodies, finite extremes and deterministic ordering.

### F28. Character stance/clearance transition

- **Evidence/gap:** `character/motion.rs:369` takes a shape each step; `Character::teleport` at 331 resets velocity/jump state and can recover to a shifted pose. There is no transition that changes capsule/box height while keeping feet and motion state, refusing to stand under a ceiling.
- **Deliverable:** pure checked clearance query and stance-change helper over existing `Terrain`, `CharacterShape` and candidate budget. Preserve feet/velocity/jump state on success, leave everything unchanged on blocked expansion, and return explicit blocked/capacity/invalid outcomes. Initial scope is height change with fixed radius/planar width.
- **Ownership/dependencies:** `character/mod.rs`, `motion.rs`, `geometry.rs`, under one owner; J200 033 baseline. Parent controls any crouch input/game design and FPS wire implications separately. Verify ceiling blocks, shrinking, moving support and exhausted query budget; this alone must not add network crouch fields.

## Networking composition without protocol changes

### F29. Reusable bounded packet encoding

- **Evidence/gap:** `crates/ferravex-net/src/lib.rs:239,265,299` allocates a `Vec` for each header/packet; `wire_v4.rs:90` repeats that for extensions. FPS encodes input, ACKs, snapshots and reliable events every tick/poll.
- **Deliverable:** `encode_client_into`/`encode_server_into` using caller byte storage or a reusable bounded packet buffer, exact encoded length and output-capacity errors before publication. Cover all v5 kinds/extensions with the same validation as current encoders; old allocating functions delegate compatibly.
- **Ownership/dependencies:** net `lib.rs` and `wire_v4.rs`, one owner; serialize with F30/F35. Parent schedules FPS adoption. Verify byte-for-byte equivalence for existing fixtures and invalid/undersized output behavior. No protocol version bump is needed when bytes and accepted values remain identical.

### F30. Borrowed validated packet views

- **Evidence/gap:** `crates/ferravex-net/src/lib.rs:456,488` allocates input/player vectors and copies event payloads; `wire_v4.rs:79,140` copies extension payloads and builds timed-input vectors. Bounds are already enforced; missing capability is reusable read access, not another decoder policy.
- **Deliverable:** validated borrowed packet views with bounded iterators for inputs/players and borrowed event/delta payload slices. Fully validate counts, duplicate IDs, finite fields and trailing bytes before a view is trusted. Provide explicit conversion to the existing owned packets for queued data.
- **Ownership/dependencies:** net codec files after F29's shared encoding/validation seam, with parent registration for a new view module. Avoid `unsafe` layout casts. Test malformed tails before any application mutation, lifetime/ownership conversion and equivalence with existing decode results. Retaining a view cannot outlive its datagram buffer.

### F31. Reusable delta construction and application buffers

- **Evidence/gap:** `crates/ferravex-net/src/delta.rs:10,42` allocates a delta `Vec` and clones the baseline/output on every operation. Existing representation selection already avoids a 3× temporary; sparse/dense encoding and malformed-index rejection are present.
- **Deliverable:** checked `between_into`/`apply_into` paths with required sizes and explicit non-growing output policy; validate the complete sparse stream before changing destination. Reuse existing sparse/dense format and exact baseline tick requirement; support caller-owned storage suitable for replication caches.
- **Ownership/dependencies:** `delta.rs`, independent of F29 except shared packet-size constants. Parent migrates replication separately. Verify all existing representation boundaries, alias policy, late duplicate/out-of-range index, wrong baseline and small destination with unchanged bytes.

### F32. Send-aware reliable-event draining

- **Evidence/gap:** `crates/ferravex-net/src/reliable.rs:48` clones due packets and sets `last_sent` before transport execution. FPS `network.rs:98,446` treats `WouldBlock` as success after that timestamp has already advanced. Reliability retries eventually, but a local refusal unnecessarily postpones the next attempt.
- **Deliverable:** nonallocating callback/borrowed due-event access with explicit accepted/deferred/fatal submission result. Advance retry state only after accepted submission, preserve event IDs and ownership, bound work and leave unsent tail eligible. Keep the legacy `due` behavior available.
- **Ownership/dependencies:** `reliable.rs`; FPS flush adoption belongs to parent or F39/F40. Coordinate F34 on the same internal pending structure. Verify a deferred first packet, successful later retry, callback failure, ACK during subsequent polls and no duplicate sequence allocation.

### F33. Caller-budgeted reliable delivery

- **Evidence/gap:** `reliable.rs:87` buffers out-of-order data, then drains all contiguous packets into a new `Vec`. FPS already implements delivered-event backpressure (`network.rs:630`); do not reintroduce it. Missing is the ability to admit a packet and deliver only the application's current budget without an intermediate output allocation.
- **Deliverable:** separate checked receive/admission and bounded ordered drain-to-callback/caller-buffer. Clarify ACK means safely retained, not yet applied; keep enough capacity for admitted contiguous events until drained. Return duplicate/window/size outcomes and never advance delivery sequence past undispatched data.
- **Ownership/dependencies:** `reliable.rs`, serialized with F32/F34. Parent adapts both command and server-event consumers. Preserve individual ACK and exactly-once ordered delivery. Verify gap closure larger than delivery budget, duplicate packets, full buffer and delayed application draining.

### F34. Configurable bounded retransmission policy

- **Evidence/gap:** `reliable.rs:6` hardcodes `RETRY_MS = 100`; pending records retain only `last_sent`. There is no caller-supplied retry interval/backoff or explicit terminal policy for an event that cannot be acknowledged.
- **Deliverable:** checked opt-in retry policy with minimum/maximum delay, bounded backoff and attempt/age exhaustion outcome using supplied monotonic time. Default remains 100 ms. Exhaustion returns a channel/session failure decision to the host; it must never drop an ordered event and continue later sequence delivery across the gap.
- **Ownership/dependencies:** `reliable.rs`, after F32; consuming disconnect policy in F39/F40. No RTT-dependent randomness or wall-clock reads in the primitive. Verify timing boundaries, overflow, backwards supplied time and permanent loss. This is a transport behavior feature, not only counters.

### F35. Checked input-batch admission with explicit semantics

- **Evidence/gap:** `crates/ferravex-net/src/lib.rs:190` returns one boolean for invalid, stale/duplicate or full input queues. FPS extension handling at `network/replication.rs:34` loops and ignores each result. `pop_latest_timed` already preserves action bits and older shot aim/time and must remain authoritative behavior.
- **Deliverable:** explicit per-input admission outcome and bounded packet-batch ingestion that prevalidates malformed inputs before enqueueing, skips benign redundancy deliberately, and reports capacity rejection without advancing the watermark for rejected entries. Document partial admission policy rather than silently choosing atomic versus best effort.
- **Ownership/dependencies:** net `lib.rs`, serialized after codec edits; parent/F39 adopts it. Preserve v5 packets and one authority step per tick. Verify wrap, redundant batches, queue full mid-batch, quick-fire coalescing and invalid last item. No retransmitted command may create extra simulation ticks.

### F36. Configurable interpolation delay with bounded adaptation

- **Evidence/gap:** `crates/ferravex-net/src/clock.rs:107` computes `(6 + jitter*hz*2).ceil()` clamped to 6–12 ticks. This is already adaptive delay, but its range is fixed in ticks and has no controlled adjustment rate; FPS clamps its render target separately at `network/replication.rs:292`.
- **Deliverable:** opt-in delay policy expressed in explicit time/tick units with validated min/max and increase/decrease rates, plus a stateful playout cursor that never moves backwards when delay grows. Default behavior stays accessible. Bound catch-up and hold behavior instead of extrapolating outside retained history.
- **Ownership/dependencies:** `clock.rs`, and a new playout module if cleaner; parent owns FPS integration. Coordinate F37. Preserve shot stamps based on the actual rendered authority tick. Verify 30/64/120 Hz, jitter spikes, recovery, reordered probes and no backwards remote pose progression.

### F37. Clock freshness and re-anchoring policy

- **Evidence/gap:** `clock.rs:110` estimates from an anchor with elapsed time clamped to -0.25..5 seconds, but it does not distinguish a stale estimate from a fresh one. Reset currently requires replacing `ClientClock`; FPS timeout separately clears the connection.
- **Deliverable:** additive sample status carrying freshness/confidence bounds and a supplied-time expiry/reset/re-anchor operation with a documented outstanding-probe policy. Let hosts choose fallback interpolation or a new probe before stale timing is used, without altering the existing `estimated_tick` behavior.
- **Ownership/dependencies:** `clock.rs`, after/common owner with F36. Parent wires appropriate FPS fallback; this does not change peer timeout or authorize old shot ticks. Verify long silent interval, clock reversal, large valid microsecond values, delayed pre-reset pong and wrapped tick re-anchor.

### F38. Reusable acknowledged snapshot-baseline store

- **Evidence/gap:** `crates/ferravex-fps/src/network/replication.rs:5,39,81,195` manually maintains 32 sent/received byte snapshots, validates exact ACKs, picks deltas and requests full recovery. `net::delta` deliberately leaves all baseline ownership to callers.
- **Deliverable:** bounded net baseline store with exact sent-tick acknowledgement, byte/capacity limits, stable eviction, session reset and full/delta selection outcomes. Reuse `Delta`; separate sender and receiver responsibilities. Unknown ACKs never authorize a baseline; a newer tick alone is insufficient proof of retained bytes.
- **Ownership/dependencies:** new net `baselines.rs`, parent registration, F31 optional buffer reuse. FPS adoption occurs with F39/F40 under the replication owner. Preserve session checks and existing full-request rate limiting. Verify lost baseline, old ACK, tick wrap, membership-induced length change and reconnect.

### F39. Transport-independent authority protocol driver

- **Evidence/gap:** `crates/ferravex-fps/src/network.rs:128,167,305` mixes packet dispatch, `Socket`, `Instant::now`, peer timeouts, gameplay stepping and output. Its test socket wrapper at 46 uses bounded real-time sleeps. `Match` itself is already pure, so do not rewrite the simulation.
- **Deliverable:** extract an authority protocol state with `receive(peer, bytes, now)` and explicit tick/poll operations producing bounded outgoing datagrams through caller storage/callbacks. Inject cookie generation at the host boundary. Retain `GameServer` as the std UDP adapter with current defaults and authority rules.
- **Ownership/dependencies:** parent first assigns/extracts `network.rs` and `network/{replication,tactical}.rs` portions to avoid concurrent F40 edits; new `network/server_state.rs` is a likely home. F29/F32/F35/F38 are useful prerequisites. **Large but bounded:** keep protocol v5 and tactical semantics unchanged. Verify supplied-time handshake, timeout, input rate and reliable backpressure against existing behavior; no socket needed for core tests.

### F40. Transport-independent prediction client driver

- **Evidence/gap:** `network.rs:462,579,609` owns a concrete socket plus prediction, pending input, reconnect and clock probes; `network/replication.rs:195` performs reconciliation and immediately sends ACKs. Wasm `browser.rs` bridges through thread-local queues, not an independently instantiable protocol client.
- **Deliverable:** extracted client state accepting datagrams/supplied time, ticking local prediction, and yielding bounded outbound datagrams/events. Preserve `GameClient` wrappers and local-menu command suppression while authority polling continues. Make session reset clear baselines, pending predictions, event channels and probes together.
- **Ownership/dependencies:** after the parent-defined F39 extraction seam, own new `network/client_state.rs` plus specifically assigned replication portions. F29/F32/F33/F38. No generic transport framework beyond the concrete datagram boundary. Verify reconnect/delayed old welcome, pending-input replay, event capacity and no wall-clock reads in the extracted core.

### F41. Bounded in-memory datagram link for playable local sessions

- **Evidence/gap:** net currently exposes real UDP sockets (`crates/ferravex-net/src/lib.rs:590`) and the native WebSocket gateway; FPS browser socket queues are global per thread (`crates/ferravex-fps/src/browser.rs:20,49`). There is no reusable independently instantiable local link for two protocol drivers in one host.
- **Deliverable:** an in-memory pair/router with explicit peer IDs, byte/packet capacity, supplied-time delivery, FIFO equal-time order and optional deterministic delay/loss/reorder settings. It must run actual authority/client protocol state in a local game or network inspector, not just test assertions; never synthesize ACKs or game outcomes.
- **Ownership/dependencies:** new net `local_link.rs`, parent registration; meaningful end-to-end use depends on F39/F40. Default reliable local queue mode should be simple; impairment mode has a bounded deterministic policy. No thread, socket, browser or external RNG dependency. Verify two independent sessions and queue saturation through an actual headless match.

## FPS authoring, game feel and efficient adoption

### F42. Checked reusable arena authoring

- **Evidence/gap:** `crates/ferravex-fps/src/lib.rs:48,98` exposes raw `solids`/`spawns` vectors and a fallback spawn at zero. `Match::new` accepts them directly; validation exists later in checkpoint encoding, not as an authoring constructor with source indices. The arena supports axis-aligned boxes; arbitrary mesh collision is not implied.
- **Deliverable:** checked arena builder/validator with explicit solid/spawn caps, indexed nonfinite/negative-extents errors and optional character-volume spawn-clearance validation. Produce the same `Arena`, no new map format. Keep raw struct literals and legacy construction for compatibility.
- **Ownership/dependencies:** new FPS `arena_authoring.rs`, parent registration; reuse existing `Arena`/character geometry and checkpoint limits. Parent adopts Courtyard scene editing and Strike setup. Verify malformed last solid, empty spawn policy, occluded spawn and large finite inputs. Do not alter fixed 64 Hz rules or replay context bytes accidentally.

### F43. Opt-in deterministic safe spawn selection

- **Evidence/gap:** `Arena::spawn` at `fps/src/lib.rs:98` picks `id % spawns.len`; dead-player recovery at 415 uses that same location. It does not score current occupancy or exposure, so a useful spawn choice requires game-specific code despite existing collision/raycast primitives.
- **Deliverable:** bounded selector over existing spawn points with static clearance, live-player distance and optional line-of-sight cost, deterministic index ties and explicit no-safe-candidate fallback. Integrate through an opt-in authority spawn policy, leaving current practice rules unchanged by default.
- **Ownership/dependencies:** new FPS `spawn.rs` plus a narrow assigned Match/rules hook; F42 useful. Parent owns live game-design adoption. This changes gameplay when enabled: replay must record the selected authoritative outcome or include policy/context versioning. Verify all candidates blocked, one safe point, dead players, same seed/order and no client-owned spawn decision.

### F44. Configurable bot navigation domain

- **Evidence/gap:** `crates/ferravex-fps/src/bots.rs:8,48` hardcodes a 48×56 grid with origin offsets -23.5/-27.5, 0.45 clearance and fixed planner budgets. `PracticeBots::new(&Arena)` accepts other arenas but still rasterizes that fixed domain.
- **Deliverable:** validated `BotNavigationConfig` for origin, cell size, dimensions, clearance and explicit work caps, plus checked construction/rebuild against an arena. Existing `new` uses identical defaults. Reuse `Grid`/`PathPlanner`/`WorkBudget`; no new pathfinder.
- **Ownership/dependencies:** `bots.rs`, after J200 navigation/AI APIs; coordinate F45. F42 can supply arena bounds. Parent adopts authored Courtyard/Strike variations only after behavior review. Verify off-origin maps, boundary cells, excessive dimensions and rebuild clearing stale routes/reservations.

### F45. Roster-aware practice bot targets and scheduling

- **Evidence/gap:** `bots.rs:115,135` only hears shots from player 0, always pushes a local command for 0 and targets player 0. Its round-robin loop indexes `game.players[index + 1]`, while `Match::spawn_player` appends IDs in caller order. This is a concrete single-human/ordering assumption, not a lack of perception or squad AI.
- **Deliverable:** explicit controlled/local roster and hostility/target-selection policy; schedule live bot IDs independently of vector position, perceive bounded eligible targets and keep noise/squad source IDs intact. Preserve current practice defaults and accepted `InputFrame` submission.
- **Ownership/dependencies:** `bots.rs`, after or combined-seam with F44; reuse J200 target hysteresis/squad reservations rather than duplicating them. Parent chooses multiplayer/offline co-op adoption. Verify shuffled player order, nonzero local ID, disconnect/reuse, no hostile candidates and work budget across multiple targets.

### F46. Reusable authoritative step output

- **Evidence/gap:** `crates/ferravex-fps/src/lib.rs:287` allocates fresh events and rejected-shot vectors every step; `rewind.rs:30` returns them in `RewindStep`. Max player count bounds ordinary work, but callers cannot supply retained output storage.
- **Deliverable:** reusable `StepOutput`/`step_with_rules_into` that retains event/rejection buffers, validates output capacity before committing a tick or uses a documented bounded sink, and returns counts. Preserve event order and existing convenience APIs. Avoid truncating accepted combat events to fit a tiny buffer.
- **Ownership/dependencies:** FPS `lib.rs` and `rewind.rs`, same owner; parent migrates network/replay loops. F48 must use the same output seam if selected. Verify max simultaneous shots/hits/kills/respawns, undersized buffer, invalid rewind and byte/event equivalence with old wrappers.

### F47. Presentation-only prediction correction smoothing

- **Evidence/gap:** `network/replication.rs:232` replays pending inputs over the authority snapshot and replaces `predicted` directly. Remote players already have epoch-aware interpolation (`rendered_players` at 320). `local_player` at `network.rs:552` exposes corrected simulation state with no local visual correction adapter.
- **Deliverable:** emit a bounded local reconciliation observation and maintain a presentation-only positional correction offset with checked decay/snap thresholds. Snap on life/session discontinuity or large correction; leave predicted collision, authoritative aim and shot origin untouched. Reuse smoothing/F02 where appropriate.
- **Ownership/dependencies:** preferably after F40; new FPS presentation helper plus assigned reconciliation observation hook. Parent controls camera/render use and reduced-motion policy. **Scope choice:** no speculative hit confirmation or smoothing of damage/ammo. Verify repeated corrections, zero render dt, disconnect, respawn and exact simulation-state equality with smoothing disabled/enabled.

### F48. Accepted weapon-transition results for feedback

- **Evidence/gap:** `fps/src/lib.rs:402` returns only whether a shot fired from `step_player_with_settings`; reload start/finish and jump acceptance happen inside the step. `GameEvent` at 192 contains Shot/Hit/Killed/Respawned only. General play magazine/action previews from J200 do not describe these actual FPS transitions.
- **Deliverable:** additive detailed local step result reporting accepted reload start/completion and jump/fire outcomes at the actual tick. Preserve the bool wrapper and core event enum. Use a sidecar or callback so hosts emit sounds/animation from accepted transitions instead of sampling changing fields or raw buttons.
- **Ownership/dependencies:** FPS `lib.rs`/`combat.rs`, after F46's output design; parent adopts offline Strike/Courtyard first. Network/replay serialization of new event kinds is explicitly excluded unless separately versioned. Verify invalid settings, movement rejection, held reload, completion+fire in one tick and no duplicate completion during replay prediction.

### F49. Authority-owned player relocation with history invalidation

- **Evidence/gap:** `fps/src/lib.rs:267` exposes `player_mut`, allowing position edits without touching rewind/life epochs. Private `reset_round_players` at 241 coordinates history clearing and new lives; `rewind.rs:65,70,112` uses epochs to prevent old-life hits. There is no public checked relocation operation joining these behaviors.
- **Deliverable:** explicit Match relocation/respawn helper with finite/clearance validation, chosen motion reset policy and an epoch/discontinuity transition so rewind and remote interpolation cannot bridge a teleport. Preserve other players' valid history where possible and define same-tick capture behavior. Return the accepted destination for F02/presentation reset.
- **Ownership/dependencies:** FPS `lib.rs`, `rewind.rs`, potentially F42 clearance. Parent wires game/editor use; does not expose teleport authority to clients. Existing v5 epochs may carry the discontinuity without new packet fields, but inspect low-32-bit epoch semantics and checkpoint consistency before claiming compatibility. Verify stale pre-teleport shots, repeated same-tick relocations, invalid destination and unaffected other players.

## Ownership sequencing and integration cautions

- **ECS serial lane:** F10 → F11 → F12 → F13; F14 is optional after F13. F06 belongs in engine and should use stable ECS APIs, not reach through private storage. F08 is a separate pool lane, not another ECS bulk task.
- **Math lane:** F15/F16 are separate new modules sharing existing checked Mat4 operations; parent chooses one convention adapter for graphics. F17 changes quaternion.rs; F18/F19 are independent new modules. Do not relocate public existing types or duplicate `scene::Transform`, `Route3` or `ClipDepth` without adapters.
- **Physics lane:** the same `api.rs` is included for both dimensions, so F20/F21/F23 cannot independently rewrite it at once. F22/F24 share compound insertion logic. F26 owns the actual solver aggregation hook; F25 merely consumes contact lifecycle. F27 depends on F20's completeness contract. F28 stays within character code and adds no FPS protocol fields.
- **Codec lane:** F29 → F30 and F35 share net/lib.rs + wire_v4.rs. F31 and new F38 can run in separate files. F32 → F33 → F34 share reliable.rs unless assigned to one owner. F36/F37 share clock.rs.
- **Protocol extraction lane:** parent defines server/client boundaries in the existing network monolith before F39/F40 run. A new standalone file with an unconnected copy is not completion. Preserve native and Wasm wrappers, tactical actions, accepted events, tick rate, current full-baseline recovery and menu progression. F41 requires both drivers to be connected to real Match/prediction behavior.
- **FPS lane:** F42 can start as a new module; F44/F45 share bots.rs. F43/F46/F48/F49 share lib.rs and must be serialized or given explicit submodule extractions. F47 follows stable reconciliation hooks. Any optional gameplay policy that changes replay results needs explicit context/version coordination, even if packet layout stays v5.
- **Public enum compatibility:** adding variants to currently exhaustive enums can break downstream exhaustive matches. Prefer separate additive error/result types for new operations, or ask parent to coordinate a compatibility change deliberately. The phrase “additive” alone does not make an enum edit source-compatible.

## Adoption handoff for all fifteen games

These foundation tasks do not replace fifteen separately owned polish/adoption tasks. Not every game should adopt ECS, physics or networking. The desired outcome is less repeated orchestration and better accepted-event behavior, with each game's existing design intact.

- **All fifteen:** assess F01 and, where poses interpolate, F02; keep current session/input rules and accepted-feedback plumbing. This is a candidate migration, not evidence that all fifteen currently need new code.
- **Emberline, Neon Swarm, Orchard Guard, Crystal Cavern, Siege Foundry, Sky Courier, Pocket Rally, Dune Skimmer, Marble Run, Salvage Pinball, Strike, 2D Physics Lab, 3D Physics Lab, Courtyard (`owned-lab`), Orbit Breaker (`owned-orbit`)** remain the explicit catalog coverage set. Parent should reserve one concrete consumer/visual acceptance scenario for each rather than counting unused engine exports as adoption.
- **Neon Swarm:** confirmed ECS loops in `examples/neon-swarm/src/game.rs:330,356,393,526,533`; F06/F10/F11/F13 can reduce local query/spawn/removal glue. Preserve existing rules and input regression tests.
- **Marble Run:** confirmed pose history in `sim.rs`/`trial.rs`; F02/F21 are direct candidates for fall/checkpoint recovery. **Salvage Pinball:** pose edits, impulse and bumper contacts in `src/lib.rs:295,307,320` make F21/F26/F27 concrete candidates.
- **2D/3D Physics Labs:** current body kicks and contact event feedback support F20/F21/F23/F26/F27; F15 is a new selection feature, not an existing picking defect. Parent should keep lab learning goals visible and reserve real desktop/narrow-layout interaction checks.
- **Strike/Courtyard:** confirmed FPS clients/bots (`examples/strike/src/multiplayer.rs:189`, `src/bots.rs:2`, `examples/owned-lab/src/lib.rs`). F29–F49 are selective opportunities; authority/menu/replay semantics are mandatory regression boundaries. Do not force networking into unrelated games.
- **Other games:** F09/F18/F19 and scoped jobs are plausible targeting, gate, flight-path or delayed-event helpers. No current source defect is asserted for those consumers; parent should confirm a concrete call-site simplification or visible feature before assigning adoption.

## Acceptance template for later implementation tasks

Every selected task needs behavior tests in its owned module or unique `tests/journey400_NNN.rs`, a compiling public usage example, exact registrations and an actual consumer handoff. Use a narrow `cargo test -p PACKAGE --offline --locked -j 2` with relevant module/test/features, then owned-file formatting checks. Physics owners exercise dim2 and dim3 paths as applicable. Codec/reliability work preserves versioned byte fixtures, authority rejection and ordering semantics. Allocation-sensitive claims need evidence beyond successful compilation; capacity tests alone do not establish zero allocations.

Parent performs combined facade/native/Wasm checks, shared staging and gameplay/visual verification. Do not infer any of those from this research or earlier receipts. The lower-confidence scope decisions are F08's first pool consumer, F14's need for ECS duplication, F19's authored-curve consumer, F24's transaction guarantees, F26's impact measure, and the exact extraction boundaries for F39/F40. The implementation gaps at the cited APIs are concrete; whether these belong ahead of game-specific polish remains a parent prioritization decision.
