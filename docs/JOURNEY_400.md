# Journey400: 200 additional improvements

This is the second 200-item expansion, numbered **201–400**. It preserves Journey50 and [Journey200](JOURNEY_200.md). The current release closes the dispatched batch; the remaining 117 items are deferred at the user's request. Platform verification limits remain explicit. These are concrete assignments; see the [stopping-point release](STOPPING_POINT.md) for delivered scope and deferred work.

<!-- journey400-status:start -->
**Stopping point:** 83 numbered implementations returned; no numbered agents remain active. The remaining 117 tasks are deferred at the user's request. Combined release verification and platform limits are recorded in [STOPPING_POINT.md](STOPPING_POINT.md). Two planning agents and a focused release review are supplementary, not numbered improvements.
<!-- journey400-status:end -->

The runtime permits three active child agents alongside the parent. Each numbered item gets a distinct implementation or verification agent, dispatched in dependency waves. Existing Journey200 work shares that limit. A returned receipt is reviewed and integrated before dependent consumers rely on its API.

Every item has a detailed brief, owned paths, dependency IDs and validation requirements in [plan.json](journey400/plan.json). Actual task IDs and receipts are tracked in [tasks.json](journey400/tasks.json). The [working contract](journey400/contract.md) and [orchestration handoff](journey400/ORCHESTRATION.md) define dispatch and integration. Source audits: [foundation](journey400/proposals/foundation.md) and [presentation](journey400/proposals/presentation.md).

New capabilities must preserve existing defaults and wire/save compatibility. Avoid duplicate systems: reuse the current session, input, physics, UI, audio, scene, replay and validation implementations. All fifteen demos receive a substantive feature, a shared presentation migration, a semantic host integration and feature-specific regression/playability coverage. Custom software hosts retain their rendering paths through pure-model adapters.

## Engine and ECS composition

### 201. Fixed-tick context and absolute scheduled IDs

additive tick-batch/context API yielding the scheduled tick ID, index within batch and fixed delta, plus a defaulted contextual `Game` hook forwarding to the existing hook. Keep scheduled and actually executed counts distinct. Handle zero work, early stop, manual stepping, reset and wrapping explicitly without allocating or changing `FrameStep` struct literals.

- Owner: `crates/ferravex-core/src/lib.rs`, `crates/ferravex-core/src/session.rs`, `crates/ferravex-engine/src/app.rs`.
- Dependencies: independent.

### 202. Reusable interpolation history and presentation holds

a small `InterpolationPair<T>`/pose adapter supporting commit-on-tick, sample-with-interpolator, snap/reset and explicit discontinuity generation. Include an explicit presentation hold/resume operation so an inactive session's zero alpha does not move the image backwards. Two retained values, no clock or input ownership; Vec2/Vec3/Quat examples.

- Owner: `crates/ferravex-engine/src/interpolation.rs`.
- Dependencies: independent.

### 203. Safe disjoint resource access

checked two-/three-resource access or scoped callbacks with mutable and shared combinations, duplicate-TypeId/missing-type errors before invoking user code, and ordinary Rust references. This lets inventory/objective or simulation/feedback systems compose without remove/reinsert choreography. Keep the 256-type cap and avoid cloning or `unsafe`.

- Owner: `crates/ferravex-engine/src/resources.rs`.
- Dependencies: independent.

### 204. Prepared sequential system runner

`SystemSchedule<C,E>` registering bounded named callbacks against the existing `SystemOrder`, resolving once before use, and executing a stable sequence with explicit error/stop and flush boundaries. Include opt-in run predicates and a preparation error for a missing callable. No parallel executor, automatic borrow inference or new dependency graph.

- Owner: `crates/ferravex-engine/src/systems.rs`.
- Dependencies: 201.

### 205. Fallible deferred commands with explicit failure policy

additive fallible command queue/adapter accepting `FnOnce(&mut W) -> Result<(), E>`, bounded application with command ID and error, and explicit stop-on-error or continue policy. Preserve untouched tail order and return rejected closures. Document that commands may partially mutate before returning an error and are never automatically retried.

- Owner: `crates/ferravex-engine/src/commands.rs`.
- Dependencies: independent.

### 206. ECS-specific deferred entity authoring

bounded ECS command adapter with spawn tokens local to one buffer epoch, insert/remove/despawn operations targeting existing entities or earlier tokens, and an apply result mapping tokens to real entities. Use checked ECS operations; report invalid/stale dependencies and consume tokens only once. Explicit partial-application policy rather than claiming arbitrary closures are transactional.

- Owner: `crates/ferravex-engine/src/ecs_commands.rs`.
- Dependencies: 205, 213.

### 207. Owner-scoped tick jobs and cancellation

bounded owner/scope tokens and scope cancellation on the existing scheduler, preserving original FIFO ties and returning/discarding payloads under a documented policy. A generation on scope reuse prevents an old scene handle from canceling new jobs. Scope cancellation must not reset unrelated deadlines or reuse task IDs.

- Owner: `crates/ferravex-engine/src/scheduler.rs`.
- Dependencies: 201.

### 208. Disjoint mutation of pooled objects

checked selected-pair/many borrowing using safe disjoint slot access, preserving caller order and rejecting duplicate/stale handles before returning any references. Include an actual transfer/collision-response example between two live pooled actors; no remove/reinsert handle churn.

- Owner: `crates/ferravex-engine/src/pool.rs`.
- Dependencies: independent.

### 209. Distance-ranked proximity queries

checked radius and nearest-k point queries returning ID, point and squared distance into caller storage, stable distance/ID ties, explicit total/truncation and the existing cell-visit ceiling. Search all candidates in the bounded region before choosing nearest-k, so early ID truncation cannot hide the closest target.

- Owner: `crates/ferravex-engine/src/spatial.rs`.
- Dependencies: independent.

### 210. Optional query members

optional component access returning `None` for absence while retaining entities matched by required accesses. Validate mutable overlap even when one component is optional; a present but conflicting borrow is an error, not `None`. Support iteration and existing `get_many` consistently.

- Owner: `crates/ferravex-ecs/src/lib.rs`.
- Dependencies: independent.

### 211. Component presence/absence filters without borrowing values

additive `query_filtered<Q,F>` with composable bounded all/any/not presence filters. Separate structural predicates from access borrowing; a marker borrowed elsewhere must not make a presence-only filter fail. Deterministic slot order and fallible iteration remain unchanged.

- Owner: `crates/ferravex-ecs/src/lib.rs`.
- Dependencies: 210.

### 212. Practical actor bundles and wider queries

extend sealed bundle/query tuples to an explicit small ceiling (eight), keeping compile-time typed access and prevalidation. Remove the per-spawn temporary metadata `Vec` in `validate_bundle` while touching tuple metadata, using bounded stack metadata rather than introducing reflection. Demonstrate an actor with position, velocity, health, faction and presentation data.

- Owner: `crates/ferravex-ecs/src/lib.rs`.
- Dependencies: 210, 211.

### 213. Prepared, bounded batch entity spawning

explicit entity/component capacity preparation and a checked slice/fixed-array batch spawn path with caller-provided handle output. Preflight logical capacity, duplicate component types and output capacity before live entities are added. Define allocation failure/panic limits honestly; user component construction/Drop cannot be promised infallible. Keep ordinary `World::new`/spawn behavior.

- Owner: `crates/ferravex-ecs/src/lib.rs`.
- Dependencies: 210, 211, 212.

### 214. Explicit selected-component entity duplication

small typed duplication plan for one/few entities, selecting only `Clone` component types, prevalidating all reads and producing a bounded old-to-new entity map. Exclude transient handles by default through explicit selection; leave reference remapping to an explicit host step. Do not add reflection, global serializers or a second scene hierarchy.

- Owner: `crates/ferravex-ecs/src/lib.rs`.
- Dependencies: 210, 211, 212, 213.

## Geometry and physical authoring

### 215. Prepared 3D projection and screen-picking rays

a prepared 3D projector using an existing `Mat4` and cached inverse, explicit viewport/Y-axis and near/far NDC conventions, checked project/unproject/pick-ray results. Cover perspective and orthographic finite-far cameras first; reject unsupported infinite-far combinations explicitly rather than inventing finite endpoints.

- Owner: `crates/ferravex-math/src/projection.rs`.
- Dependencies: independent.

### 216. Prepared affine point/vector/normal transformation

checked prepared affine transform holding the model and normal transform, with point/vector/normalized-normal and bounded slice operations. Validate the affine row and singular/reflected/nonuniform-scale cases once; define negative-determinant orientation behavior. Reuse matrix inversion and existing Vec/Mat types.

- Owner: `crates/ferravex-math/src/affine.rs`.
- Dependencies: independent.

### 217. Quaternion swing/twist authoring constraints

checked swing/twist decomposition and twist-limit application with reconstruction guarantees, normalized axis handling, explicit singular 180-degree swing result and shortest-angle conventions. Keep it a math operation suitable for aiming, bone alignment and authoring; no new solver joint type.

- Owner: `crates/ferravex-math/src/quaternion.rs`.
- Dependencies: independent.

### 218. Checked closest-point and segment-pair geometry

finite 2D/3D point-to-segment and segment-to-segment closest results with clamped parameters, degenerate point segments, stable parallel/tie policy and f64 intermediates for extreme f32 coordinates. Return both witness points and distance, rather than forcing consumers to recreate them.

- Owner: `crates/ferravex-math/src/segments.rs`.
- Dependencies: independent.

### 219. Bounded cubic path authoring

one cubic Bezier3 primitive with checked position/tangent sampling and bounded adaptive tessellation into caller storage, explicit error tolerance, subdivision/work ceiling and saturation outcome. Feed the produced polyline to existing `Route3`; do not build a competing navigation or spline-editor subsystem.

- Owner: `crates/ferravex-math/src/curves.rs`.
- Dependencies: 218.

### 220. Direct overlap and separation queries

direction-free point/sphere overlap queries with collider/body, separation or penetration depth and witness/normal into caller buffers. Reuse current-pose filtering, sensor policy and nearest/total conventions. Initially support point/ball probes against existing collider shapes; broad arbitrary-convex overlaps are excluded.

- Owner: `crates/ferravex-physics/src/api.rs`, `crates/ferravex-physics/src/geometry.rs`.
- Dependencies: independent.

### 221. Atomic rigid-body teleport and motion reset

checked rigid-body pose/reset operation with explicit preserve/clear velocity policy, clearing accumulated forces and pending kinematic targets when requested, contact-cache invalidation and proper wake propagation. Reject the whole request before mutation; preserve body/collider/joint identity. Return the committed pose for host interpolation snap.

- Owner: `crates/ferravex-physics/src/api.rs`, `crates/ferravex-physics/src/body.rs`, `crates/ferravex-physics/src/world.rs`.
- Dependencies: 220.

### 222. Handle-preserving collider shape replacement

replace a collider's geometric description transactionally, retaining its generational identity and chosen material/layer/sensor settings, recomputing valid compound mass and invalidating contact caches. Reject invalid/zero-mass dynamic configurations before changing live state. Define whether body-origin velocity or momentum is preserved, matching existing mass-edit conventions by default.

- Owner: `crates/ferravex-physics/src/world/compounds.rs`, `crates/ferravex-physics/src/body.rs`, `crates/ferravex-physics/src/api.rs`.
- Dependencies: 220, 221.

### 223. World-space joint-frame authoring

checked builder/helper deriving local anchors, axes and reference rotation from two live bodies and a world-space pivot/frame, with a no-initial-snap mode. Reuse `JointDesc::validate`, existing joint kinds and world insertion; do not alter solver limits or motors.

- Owner: `crates/ferravex-physics/src/world/joints.rs`, `crates/ferravex-physics/src/api.rs`.
- Dependencies: 220, 221, 222.

### 224. Atomic multi-body assembly instantiation

bounded assembly description with local body IDs, collider children and local joint endpoints; preflight counts/geometry/mass/references before insertion, then return local-ID→handle mappings. Preserve world state on ordinary validation/capacity errors. Distinguish recoverable validation from allocation/destructor panic guarantees.

- Owner: `crates/ferravex-physics/src/assembly.rs`, `crates/ferravex-physics/src/world/compounds.rs`, `crates/ferravex-physics/src/world/joints.rs`.
- Dependencies: 222, 223.

### 225. Body-level sensor occupancy tracker

bounded opt-in tracker keyed by sensor and logical actor/body, consuming existing pair events plus caller-resolved identity. Emit enter on 0→1 and exit on 1→0, handle multiple child colliders, explicit actor removal/reset and overflow without inventing exits. Keep mapping entries long enough to resolve stop events after collider removal.

- Owner: `crates/ferravex-physics/src/triggers.rs`.
- Dependencies: independent.

### 226. Accepted impact aggregation across substeps

opt-in bounded outer-step impact records accumulated where constraints are solved, with stable pair identity, impact point/normal and documented impulse measure. Exclude warm-start reapplication/resting support from being counted as a new impact; expose dropped-record count and threshold/capacity policy.

- Owner: `crates/ferravex-physics/src/world.rs`, `crates/ferravex-physics/src/world/cache.rs`.
- Dependencies: 221.

### 227. Bounded radial rigid-body impulse application

checked radial impulse request with explicit total impulse/falloff policy, layer filtering, one application per body despite multiple colliders, selected max-body/work bound and caller-buffer report of accepted bodies/impulses. Reuse J400 220 overlap candidates and existing impulse methods. No damage, particles or audio in the physics operation.

- Owner: `crates/ferravex-physics/src/radial.rs`.
- Dependencies: 220.

### 228. Character stance/clearance transition

pure checked clearance query and stance-change helper over existing `Terrain`, `CharacterShape` and candidate budget. Preserve feet/velocity/jump state on success, leave everything unchanged on blocked expansion, and return explicit blocked/capacity/invalid outcomes. Initial scope is height change with fixed radius/planar width.

- Owner: `crates/ferravex-physics/src/character/mod.rs`, `crates/ferravex-physics/src/character/motion.rs`, `crates/ferravex-physics/src/character/geometry.rs`.
- Dependencies: independent.

## Shooter and transport composition

### 229. Reusable acknowledged snapshot-baseline store

bounded net baseline store with exact sent-tick acknowledgement, byte/capacity limits, stable eviction, session reset and full/delta selection outcomes. Reuse `Delta`; separate sender and receiver responsibilities. Unknown ACKs never authorize a baseline; a newer tick alone is insufficient proof of retained bytes.

- Owner: `crates/ferravex-net/src/baselines.rs`.
- Dependencies: 313.

### 230. Transport-independent authority protocol driver

extract an authority protocol state with `receive(peer, bytes, now)` and explicit tick/poll operations producing bounded outgoing datagrams through caller storage/callbacks. Inject cookie generation at the host boundary. Retain `GameServer` as the std UDP adapter with current defaults and authority rules.

- Owner: `crates/ferravex-fps/src/network.rs`, `crates/ferravex-fps/src/network/replication.rs`, `crates/ferravex-fps/src/network/tactical.rs`, `crates/ferravex-fps/src/network/server_state.rs`.
- Dependencies: 229, 237, 311, 312, 316, 317.

### 231. Transport-independent prediction client driver

extracted client state accepting datagrams/supplied time, ticking local prediction, and yielding bounded outbound datagrams/events. Preserve `GameClient` wrappers and local-menu command suppression while authority polling continues. Make session reset clear baselines, pending predictions, event channels and probes together.

- Owner: `crates/ferravex-fps/src/network.rs`, `crates/ferravex-fps/src/network/replication.rs`, `crates/ferravex-fps/src/network/tactical.rs`, `crates/ferravex-fps/src/network/client_state.rs`, `crates/ferravex-fps/src/browser.rs`.
- Dependencies: 230, 315.

### 232. Bounded in-memory datagram link for playable local sessions

an in-memory pair/router with explicit peer IDs, byte/packet capacity, supplied-time delivery, FIFO equal-time order and optional deterministic delay/loss/reorder settings. It must run actual authority/client protocol state in a local game or network inspector, not just test assertions; never synthesize ACKs or game outcomes.

- Owner: `crates/ferravex-net/src/local_link.rs`, `crates/ferravex-fps/examples/local_session.rs`.
- Dependencies: 230, 231.

### 233. Checked reusable arena authoring

checked arena builder/validator with explicit solid/spawn caps, indexed nonfinite/negative-extents errors and optional character-volume spawn-clearance validation. Produce the same `Arena`, no new map format. Keep raw struct literals and legacy construction for compatibility.

- Owner: `crates/ferravex-fps/src/arena_authoring.rs`.
- Dependencies: independent.

### 234. Opt-in deterministic safe spawn selection

bounded selector over existing spawn points with static clearance, live-player distance and optional line-of-sight cost, deterministic index ties and explicit no-safe-candidate fallback. Integrate through an opt-in authority spawn policy, leaving current practice rules unchanged by default.

- Owner: `crates/ferravex-fps/src/spawn.rs`, `crates/ferravex-fps/src/lib.rs`.
- Dependencies: 233, 250.

### 235. Configurable bot navigation domain

validated `BotNavigationConfig` for origin, cell size, dimensions, clearance and explicit work caps, plus checked construction/rebuild against an arena. Existing `new` uses identical defaults. Reuse `Grid`/`PathPlanner`/`WorkBudget`; no new pathfinder.

- Owner: `crates/ferravex-fps/src/bots.rs`.
- Dependencies: 233, 264.

### 236. Roster-aware practice bot targets and scheduling

explicit controlled/local roster and hostility/target-selection policy; schedule live bot IDs independently of vector position, perceive bounded eligible targets and keep noise/squad source IDs intact. Preserve current practice defaults and accepted `InputFrame` submission.

- Owner: `crates/ferravex-fps/src/bots.rs`.
- Dependencies: 235, 261, 262.

### 237. Reusable authoritative step output

reusable `StepOutput`/`step_with_rules_into` that retains event/rejection buffers, validates output capacity before committing a tick or uses a documented bounded sink, and returns counts. Preserve event order and existing convenience APIs. Avoid truncating accepted combat events to fit a tiny buffer.

- Owner: `crates/ferravex-fps/src/lib.rs`, `crates/ferravex-fps/src/rewind.rs`.
- Dependencies: 234.

### 238. Presentation-only prediction correction smoothing

emit a bounded local reconciliation observation and maintain a presentation-only positional correction offset with checked decay/snap thresholds. Snap on life/session discontinuity or large correction; leave predicted collision, authoritative aim and shot origin untouched. Reuse smoothing/J400 202 where appropriate.

- Owner: `crates/ferravex-fps/src/presentation.rs`, `crates/ferravex-fps/src/network/replication.rs`.
- Dependencies: 202, 230, 231.

### 239. Accepted weapon-transition results for feedback

additive detailed local step result reporting accepted reload start/completion and jump/fire outcomes at the actual tick. Preserve the bool wrapper and core event enum. Use a sidecar or callback so hosts emit sounds/animation from accepted transitions instead of sampling changing fields or raw buttons.

- Owner: `crates/ferravex-fps/src/lib.rs`, `crates/ferravex-fps/src/combat.rs`.
- Dependencies: 234, 237.

### 240. Authority-owned player relocation with history invalidation

explicit Match relocation/respawn helper with finite/clearance validation, chosen motion reset policy and an epoch/discontinuity transition so rewind and remote interpolation cannot bridge a teleport. Preserve other players' valid history where possible and define same-tick capture behavior. Return the accepted destination for J400 202/presentation reset.

- Owner: `crates/ferravex-fps/src/lib.rs`, `crates/ferravex-fps/src/rewind.rs`.
- Dependencies: 233, 234, 237, 239.

## Reusable game rules

### 241. Expiring gameplay command outcomes

Add checked push/replace/expire/consume outcomes and remaining-life inspection to the existing one-command buffer. Let callers distinguish replaced, expired, gated and executed commands without consuming a read; retain edge-only semantics and invalid-delta transactionality.

- Owner: `crates/ferravex-play/src/gameplay/buffer.rs`.
- Dependencies: independent.

### 242. Combo scoring receipts

Expose accepted score breakdowns (base, multiplier, tier, awarded total and chain expiry) with checked overflow behavior and explicit reset modes. Preserve legacy awards; give Pinball/Orbit UI exact accepted-event data rather than recomputing scoring.

- Owner: `crates/ferravex-play/src/gameplay/combo.rs`.
- Dependencies: independent.

### 243. Progression award and restore reports

Add nonmutating award previews and checked progression snapshots/restoration tied to curve configuration. Report promotions, retained XP and saturation, including cap behavior; avoid re-awarding level rewards after restore.

- Owner: `crates/ferravex-play/src/gameplay/progression.rs`.
- Dependencies: independent.

### 244. Distinct weighted reward offers

Add bounded weighted sampling without replacement using the existing deterministic RNG, with clear zero-weight/exhaustion and caller-buffer outcomes. Preserve single-roll sequences and avoid bias from duplicate rejection loops.

- Owner: `crates/ferravex-play/src/gameplay/loot.rs`.
- Dependencies: independent.

### 245. Inspectable pity guarantees

Add an accepted-attempt outcome and a small transactional delivery method/policy: ineligible/deferred attempts do not advance misses, failed insertion does not consume the guarantee, successful insertion reports natural versus guaranteed. Keep RNG ownership/consumption explicit; do not silently rewind or change existing rolls.

- Owner: `crates/ferravex-play/src/gameplay/pity.rs`.
- Dependencies: independent.

### 246. Charge actions with explicit input edges

Add an edge-aware checked stepping API retaining held/pressed/released independently, including release-and-repress between host samples. Specify minimum commitment, cooldown remainder and at-most-once release outcomes; keep legacy held-only API behavior.

- Owner: `crates/ferravex-play/src/gameplay/charge.rs`.
- Dependencies: independent.

### 247. Shield recharge state and safe resizing

Expose delay/recharging/full/broken state and absorption/recharge receipts. Add checked maximum/rate/delay reconfiguration with explicit preserve-value or preserve-ratio policy, no accidental healing on failed edits, and existing time-partition behavior intact.

- Owner: `crates/ferravex-play/src/gameplay/shield.rs`.
- Dependencies: independent.

### 248. Stagger onset and recovery events

Add an opt-in recovery grace/resistance policy and transition receipt to the existing helper, keeping default zero grace. During grace, define whether poise is rejected or accumulated without triggering; choose one explicit, tested policy.

- Owner: `crates/ferravex-play/src/gameplay/stagger.rs`.
- Dependencies: independent.

### 249. Projectile contact identity and outcomes

Add compatible target/wall receipts identifying duplicate target, inactive projectile, accepted hit, exhausted pierce/bounce and separating contact, including remaining budgets. Do not create another collision query or change perfect-reflection defaults.

- Owner: `crates/ferravex-play/src/gameplay/projectile.rs`.
- Dependencies: independent.

### 250. Explainable safe spawn selection

Add checked candidate assessment and deterministic scored selection with exclusion reasons for distance, unavailable slots and invalid geometry. Keep caller-controlled obstruction queries and existing seeded tie behavior; report no valid spawn without fabricating one.

- Owner: `crates/ferravex-play/src/gameplay/spawn.rs`.
- Dependencies: independent.

### 251. Upgrade availability and atomic choices

A bounded `UpgradeOffer<K>`/choice-session adapter using existing `Upgrades` and `RewardRng`, with stable offer serial, earned-point balance and an accepted choice receipt. Failed/capped/stale/unoffered choices spend nothing; redraw does not reroll.

- Owner: `crates/ferravex-play/src/gameplay/upgrades.rs`.
- Dependencies: 244.

### 252. Segment and lap trial comparisons

Add checked named/indexed segments and comparison reports against a caller-supplied valid prior run. Distinguish segment, cumulative and lap deltas; reject reordered/duplicate splits and preserve existing Trial timing/medals and save compatibility. Also provide bounded reason-tagged time penalties deduplicated by accepted event serial for Dune/Sky; no additional clock ownership.

- Owner: `crates/ferravex-play/src/gameplay/trial.rs`.
- Dependencies: independent.

### 253. Pickup attraction and arrival reports

Add validated attraction configuration, explicit idle/attracting/arrived outcomes and reset/teleport handling. Prevent overshoot and repeated pickup acceptance, expose authored capture radii, and keep optional obstruction decisions with the host.

- Owner: `crates/ferravex-play/src/gameplay/pickup.rs`.
- Dependencies: independent.

### 254. Bounded multi-contact projectile traversal

A pure 2D traversal helper adjacent to `gameplay/projectile.rs` that consumes caller-provided nearest-hit queries, a displacement, a contact budget and task 249 policy. Return accepted contacts, residual motion and a work-cap outcome. Define tie ordering, initial overlap and separating-contact progress; reuse existing sweep implementations.

- Owner: `crates/ferravex-play/src/gameplay/projectile_traversal.rs`.
- Dependencies: 218, 249.

### 255. Platform phase transition reports

Expose launch acceptance and crumble phase transitions with bounded catch-up reports, exact reset behavior and rearm rules. Preserve authored defaults, and prevent held contact or large steps from producing duplicate launch/break feedback.

- Owner: `crates/ferravex-play/src/gameplay/platforms.rs`.
- Dependencies: independent.

### 256. Precision timing result breakdowns

Add signed/absolute timing errors, classification tiers and accepted-return summaries to PrecisionReturns, with checked threshold configuration and stable reset. Keep existing streak/progress rules and make near-miss guidance accurate.

- Owner: `crates/ferravex-play/src/gameplay/skill.rs`.
- Dependencies: independent.

### 257. Stable interaction target selection

Add a reusable bounded candidate selector that combines existing eligibility with distance/priority and explicit focus hysteresis. Report why focus changed or no target is eligible, preserve held interactions on stable targets, and cancel once when the target actually becomes invalid.

- Owner: `crates/ferravex-play/src/gameplay/interaction.rs`.
- Dependencies: independent.

### 258. Objective graph authoring diagnostics

Add indexed dependency/cycle/branch diagnostics and checked graph construction. Reproduce and resolve skipped-gate settlement ambiguities with explicit rules while preserving valid authored objective sequences, snapshots and event serial deduplication.

- Owner: `crates/ferravex-play/src/objective.rs`.
- Dependencies: independent.

### 259. Atomic encounter group admission

Add a checked group-admission operation for several simultaneous spawns, previewing aggregate credit, alive slots and pacing before any mutation. Reject the whole group on capacity failure; preserve individual admission and stable countdown semantics.

- Owner: `crates/ferravex-play/src/encounter.rs`.
- Dependencies: independent.

### 260. Navigation request replacement and fairness

Extend the existing PathPlanner with explicit replace/cancel/terminal outcomes and bounded fair scheduling across queued searches. Preserve pending results until acknowledged and tie results to the requested graph revision; prevent stale paths after level edits.

- Owner: `crates/ferravex-play/src/navigation.rs`.
- Dependencies: 264.

### 261. Priority-aware AI work scheduling

Extend RoundRobin with an opt-in bounded priority/fairness policy and per-pass granted/deferred counts. Ensure low-priority agents receive service under persistent load, handle population changes and reset deterministically without changing the current simple scheduler.

- Owner: `crates/ferravex-play/src/ai/scheduler.rs`.
- Dependencies: independent.

### 262. Target selection policies for game rules

Add checked nearest/highest-priority/progress-score candidate policies to the existing TargetSelector with stable ties and hysteresis. Preserve visibility masks, expiry and query budgets; make Orchard targeting policy use the same accepted candidate evidence.

- Owner: `crates/ferravex-play/src/ai/perception.rs`.
- Dependencies: independent.

### 263. Transactional squad cover handoff

Support checked transfer/replacement of a cover reservation and per-agent cancellation without briefly double-owning a slot. Preserve leases/expiry and deterministic selection; return clear stale-owner, occupied and missing-slot outcomes.

- Owner: `crates/ferravex-play/src/ai/squad.rs`.
- Dependencies: independent.

### 264. Revisioned navigation traversal overrides

Add checked per-link enabled/cost overrides with transactional batch application and a monotonic revision visible to planners. Preserve authored geometry and legacy costs; reject invalid indices/negative costs and document how in-flight searches detect changed traversal.

- Owner: `crates/ferravex-play/src/navigation/mesh.rs`.
- Dependencies: independent.

### 265. Safe live ability tuning

Add checked reconfiguration for cooldown/recharge/cost parameters with an explicit policy for active cooldowns and existing charges/reservations. Reject incompatible edits atomically; share validation with existing availability and begin/activate acceptance paths.

- Owner: `crates/ferravex-play/src/ability.rs`.
- Dependencies: independent.

### 266. Optional target-assist geometry

Provide pure checked cone/distance target-assist selection and angular correction limits using caller-supplied candidate visibility. Default to disabled at consumers, preserve input authority, and expose accepted target/angle evidence. Never alter authoritative hit validation.

- Owner: `crates/ferravex-play/src/gameplay/aim.rs`.
- Dependencies: independent.

### 267. Composable timed training challenges

Build a bounded challenge-rule helper around accepted events, ordered stages, tick deadlines and result summaries, independent of host rendering/lifecycle. Support reset and practice mode without duplicating GameSession; provide a small runnable drill composition.

- Owner: `crates/ferravex-play/src/gameplay/challenge.rs`.
- Dependencies: independent.

### 268. Deterministic trigger-zone membership

Add bounded checked point/shape trigger membership with enter/stay/exit outcomes, stable identities and optional boundary hysteresis. Support teleport/reset and removed targets without duplicate exits; leave physics contact ownership untouched.

- Owner: `crates/ferravex-play/src/gameplay/zones.rs`.
- Dependencies: independent.

### 269. Explicit record eligibility policies

Add attempt metadata/policy helpers explaining why a practice, assisted, restarted or modified-rules attempt may be recorded. Keep existing serialized records/acceptance unchanged unless a caller explicitly supplies policy; return structured disqualification reasons.

- Owner: `crates/ferravex-play/src/persistence/records.rs`.
- Dependencies: independent.

### 270. Shared deterministic gameplay scenario driver

Create a headless fixed-tick scenario utility with ordered input edges, pause/resume/reset commands, render-rate partitions and bounded trace capture. Drive actual game adapters, distinguish issued versus executed terminal ticks, and support assertions without wall clocks or fabricated wins.

- Owner: `crates/ferravex-play/src/scenario.rs`.
- Dependencies: 201, 241, 246.

## Readable presentation

### 271. Reserve screen-space HUD regions around a fitted world

Add a pure `HudRegions`/edge-reservation helper under `crates/ferravex-play/src/ui/`, returning named rectangles, remaining content and explicit insufficient-space results. Add a small renderer adapter for fitting an existing `Canvas2D` into the returned content rectangle. Support four edges and a fixed reservation count, not a general constraint solver.

- Owner: `crates/ferravex-play/src/ui/layout.rs`, `crates/ferravex-play/src/ui.rs`.
- Dependencies: independent.

### 272. Content-sized rows and wrapping action groups

A caller-buffer `flow` helper under `crates/ferravex-play/src/ui/` taking each item's minimum/preferred/maximum extent, gap and optional grow weight; deterministic wrap order and an explicit overflow count. Limit items and avoid a retained layout tree.

- Owner: `crates/ferravex-play/src/ui/flow.rs`.
- Dependencies: independent.

### 273. Reusable wrapped-text layout with measured block height

Extract the existing line-breaking algorithm into a borrowed layout iterator in `ui/shared.rs` or a dedicated renderer text-layout module. Return line spans, required height and truncation/hidden-line counts; offer an explicit maximum-line policy. Reuse existing bitmap metrics and unsupported-glyph behavior.

- Owner: `crates/ferravex-render/src/ui/shared.rs`, `crates/ferravex-render/src/ui/text_layout.rs`.
- Dependencies: independent.

### 274. Bounded scroll regions with focus reveal

Pure `ScrollState` under `crates/ferravex-play/src/ui/`, plus renderer wheel/drag/page-navigation adapter using existing capture and clips. Support one vertical axis, clamped content extent and focus reveal; nested scroll routing can remain out of scope.

- Owner: `crates/ferravex-play/src/ui/scroll.rs`, `crates/ferravex-render/src/ui/scroll.rs`.
- Dependencies: 271.

### 275. Consistent meters and segmented progress widgets

Renderer `meter` and `segments` compositions over supplied values/labels, fixed bounds and explicit empty/unknown states. Put reusable geometry under play UI and painting under renderer UI. Support threshold markers and text/pattern cues; do not compute gameplay availability or add smoothing to authoritative values.

- Owner: `crates/ferravex-render/src/ui/meters.rs`.
- Dependencies: 281.

### 276. Responsive label/value and result-summary lists

A `stat_rows` composition over borrowed labels and formatted values with a capped row count, shared label width, stacked fallback and explicit overflow. Use J400 273 for variable-height descriptions; retain `Ui::text_fmt` storage reuse.

- Owner: `crates/ferravex-render/src/ui/stats.rs`.
- Dependencies: 273, 281.

### 277. Content-driven menu and results cards

A renderer card composition accepting borrowed sections and stable action IDs, with measured title/body/action layout, fixed padding, optional J400 274 scrolling and a required primary-action region. Reuse `Ui::modal`; no new lifecycle or modal manager.

- Owner: `crates/ferravex-render/src/ui/cards.rs`.
- Dependencies: 271, 272, 273, 274, 276, 281.

### 278. One geometry description for visible HUD touch controls

Renderer geometry adapters from `UiRect`/`Viewport` and `Canvas2D` to existing `touch::Control`, retaining explicit slot/channel identity. Supply draw/hit geometry from the same result; preserve custom touch overrides and their stored indexes.

- Owner: `crates/ferravex-render/src/input/touch_layout.rs`.
- Dependencies: 271, 286.

### 279. Explicit UI-versus-world pointer ownership result

Return or expose a bounded `UiInteraction` result identifying consumed pointer activation/capture and keyboard confirmation, with a documented order for sampling UI then translating remaining scene actions. Additive renderer API; do not consume all gameplay edges or replace `ActionMap` contexts.

- Owner: `crates/ferravex-render/src/ui/shared.rs`.
- Dependencies: 273.

### 280. Read-only UI semantic and geometry inspection

Opt-in borrowed/caller-buffer inspection of submitted widget ID, role, full label, enabled/focused state, logical bounds and visible clipped bounds in renderer UI. Add a bounded debug exporter adapter only where a host opts in. This is UI state inspection, not another scene-schema export or an action-execution backdoor.

- Owner: `crates/ferravex-render/src/ui/shared.rs`.
- Dependencies: 273, 279.

### 281. Atomic multi-widget composition

A bounded composition transaction/checkpoint in `ui/shared.rs`, restoring commands, text, estimated vertices, clip depth and new widget registrations on failure. Preserve existing single-command semantics and avoid cloning the whole UI arena.

- Owner: `crates/ferravex-render/src/ui/shared.rs`.
- Dependencies: 273, 279, 280.

### 282. Bounded accepted-event notices with priority and coalescing

Renderer-independent `NoticeQueue<N>` in play UI, with caller-owned message payloads, explicit priority/key/coalescing policy, bounded lifetime and dismissal. The host chooses presentation or simulation delta; no hidden wall clock and no accepted-action inference from health/score differences.

- Owner: `crates/ferravex-play/src/ui/notices.rs`.
- Dependencies: independent.

### 283. Checked minimap and waypoint projection

Provide a bounded world-to-map transform with explicit map bounds, orientation, clamp/off-map indicators and inverse picking. Reuse existing Rect/Vec geometry; do not discover hidden objectives automatically. Crystal's discovered-room journal is the first consumer.

- Owner: `crates/ferravex-play/src/ui/minimap.rs`.
- Dependencies: 218.

### 284. Atomic retained instance submission

Add preflighted caller-slice submission of multiple instances of resident meshes with explicit accepted/rejected counts and unchanged buffers on group validation/capacity failure. Reuse existing RetainedBatch batching, mesh validation and telemetry; preserve ABI and warmed allocation behavior.

- Owner: `crates/ferravex-render/src/retained.rs`.
- Dependencies: 216.

### 285. Checked numeric inspector widgets

Compose bounded steppers, toggles and choice controls for numeric authoring with field labels, units, validation messages and accepted value-change results. Reuse focus, capture, clips and button states. Invalid/disabled edits must not mutate supplied model state; use both physics lab inspectors.

- Owner: `crates/ferravex-render/src/ui/inspector.rs`.
- Dependencies: 277, 279, 290.

### 286. Checked canvas fitting into reserved rectangles

Add nonpanicking Canvas2D construction and fitting into an explicit content rectangle with checked world/screen units and inset offsets. Preserve current fit signature and world framing; return inert/error outcomes for invalid dimensions. Project/unproject/touch mapping must round-trip under DPR and reserved HUD regions.

- Owner: `crates/ferravex-render/src/canvas.rs`.
- Dependencies: 271.

### 287. Explicit camera blends and cuts

Add checked presentation-only transitions between camera frames, with continuous retargeting and explicit cut/reset/hold semantics. Interpolate rotations by shortest arc, keep projection compatibility explicit, and leave collision queries/authoritative poses to the host. Demonstrate pause and teleport without stale interpolation.

- Owner: `crates/ferravex-play/src/camera/transition.rs`.
- Dependencies: 202, 217.

### 288. Authored camera rail sampling

Compose bounded curve/route position sampling with authored orientation and durations for optional start/result camera rails. Expose cut/loop/finished outcomes and caller-time sampling; preserve look tuning and reduced-motion instant-cut policy. Reuse curves and camera transitions rather than add a spline engine.

- Owner: `crates/ferravex-play/src/camera/rail.rs`.
- Dependencies: 219, 287.

### 289. Validated skeleton pose remapping

Add a checked name/hierarchy mapping between compatible skeletons with explicit missing/duplicate joint diagnostics, rest-pose offsets and caller-buffer output. Preserve existing animation/skin formats; reject unsupported nonuniform/shear cases explicitly. Provide a real two-skeleton pose recipe and no hot-path name lookup.

- Owner: `crates/ferravex-play/src/animation/retarget.rs`.
- Dependencies: 216, 217.

### 290. Shared UI type and contrast themes

Provide validated logical-pixel type scales, spacing and semantic color roles for normal/high-contrast presentation, preserving authored game palettes through explicit overrides. Include contrast measurements and non-color state cues; do not silently shrink essential text to fit. Supply theme tokens to cards/meters/inspectors.

- Owner: `crates/ferravex-play/src/ui/theme.rs`.
- Dependencies: independent.

## Input, audio, content and persistence

### 291. Atomic typed action snapshots

Add caller-array snapshots/consumption for ActionHandle groups, validating every handle before consuming any edge. Define duplicate-handle semantics consistently with PlayerInput::take_tick, preserve contexts and held suppression, and avoid allocation on valid tick reads.

- Owner: `crates/ferravex-play/src/input/mod.rs`.
- Dependencies: independent.

### 292. Visible-control-aware binding hints

Let callers supply actual visible touch-control labels/slots and receive faithful remapping-aware hints. Distinguish absent controls, custom layouts, scaled/negative bindings and gestures; preserve existing format_hint and caller-buffer errors. Extract the useful Pocket Rally convention without claiming hidden Touch 14 is a visible button.

- Owner: `crates/ferravex-render/src/input/player/editor.rs`.
- Dependencies: 278.

### 293. Checked gamepad calibration profiles

Add opt-in validated per-axis center/range and radial-stick deadzone calibration with explicit reset/defaults, finite clamping and disconnected-device behavior. Preserve current mapping defaults and button edges; expose profile inspection for tools and test asymmetric/drifting axes.

- Owner: `crates/ferravex-render/src/input/gamepad.rs`.
- Dependencies: independent.

### 294. Configurable touch gesture recognition

Expose validated logical-distance/time thresholds for tap, drag and swipe recognition using existing capture state. Preserve current defaults and channels, cancel old captures on profile changes, and test resize/focus loss/multi-pointer edge delivery without inventing new gesture ABI.

- Owner: `crates/ferravex-play/src/input/touch.rs`.
- Dependencies: independent.

### 295. Live in-game text and contrast preferences

Extend the existing presentation-preference bridge with versioned text-scale/contrast values consumed by the new UI theme. Preserve motion/flash independence and existing imports/exports; applying preferences must reflow layout and touch geometry without changing world scale or gameplay.

- Owner: `crates/ferravex-render/src/preferences.rs`, `web/player.js`.
- Dependencies: 290.

### 296. Bounded semantic game-status bridge

Define an opt-in bounded read-only semantic snapshot for lifecycle, objective, result and available action labels, with stable IDs/change serials and explicit truncation. Supply a browser adapter rendering text/live-region updates only for meaningful changes, plus cleanup. Reuse UI inspection; never add an arbitrary game-action execution backdoor.

- Owner: `crates/ferravex-render/src/status.rs`, `web/status.js`, `web/tests/status.mjs`.
- Dependencies: 280, 282.

### 297. Accepted audio cue batching

A fixed-capacity cue accumulator in `crates/ferravex-render/src/audio/`, with explicit category coalescing, retained critical cues and admitted/coalesced/dropped counts. Flush through `GameAudio`; pause/reset discards the batch. Do not replace voice priorities, audio-ring recovery or event deduplication.

- Owner: `crates/ferravex-render/src/audio/batch.rs`.
- Dependencies: independent.

### 298. Allocation-free mixer level metering

Add opt-in bounded per-block peak, RMS, clipped/nonfinite sample counters and resettable summaries measured from actual mixed output. Preserve voice admission/fades and exact silence; no log/string allocation in render. Clearly distinguish meter measurements from physical audibility.

- Owner: `crates/ferravex-play/src/audio/mixer.rs`.
- Dependencies: independent.

### 299. Actionable WAV decode diagnostics

Add checked Clip WAV decoding with typed chunk/offset/format/size diagnostics while preserving Clip::wav compatibility and accepted byte formats. Reuse the existing decoder, reject malformed padding/duplicate chunks transactionally, and include a small asset-loading example.

- Owner: `crates/ferravex-play/src/audio/mod.rs`.
- Dependencies: independent.

### 300. Optional gameplay sound captions

An optional bounded caption adapter consuming accepted gameplay event categories with priority, expiry and optional coarse direction, implemented on task 282. Captions follow the accepted event even if sound is muted or a voice is denied; they must not infer hidden game information from arbitrary audio positions.

- Owner: `crates/ferravex-play/src/ui/captions.rs`.
- Dependencies: 282.

### 301. Transactional named input profiles

Add a bounded opt-in profile set around existing validated FXIN configs, with preview/activate/rename/delete and explicit active-profile identity. Activation must respect conflict checks and held-input suppression; retain single-config imports and version any outer profile envelope.

- Owner: `crates/ferravex-play/src/input/config.rs`, `crates/ferravex-render/src/input/storage.rs`.
- Dependencies: 291.

### 302. Native save-slot inventory and inspection

Provide read-only bounded slot enumeration and envelope metadata inspection with explicit valid/corrupt/backup-present outcomes. Do not deserialize arbitrary game payloads, recover automatically or mutate files. Preserve owner/slot validation and atomic last-good backups.

- Owner: `crates/ferravex-render/src/storage/native.rs`.
- Dependencies: independent.

### 303. Explicit transactional save migrations

Add a bounded opt-in version migration chain for existing Document/SaveData boundaries, preserving original bytes until every migration and final validation succeeds. Report missing steps/unsupported versions and define callback side-effect limits; provide a real two-version fixture without changing current saves by default.

- Owner: `crates/ferravex-play/src/persistence/migrations.rs`.
- Dependencies: independent.

### 304. Source-preserving scene transform edits

An authoring helper limited initially to transforms of explicitly authored top-level node IDs: locate token spans, check expected old value/source fingerprint, apply a bounded non-overlapping patch batch, then reparse/validate before returning text. Preserve comments and unrelated bytes. Explicitly reject prefab-generated paths in this first version.

- Owner: `crates/ferravex-play/src/authoring/patch.rs`, `crates/ferravex-play/src/authoring/parser.rs`.
- Dependencies: independent.

### 305. Schema-aware authoring completions

Add deterministic bounded completions for existing schema component/field names and enumerated choices, retaining exact constraints and documentation. Accept explicit source context/prefix, distinguish unknown context, and return replacement spans only when validated; build on schema inspection rather than infer a schema from samples.

- Owner: `crates/ferravex-play/src/authoring/schema.rs`.
- Dependencies: independent.

### 306. Revision-guarded content submissions

Add a `queue_if_current(document, generation, source)` operation with explicit stale-document/stale-generation/pending outcomes. Preserve legacy `queue` and its ABI. If exported to Wasm, add coordinated versioned/additive exports and host tests rather than reinterpret existing status values.

- Owner: `crates/ferravex-render/src/content.rs`.
- Dependencies: independent.

### 307. Replay import preflight without materialization

Add a bounded inspection pass over serialized recordings reporting validated header, ticks/checkpoints/events, payload budgets and exact failure offsets before allocating full playback state. Reuse current wire validation and limits; preflight must agree with decode and must not claim simulation checksum verification.

- Owner: `crates/ferravex-play/src/replay/wire.rs`.
- Dependencies: independent.

### 308. Replay bookmarks and annotation sidecars

Add bounded named tick/range bookmarks in a separately versioned sidecar tied to recording fingerprint/rules/start/end. Support checked insert/edit/remove and next/previous marker lookup; reject stale sidecars without changing the replay. Preserve original recording bytes and existing budgeted seek behavior.

- Owner: `crates/ferravex-play/src/replay/bookmarks.rs`.
- Dependencies: 307.

### 309. Discontinuity-preserving ghost compaction

Add explicit offline bounded ghost sample reduction with caller-selected position/rotation error tolerances, preserving endpoints and teleport markers. Report retained/removed counts and error bounds; do not alter default recording or sample_checked behavior.

- Owner: `crates/ferravex-play/src/replay/ghost.rs`.
- Dependencies: independent.

### 310. Opt-in bounded runtime bug traces

Record an explicit opt-in ring of bounded frame/lifecycle/performance and semantic action events for export alongside existing diagnostics. Exclude arbitrary text input, keys outside the game, URLs/secrets and account data; record dropped counts and clear/dispose behavior. Keep diagnostics exports usable when tracing is off.

- Owner: `web/diagnostic-trace.js`, `web/tests/diagnostic-trace.mjs`.
- Dependencies: 296.

## Network ergonomics and diagnostics

### 311. Borrowed reliable-send batches

Add a bounded caller-driven reliable-send path exposing borrowed payloads and packet/byte budgets without cloning every EventPacket. Mark a packet sent only after the caller accepts it; preserve retry cadence, individual ACKs, sequence exhaustion and legacy due behavior. Prove early exit does not hide pending work.

- Owner: `crates/ferravex-net/src/reliable.rs`.
- Dependencies: independent.

### 312. Budgeted reliable receive delivery

Separate bounded receipt/acknowledgment from ordered caller-buffer delivery so backpressure never drops an undelivered event. Report duplicate/outside-window/capacity outcomes, preserve once-only order and retain queued tails. Test lost first packets, full destinations, duplicate payloads and sequence boundaries.

- Owner: `crates/ferravex-net/src/reliable.rs`.
- Dependencies: 311.

### 313. Reusable delta encode and apply buffers

Add checked size planning and caller-buffer delta construction/application, with exact failure diagnostics and unchanged destination on rejected lengths/base ticks. Preserve current wire data and quantization; verify corrupted ranges and warmed storage reuse.

- Owner: `crates/ferravex-net/src/delta.rs`.
- Dependencies: independent.

### 314. Precise network-history interpolation

Add integer-tick plus fraction history bracketing with explicit empty/exact/clamped/stale outcomes, avoiding f64 loss of adjacent large ticks. Preserve existing History/ClockSync behavior, bounded ring eviction and monotonic insertion; include reconnect history-reset guidance.

- Owner: `crates/ferravex-net/src/history.rs`.
- Dependencies: independent.

### 315. Explainable clock synchronization samples

Expose typed reasons for ignored clock samples and inspect pending probes, RTT/offset freshness and selected render delay. Add explicit safe rebase/reset after reconnect while preserving existing estimators and tick unwrapping. Reject malformed/expired probes without partially changing estimates.

- Owner: `crates/ferravex-net/src/clock.rs`.
- Dependencies: independent.

### 316. Observable network input queue admission

Add checked input-queue admission reports and saturating rejection/eviction counters for duplicate, out-of-order, full and invalid input cases. Preserve authoritative ordering and wire protocol; make reset and metric scopes explicit. Do not reinterpret local menus as network suspension.

- Owner: `crates/ferravex-net/src/lib.rs`.
- Dependencies: independent.

### 317. Caller-buffer packet encoding

Provide encoded-length and checked encode-into paths for the existing client/server protocol, with field-specific validation and clear capacity failures. Preserve all v5 bytes, decoder compatibility and 1200-byte bounds; reject invalid inputs before writing where promised.

- Owner: `crates/ferravex-net/src/lib.rs`.
- Dependencies: 316.

### 318. Reusable client events and interpolation

Add bounded event draining and interpolated-player output into caller-provided reusable storage. Preserve old convenience methods, pending event order, correction semantics and local-player authority. Do not consume events that do not fit.

- Owner: `crates/ferravex-fps/src/network.rs`.
- Dependencies: 230, 231, 312, 314.

### 319. Network health and correction snapshots

Expose read-only connection/reconciliation telemetry: confirmed state, snapshot age, queued/acknowledged input, reliable backlog and correction magnitude/count, with explicit unknown values. Scope counters to connection epochs and keep transport/authority behavior unchanged.

- Owner: `crates/ferravex-fps/src/network.rs`.
- Dependencies: 230, 231, 315, 318.

### 320. Replayable adverse-network scenario manifests

Use the actual transport-independent authority/client states and bounded local link from task 232 to run checked seeded scenarios for delay, loss, duplicate/reorder, reconnect and buffer saturation. Export bounded reproducible traces and expected invariants for agent debugging; preserve packet fixtures and never synthesize acknowledgments or accepted hits. No second simulated transport implementation.

- Owner: `crates/ferravex-fps/examples/network_scenarios.rs`, `crates/ferravex-fps/tests/journey400_network.rs`.
- Dependencies: 232, 318, 319.

## Developer and coding-agent workflow

### 321. Local API symbol search for coding agents

Build a bounded, deterministic JSON symbol search from first-party generated Rustdoc/site index evidence; expose actual import paths, kind, signature excerpt, source/doc locations, features and provenance. Refuse stale or absent indexes explicitly, and never invent an API from a filename. Include exact and fuzzy-name lookup without external packages.

- Owner: `scripts/api-index.py`, `scripts/test_api_index.py`.
- Dependencies: independent.

### 322. Self-describing developer CLI

Add a read-only `describe` command returning command/argument descriptors and execution category from the same parser/explicit helper descriptors. Bound the description and version it; forwarded flags that the wrapper does not understand must remain marked opaque.

- Owner: `scripts/ferravex.py`, `scripts/test_ferravex_cli.py`.
- Dependencies: 321, 323, 324.

### 323. Executable task-oriented recipe catalog

Create a versioned catalog of at least 20 common engine tasks with correct APIs, package/features, runnable examples and expected outcomes. Cover start/pause, tick input, UI, physics queries, assets, save/replay, audio and debugging. Validate referenced examples/imports; reuse guide checking rather than invent a second compiler runner.

- Owner: `docs/recipes.json`, `scripts/check-recipes.py`, `scripts/test_check_recipes.py`, `crates/ferravex/examples/journey400_recipes.rs`.
- Dependencies: 201, 203, 204, 206, 213, 221, 271, 277, 291, 296, 297, 302.

### 324. Actionable engine error reference

Map real stable diagnostic codes and qualified error variants to concise cause, recovery, API links and validated recipe references. Provide versioned machine JSON and human CLI lookup, unambiguous unknown-code results and source-presence checks. Avoid asserting that every Display string is a stable code.

- Owner: `docs/diagnostic-help.json`, `scripts/explain-error.py`, `scripts/test_explain_error.py`.
- Dependencies: 323.

### 325. Ownership-aware narrow validation plans

Add a separate advisory owned-scope planning mode/tool taking explicit owned paths, package and optional test filter. Emit only candidate package-local/source checks plus a distinct parent-integration requirement list; include refused broad/staging commands. Do not weaken or relabel the existing authoritative full/changed modes.

- Owner: `scripts/plan-owned-checks.py`, `scripts/test_plan_owned_checks.py`.
- Dependencies: independent.

### 326. Content-verified validation resume

Add explicit resume-from-report support. Reuse a passing check only when its declared transitive input, command, toolchain, environment overrides and prerequisite artifact hashes match. Never promote advisory/changed checks to full verification; report reused versus executed evidence and invalidate deleted/changed inputs.

- Owner: `scripts/validate.py`, `scripts/test_validate.py`.
- Dependencies: independent.

### 327. Precise CLI features, targets and doctests

Add mutually validated `--features`, `--no-default-features`, `--all-features`, `--target`, and test `--doc` selection to the wrapper. Keep existing defaults for compatibility or expose an explicit documented narrow mode; distinguish wrapper flags from arguments after `--`.

- Owner: `scripts/ferravex.py`, `scripts/test_ferravex_cli.py`.
- Dependencies: 322.

### 328. Performance artifact provenance

Replace guessed target/profile executable selection with matching Cargo compiler-artifact evidence, including explicit target dirs and target triples. Reject stale or ambiguous executable evidence. Record the exact measured binary hash and preserve existing raw samples, budgets and comparison contracts.

- Owner: `scripts/perf-games.py`, `scripts/test_perf.py`.
- Dependencies: independent.

### 329. Reproducible benchmark scenario manifests

Add checked scenario manifests recording seed, dimensions, quality, warm-up, frames and repetitions alongside toolchain/artifact identity. Reject incompatible baselines with field-specific reasons. Provide repeatable CPU/GPU/allocation scenarios without silently widening budgets or dropping samples.

- Owner: `scripts/perf-games.py`, `scripts/test_perf.py`.
- Dependencies: 328.

### 330. Fresh browser artifact verification command

Verify the existing web manifest against actual staged bytes and current scoped source provenance without rebuilding. Return exact missing/changed/unexpected evidence and per-game freshness; distinguish source-only differences from stale Wasm. Support explicit staging roots and versioned JSON for agents.

- Owner: `scripts/verify-web.py`, `scripts/test_verify_web.py`.
- Dependencies: independent.

### 331. Scene authoring lint command

Provide a runnable first-party scene lint example/CLI for actual Document parsing, prefab expansion and supplied built-in schema validation, with structured diagnostic locations and meaningful exit codes. Keep syntax-only results distinct from schema validation and cooking; bound input size.

- Owner: `crates/ferravex-play/examples/scene_lint.rs`, `docs/SCENE_LINT.md`.
- Dependencies: 304, 305.

### 332. Scaffold input presets

Extend generation with explicit checked keyboard/controller/touch input presets using the existing action APIs. Record chosen presets in generation metadata, escape names safely and preserve no-overwrite/standalone behavior. Generated templates must show live hints and tick-only edge consumption.

- Owner: `scripts/new-game.py`, `scripts/test_new_game.py`, `templates/game-2d/`, `templates/game-3d/`.
- Dependencies: 291, 292.

### 333. Scaffold optional save and replay examples

Add explicit opt-in first-party save and replay starter modules with a tiny deterministic game-state boundary, versioned owned save data and round-trip examples. Keep the minimal default template small and avoid mandatory new runtime dependencies. Document which host modes support each option.

- Owner: `scripts/new-game.py`, `scripts/test_new_game.py`, `templates/game-2d/`, `templates/game-3d/`.
- Dependencies: 307, 308, 332.

### 334. Typed Wasm host-contract audit

Extend the existing bounded Wasm section reader into a reusable signature audit (type/import/function/export sections) and compare against explicit per-host contract data. Reuse existing readers rather than add an unrelated parser. No instantiation, build or staging is necessary to audit a supplied module.

- Owner: `scripts/audit-wasm.py`, `scripts/test_audit_wasm.py`.
- Dependencies: 296.

### 335. Headless game-domain content validation

Add a narrow check-content entry point/example to each of Marble and Orchard that calls its actual `Level::parse`, emits structured diagnostics, and exits without a window, session restart or staging. A Python wrapper may select these existing package entry points; do not reimplement their schemas in Python.

- Owner: `scripts/check-game-content.py`, `scripts/test_check_game_content.py`, `examples/marble-run/src/`, `examples/orchard-guard/src/`.
- Dependencies: 304, 331; journey200-190-accepted, journey200-191-accepted.

### 336. Freshness-aware adoption evidence dashboard

A read-only stdlib evidence joiner consuming existing build/smoke/validation reports and explicit visual observations, outputting per-feature/per-game states: implemented, source-checked, built-current, visually-observed-current, blocked/unknown. Use hashes and recorded configurations; never use an image filename or receipt's “pass” prose as proof.

- Owner: `scripts/report-validation.py`, `scripts/test_report_validation.py`.
- Dependencies: 326, 329, 330.

### 337. Downstream API compatibility examples

Add real consumer examples for preserved Journey50/200 contracts and new recommended composition paths, including feature-minimal/native/Wasm import coverage. Catch private-module reexport and facade drift; document intentionally retained behavior rather than asserting unverified semantic-version guarantees.

- Owner: `crates/ferravex/tests/journey400_compatibility.rs`, `docs/JOURNEY_400_COMPATIBILITY.md`.
- Dependencies: 201, 220, 271, 291, 296.

### 338. Generated-project health inspection

Read a generated project's ferravex.json/manifests and report moved engine paths, unsupported generation schema, missing entry points, feature/input preset inconsistencies and regeneration options. Read-only by default; never overwrite project customization or infer freshness from timestamps.

- Owner: `scripts/doctor-project.py`, `scripts/test_doctor_project.py`.
- Dependencies: 332, 333.

### 339. Live content and shader workflow recipes

Provide executable recipes for schema discovery, validated edits, last-good reload, shader failure navigation and authored scene replacement using the new APIs. Demonstrate error recovery and teardown with a headless mode; register links/snippets through parent integration.

- Owner: `docs/JOURNEY_400_AUTHORING.md`, `crates/ferravex-render/examples/journey400_authoring.rs`.
- Dependencies: 304, 305, 306.

### 340. Journey400 migration and agent navigation

Write before/after recipes for the final new APIs and update the agent API/task map with actual signatures, bounded ownership, validation/provenance tools and all fifteen demo feature migrations. Compile and execute complete snippets; preserve Journey50/200 guidance as history. Parent owns root AGENTS/site registration.

- Owner: `docs/MIGRATION_400.md`, `docs/AGENT_GUIDE.md`, `CONTRIBUTING.md`.
- Dependencies: 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255, 256, 257, 258, 259, 260, 261, 262, 263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283, 284, 285, 286, 287, 288, 289, 290, 291, 292, 293, 294, 295, 296, 297, 298, 299, 300, 301, 302, 303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 317, 318, 319, 320, 321, 322, 323, 324, 325, 326, 327, 328, 329, 330, 331, 332, 333, 334, 335, 336, 337, 338, 339, 341, 342, 343, 344, 345, 346, 347, 348, 349, 350, 351, 352, 353, 354, 355, 356, 357, 358, 359, 360, 361, 362, 363, 364, 365, 366, 367, 368, 369, 370, 371, 372, 373, 374, 375, 376, 377, 378, 379, 380, 381, 382, 383, 384, 385.

## Fifteen new playable features

### 341. Emberline: optional lantern-spark relay

Add one optional spark on an existing high side route, carried to the second lantern within an authored simulation-time window for a once-only bonus. Falls/expiry return only that challenge to its start; normal embers, checkpoint recovery and the main finish remain unchanged. Own Emberline source/README/tests, with a small private relay module if useful.

- Owner: `examples/emberline/src/`, `examples/emberline/README.md`.
- Dependencies: 201, 202, 241, 255, 268, 282; journey200-186-accepted.

### 342. Neon Swarm: one bounded upgrade choice between waves

Add a separate selectable run variant with at most one choice at each of the two wave breaks: a damage, projectile or shield branch with explicit rank caps. Keep normal run balance/records intact; use a distinct rules identity for the variant. Own Neon simulation/run/screen/README/tests after the portrait supplement finishes.

- Owner: `examples/neon-swarm/src/`, `examples/neon-swarm/README.md`.
- Dependencies: 206, 209, 213, 241, 242, 243, 245, 247, 248, 249, 251, 254, 265, 277; journey200-187-accepted.

### 343. Pocket Rally: scored drift sectors

Add an optional challenge over three authored bends: score controlled lateral slip only above a speed threshold, on road, facing forward and making gate/route progress. Cap reward per sector/lap so circles cannot farm points. Keep the normal three-lap timing/ghost path unchanged and records distinct if persisted.

- Owner: `examples/pocket-rally/src/`, `examples/pocket-rally/README.md`.
- Dependencies: 202, 218, 242, 252, 268; journey200-188-accepted.

### 344. Salvage Pinball: nudge with a tilt consequence

Add one remappable nudge action applying a small bounded physical impulse to the active ball. Accumulate a simulation-time tilt budget; overuse disables flippers for the remainder of that ball and shows a persistent tilt explanation. Saves/lane returns must have an explicit same-life tilt policy. No multiball or new table is required.

- Owner: `examples/salvage-pinball/src/`, `examples/salvage-pinball/README.md`.
- Dependencies: 221, 226, 241, 246, 265, 275, 297; journey200-189-accepted.

### 345. Orchard Guard: player-selectable tower targeting priority

Per-tower First/Strongest/Nearest policies with deterministic ID tie breaks and a selected-tower control. Default remains First; in-flight projectiles retain their accepted target ID. Own `src/game.rs`, presentation/host UI, save adapter, README and tests.

- Owner: `examples/orchard-guard/src/`, `examples/orchard-guard/README.md`.
- Dependencies: 209, 262, 272, 279, 303, 335; journey200-190-accepted.

### 346. Marble Run: isolated checkpoint practice

A Practice entry for the current course allowing selection of an existing checkpoint and a run to the next checkpoint/finish. Define a fresh countdown, deterministic hazard phase, bounded local attempts and an explicit practice label. Practice must never submit a medal/best-time or overwrite campaign checkpoint/split references.

- Owner: `examples/marble-run/src/`, `examples/marble-run/README.md`.
- Dependencies: 202, 221, 252, 269, 274, 277, 335; journey200-191-accepted.

### 347. Dune Skimmer: a second technical circuit

Extract a bounded course definition and add one short technical circuit with tighter but traversable bends and different boost opportunities. Select at Ready/finished state; preserve the original circuit as default. Own Dune course/race/session/presentation/README/tests; no procedural generator or opponent AI.

- Owner: `examples/dune-skimmer/src/`, `examples/dune-skimmer/README.md`.
- Dependencies: 202, 219, 252, 267, 277; journey200-192-accepted.

### 348. Crystal Cavern: discovered-room map and player waypoint

Add a compact map that reveals visited/visible tiles and lets the player choose one waypoint only on discovered walkable space. Show a bearing to that waypoint while exploring and mark discovered objective locations. Do not expose the autopilot's full solution or unvisited crystals through walls.

- Owner: `examples/crystal-cavern/src/`, `examples/crystal-cavern/README.md`.
- Dependencies: 257, 258, 268, 279, 283; journey200-193-accepted.

### 349. Siege Foundry: demolition with protected cargo

Add one optional contract placing a clearly marked protected cargo body among supports. Win requires all designated targets down and cargo below an authored displacement/rotation damage limit; define failure during the same final-shot settling window. Keep the existing range as the default contract and keep the existing arc.

- Owner: `examples/siege-foundry/src/`, `examples/siege-foundry/README.md`.
- Dependencies: 224, 226, 258, 275, 277; journey200-194-accepted.

### 350. Sky Courier: a two-stop mail itinerary

Add one optional two-stop mission with two small route slices and low-speed delivery zones; accept each parcel once, then expose the next leg without teleporting the plane. Carry hull and a clearly specified overall deadline across legs. Original single delivery stays selectable/default.

- Owner: `examples/sky-courier/src/`, `examples/sky-courier/README.md`.
- Dependencies: 202, 252, 258, 267, 268, 277; journey200-195-accepted.

### 351. Strike: timed offline combat drill with accuracy results

Add an optional 60-second offline drill with accepted shots, hits, eliminations and deaths, ending in a result card and explicit retry. Derive accuracy from accepted events and define multi-hit counting; retain free practice. No network protocol or authoritative FPS weapon changes.

- Owner: `examples/strike/src/`, `examples/strike/README.md`.
- Dependencies: 236, 237, 239, 267, 276, 277, 297; journey200-196-accepted.

### 352. 2D Physics Lab: spring-based body dragging

Add an explicit Grab mode with at most one temporary spring constraint to a bounded kinematic anchor. Pick a real dynamic body at the current pose, move the anchor from pointer/virtual cursor input, and release/cancel cleanly. Keep Drop mode and paused inspection; do not teleport a running body to the pointer.

- Owner: `examples/physics-2d/src/`, `examples/physics-2d/README.md`.
- Dependencies: 215, 220, 221, 223, 279, 285; journey200-197-accepted.

### 353. 3D Physics Lab: controlled surface comparison experiment

Add one selectable experiment with three parallel identical ramps and equal bodies released together over low/medium/high friction surfaces. Show displacement and settling time for each lane; explicit Reset Experiment recreates the same initial conditions. Retain the original yard as a preset.

- Owner: `examples/physics-3d/src/`, `examples/physics-3d/README.md`.
- Dependencies: 221, 224, 226, 275, 276, 285; journey200-198-accepted.

### 354. Courtyard: ordered cover-to-cover training route

Add one optional offline course of three marked cover stations requiring actual arrival, then a specified accepted action at that station. Track splits and final time separately from the existing independent drills; retain free training and 2D overview. Disable the course during peer matches and invalidate/restart it deliberately if scene edits move its geometry.

- Owner: `examples/owned-lab/src/`, `examples/owned-lab/README.md`.
- Dependencies: 233, 235, 239, 240, 252, 258, 267; journey200-199-accepted.

### 355. Orbit Breaker: limited sticky-paddle catch and release

Add an optional challenge with one catch charge per sector. Holding a remappable Catch action during a real paddle contact consumes it and parks the ball on the paddle; a fresh launch releases through the existing charge rule. Catch cannot rescue a missed paddle, grant a life or farm precision points. Keep original mode/default inputs and software raster path.

- Owner: `examples/owned-orbit/src/`, `examples/owned-orbit/README.md`.
- Dependencies: 241, 242, 245, 246, 256, 282, 297; journey200-200-accepted.

## Fifteen shared presentation migrations

### 356. Emberline: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/emberline/src/`, `examples/emberline/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 341.

### 357. Neon Swarm: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/neon-swarm/src/`, `examples/neon-swarm/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 342.

### 358. Pocket Rally: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/pocket-rally/src/`, `examples/pocket-rally/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 343.

### 359. Salvage Pinball: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/salvage-pinball/src/`, `examples/salvage-pinball/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 344.

### 360. Orchard Guard: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/orchard-guard/src/`, `examples/orchard-guard/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 335, 345.

### 361. Marble Run: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/marble-run/src/`, `examples/marble-run/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 335, 346.

### 362. Dune Skimmer: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/dune-skimmer/src/`, `examples/dune-skimmer/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 347.

### 363. Crystal Cavern: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/crystal-cavern/src/`, `examples/crystal-cavern/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 348.

### 364. Siege Foundry: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/siege-foundry/src/`, `examples/siege-foundry/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 349.

### 365. Sky Courier: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/sky-courier/src/`, `examples/sky-courier/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 350.

### 366. Strike: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/strike/src/`, `examples/strike/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 351.

### 367. 2D Physics Lab: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/physics-2d/src/`, `examples/physics-2d/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 352.

### 368. 3D Physics Lab: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/physics-3d/src/`, `examples/physics-3d/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 353.

### 369. Courtyard: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/owned-lab/src/`, `examples/owned-lab/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 354.

### 370. Orbit Breaker: shared responsive presentation adoption

Replace repeated local HUD/menu calculations with the new shared layout, measured text, cards, controls and preference APIs; preserve world framing and custom host behavior.

- Owner: `examples/owned-orbit/src/`, `examples/owned-orbit/README.md`.
- Dependencies: 271, 272, 273, 275, 276, 277, 278, 282, 286, 290, 292, 295, 300, 355.

## Fifteen semantic host integrations

### 371. Emberline: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/emberline/index.html`, `web/tests/journey400-emberline-host.mjs`, `examples/emberline/src/`, `examples/emberline/README.md`.
- Dependencies: 280, 296, 310, 334, 341, 356.

### 372. Neon Swarm: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/neon-swarm/index.html`, `web/tests/journey400-neon-swarm-host.mjs`, `examples/neon-swarm/src/`, `examples/neon-swarm/README.md`.
- Dependencies: 280, 296, 310, 334, 342, 357.

### 373. Pocket Rally: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/pocket-rally/index.html`, `web/tests/journey400-pocket-rally-host.mjs`, `examples/pocket-rally/src/`, `examples/pocket-rally/README.md`.
- Dependencies: 280, 296, 310, 334, 343, 358.

### 374. Salvage Pinball: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/salvage-pinball/index.html`, `web/tests/journey400-salvage-pinball-host.mjs`, `examples/salvage-pinball/src/`, `examples/salvage-pinball/README.md`.
- Dependencies: 280, 296, 310, 334, 344, 359.

### 375. Orchard Guard: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/orchard-guard/index.html`, `web/tests/journey400-orchard-guard-host.mjs`, `examples/orchard-guard/src/`, `examples/orchard-guard/README.md`.
- Dependencies: 280, 296, 310, 334, 335, 345, 360.

### 376. Marble Run: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/marble-run/index.html`, `web/tests/journey400-marble-run-host.mjs`, `examples/marble-run/src/`, `examples/marble-run/README.md`.
- Dependencies: 280, 296, 310, 334, 335, 346, 361.

### 377. Dune Skimmer: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/dune-skimmer/index.html`, `web/tests/journey400-dune-skimmer-host.mjs`, `examples/dune-skimmer/src/`, `examples/dune-skimmer/README.md`.
- Dependencies: 280, 296, 310, 334, 347, 362.

### 378. Crystal Cavern: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/crystal-cavern/index.html`, `web/tests/journey400-crystal-cavern-host.mjs`, `examples/crystal-cavern/src/`, `examples/crystal-cavern/README.md`.
- Dependencies: 280, 296, 310, 334, 348, 363.

### 379. Siege Foundry: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/siege-foundry/index.html`, `web/tests/journey400-siege-foundry-host.mjs`, `examples/siege-foundry/src/`, `examples/siege-foundry/README.md`.
- Dependencies: 280, 296, 310, 334, 349, 364.

### 380. Sky Courier: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/sky-courier/index.html`, `web/tests/journey400-sky-courier-host.mjs`, `examples/sky-courier/src/`, `examples/sky-courier/README.md`.
- Dependencies: 280, 296, 310, 334, 350, 365.

### 381. Strike: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/strike/index.html`, `web/tests/journey400-strike-host.mjs`, `examples/strike/src/`, `examples/strike/README.md`.
- Dependencies: 280, 296, 310, 334, 351, 366.

### 382. 2D Physics Lab: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/physics-2d/index.html`, `web/tests/journey400-physics-2d-host.mjs`, `examples/physics-2d/src/`, `examples/physics-2d/README.md`.
- Dependencies: 280, 296, 310, 334, 352, 367.

### 383. 3D Physics Lab: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/physics-3d/index.html`, `web/tests/journey400-physics-3d-host.mjs`, `examples/physics-3d/src/`, `examples/physics-3d/README.md`.
- Dependencies: 280, 296, 310, 334, 353, 368.

### 384. Courtyard: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/owned/index.html`, `web/tests/journey400-owned-lab-host.mjs`, `examples/owned-lab/src/`, `examples/owned-lab/README.md`.
- Dependencies: 280, 296, 310, 334, 354, 369.

### 385. Orbit Breaker: semantic status and new-feature controls

Expose the actual new-feature state through a bounded semantic snapshot, update visible controls/help, and preserve focus, remapping and host lifecycle behavior.

- Owner: `web/owned-orbit/index.html`, `web/tests/journey400-owned-orbit-host.mjs`, `examples/owned-orbit/src/`, `examples/owned-orbit/README.md`.
- Dependencies: 280, 296, 310, 334, 355, 370.

## Fifteen new-feature scenario and visual passes

### 386. Emberline: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/emberline/src/`, `examples/emberline/tests/journey400_386.rs`, `examples/emberline/README.md`, `web/emberline/index.html`, `web/tests/journey400-emberline-host.mjs`, `target/journey-400/visual/emberline/`.
- Dependencies: 270, 330, 334, 336, 341, 356, 371; fresh-integrated-journey400-web-build.

### 387. Neon Swarm: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/neon-swarm/src/`, `examples/neon-swarm/tests/journey400_387.rs`, `examples/neon-swarm/README.md`, `web/neon-swarm/index.html`, `web/tests/journey400-neon-swarm-host.mjs`, `target/journey-400/visual/neon-swarm/`.
- Dependencies: 270, 330, 334, 336, 342, 357, 372; fresh-integrated-journey400-web-build.

### 388. Pocket Rally: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/pocket-rally/src/`, `examples/pocket-rally/tests/journey400_388.rs`, `examples/pocket-rally/README.md`, `web/pocket-rally/index.html`, `web/tests/journey400-pocket-rally-host.mjs`, `target/journey-400/visual/pocket-rally/`.
- Dependencies: 270, 330, 334, 336, 343, 358, 373; fresh-integrated-journey400-web-build.

### 389. Salvage Pinball: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/salvage-pinball/src/`, `examples/salvage-pinball/tests/journey400_389.rs`, `examples/salvage-pinball/README.md`, `web/salvage-pinball/index.html`, `web/tests/journey400-salvage-pinball-host.mjs`, `target/journey-400/visual/salvage-pinball/`.
- Dependencies: 270, 330, 334, 336, 344, 359, 374; fresh-integrated-journey400-web-build.

### 390. Orchard Guard: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/orchard-guard/src/`, `examples/orchard-guard/tests/journey400_390.rs`, `examples/orchard-guard/README.md`, `web/orchard-guard/index.html`, `web/tests/journey400-orchard-guard-host.mjs`, `target/journey-400/visual/orchard-guard/`.
- Dependencies: 270, 330, 334, 335, 336, 345, 360, 375; fresh-integrated-journey400-web-build.

### 391. Marble Run: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/marble-run/src/`, `examples/marble-run/tests/journey400_391.rs`, `examples/marble-run/README.md`, `web/marble-run/index.html`, `web/tests/journey400-marble-run-host.mjs`, `target/journey-400/visual/marble-run/`.
- Dependencies: 270, 330, 334, 335, 336, 346, 361, 376; fresh-integrated-journey400-web-build.

### 392. Dune Skimmer: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/dune-skimmer/src/`, `examples/dune-skimmer/tests/journey400_392.rs`, `examples/dune-skimmer/README.md`, `web/dune-skimmer/index.html`, `web/tests/journey400-dune-skimmer-host.mjs`, `target/journey-400/visual/dune-skimmer/`.
- Dependencies: 270, 330, 334, 336, 347, 362, 377; fresh-integrated-journey400-web-build.

### 393. Crystal Cavern: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/crystal-cavern/src/`, `examples/crystal-cavern/tests/journey400_393.rs`, `examples/crystal-cavern/README.md`, `web/crystal-cavern/index.html`, `web/tests/journey400-crystal-cavern-host.mjs`, `target/journey-400/visual/crystal-cavern/`.
- Dependencies: 270, 330, 334, 336, 348, 363, 378; fresh-integrated-journey400-web-build.

### 394. Siege Foundry: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/siege-foundry/src/`, `examples/siege-foundry/tests/journey400_394.rs`, `examples/siege-foundry/README.md`, `web/siege-foundry/index.html`, `web/tests/journey400-siege-foundry-host.mjs`, `target/journey-400/visual/siege-foundry/`.
- Dependencies: 270, 330, 334, 336, 349, 364, 379; fresh-integrated-journey400-web-build.

### 395. Sky Courier: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/sky-courier/src/`, `examples/sky-courier/tests/journey400_395.rs`, `examples/sky-courier/README.md`, `web/sky-courier/index.html`, `web/tests/journey400-sky-courier-host.mjs`, `target/journey-400/visual/sky-courier/`.
- Dependencies: 270, 330, 334, 336, 350, 365, 380; fresh-integrated-journey400-web-build.

### 396. Strike: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/strike/src/`, `examples/strike/tests/journey400_396.rs`, `examples/strike/README.md`, `web/strike/index.html`, `web/tests/journey400-strike-host.mjs`, `target/journey-400/visual/strike/`.
- Dependencies: 270, 330, 334, 336, 351, 366, 381; fresh-integrated-journey400-web-build.

### 397. 2D Physics Lab: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/physics-2d/src/`, `examples/physics-2d/tests/journey400_397.rs`, `examples/physics-2d/README.md`, `web/physics-2d/index.html`, `web/tests/journey400-physics-2d-host.mjs`, `target/journey-400/visual/physics-2d/`.
- Dependencies: 270, 330, 334, 336, 352, 367, 382; fresh-integrated-journey400-web-build.

### 398. 3D Physics Lab: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/physics-3d/src/`, `examples/physics-3d/tests/journey400_398.rs`, `examples/physics-3d/README.md`, `web/physics-3d/index.html`, `web/tests/journey400-physics-3d-host.mjs`, `target/journey-400/visual/physics-3d/`.
- Dependencies: 270, 330, 334, 336, 353, 368, 383; fresh-integrated-journey400-web-build.

### 399. Courtyard: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/owned-lab/src/`, `examples/owned-lab/tests/journey400_399.rs`, `examples/owned-lab/README.md`, `web/owned/index.html`, `web/tests/journey400-owned-lab-host.mjs`, `target/journey-400/visual/owned-lab/`.
- Dependencies: 270, 330, 334, 336, 354, 369, 384; fresh-integrated-journey400-web-build.

### 400. Orbit Breaker: new-feature scenarios and visual regressions

Add deterministic scenarios for the new feature and verify its real success/failure, responsive UI, input, preferences and semantic status against freshly staged artifacts.

- Owner: `examples/owned-orbit/src/`, `examples/owned-orbit/tests/journey400_400.rs`, `examples/owned-orbit/README.md`, `web/owned-orbit/index.html`, `web/tests/journey400-owned-orbit-host.mjs`, `target/journey-400/visual/owned-orbit/`.
- Dependencies: 270, 330, 334, 336, 355, 370, 385; fresh-integrated-journey400-web-build.

## Completion contract

Each implementation must be registered and exercised through a real consumer or runnable example. Public APIs need compiling usage examples; changed semantics need meaningful error, capacity, reset and timing regressions. Rust checks are package-scoped, offline/locked, at most two jobs. Python remains stdlib-only; browser hosts use standard APIs. Preserve existing tests and uncommitted work.

The parent owns shared registrations, consumer sequencing, broad native/Wasm checks, fresh build provenance and staged browser verification. A source test, build, browser playtest, device test and audio listening check are different evidence. Source edits after a screenshot require a fresh build and recheck. No deployment, commit, PR or recurring scheduler is part of this request.
