# Ferravex Journey200: engine ergonomics and polished games

Requested 2026-10-22. This is a new 200-item implementation roadmap built on the completed [Journey50 baseline](JOURNEY_50_RESULTS.md). Each numbered item has one implementation agent, explicit ownership and acceptance checks. A planned or delegated item is not an implementation or verification claim.

The work comprises 100 engine improvements, 30 developer/browser improvements, four focused passes over each of the fifteen demos (60 items), and ten integration/documentation improvements. Agents launch in dependency-aware waves; the parent collects receipts, wires shared APIs, stages current artifacts and checks the combined result. All original uncommitted work is preserved. No commits or deployment are implied.

## Integration contract

The existing Journey50 implementation is the baseline. Preserve all current uncommitted work. Read AGENTS.md, CONTRIBUTING.md, DEPENDENCIES.md, docs/AGENT_GUIDE.md and your task entry in docs/JOURNEY_200.md. Implement a real useful addition or improvement; inspect current code first and avoid duplicate APIs. Prefer compatible additive methods, checked inputs, bounded storage and clear errors. If the exact capability already exists, improve its concrete ergonomic/edge-case gap within the task scope and explain evidence instead of inventing a redundant subsystem.

All Rust dependencies remain first-party/std-only; Python tools use stdlib; browser runtime uses standard APIs. Do not modify manifests or Cargo.lock without reporting the concrete requirement to parent. Preserve existing wire formats, native/Wasm exports and feature builds. Keep fixed ticks separate from render time, consume gameplay edges on actual ticks, emit feedback from accepted events, and preserve authoritative network/replay progression under local menus.

Only edit your listed ownership paths and your receipt. Tests can be inline in owned files or in a unique test file named journey200_NNN under the owning crate's tests. Parent owns module registrations/reexports outside explicitly assigned roots, shared tests outside ownership, integration, roadmap/task statuses and staging. Do not edit another owner's file to fix a temporary compile error. Report exact required wiring/API signature to parent. Do not format whole packages/workspace; rustfmt only owned files. No commits, branches, PRs, deployment, scheduling or further delegation.

Use targeted validation offline/locked with at most two Cargo jobs. Do not run workspace builds, broad tests, shared dist/site generation or repeated polling. Concurrent edits may temporarily block compilation: record the first relevant error, continue independent work, and report it for integration. Preserve existing tests; add behavioral regressions for changed semantics and compiling examples for public APIs. Avoid test-only feature claims and unregistered code claims.

Write target/journey-200/receipts/NNN.md with changed paths, concrete behavior, exact public APIs, required registration, executed commands/results and unverified limits. Your final result must say what is actually implemented and tested, what remains blocked, and any cross-owner integration needs. Do not mark your task fully integrated; parent verifies combined output.


## Task list

### 001. Checked session setup and paused stepping

Add a validated session configuration/try-constructor and explicit single fixed-step while paused for simulation inspectors. Preserve Ready/Playing/Paused/Won/Lost semantics, inactive-time discard and existing new/advance behavior; reject stepping Ready/terminal states and document tick accounting.

- Ownership: `crates/ferravex-core/src/session.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-core --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; combined core tests and Wasm check passed, workspace/demo validation pending.

### 002. Frame-time percentiles and jitter

Extend FrameStats with useful bounded sample-count, percentile and jitter diagnostics, handling warm-up, zero capacity and invalid samples. Preserve clock code and avoid allocating in record().

- Ownership: `crates/ferravex-core/src/lib.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-core --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; combined core tests and Wasm check passed, workspace/demo validation pending.

### 003. Application frame reports

Add an ergonomic frame outcome/report exposing executed update count, interpolation, stop state and dropped-time information without breaking Game implementations or exactly-once cleanup/error ordering.

- Ownership: `crates/ferravex-engine/src/app.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 004. Drift-free simulation timers

Add reusable one-shot/repeating timers driven by supplied simulation time, retaining residual elapsed time and bounded catch-up reporting. Keep pause/time-scale behavior explicit and checked overflow/zero intervals.

- Ownership: `crates/ferravex-engine/src/time.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 005. Allocation-free scheduled-task draining

Add iterator/callback or caller-buffer ready draining that reuses storage, plus checked rescheduling of a TaskId while preserving equal-tick order, cancellation and capacity semantics.

- Ownership: `crates/ferravex-engine/src/scheduler.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 006. Observable deferred commands

Add safe command-queue inspection and bounded drain/apply outcomes so accepted, rejected and remaining work are explicit. Preserve command order and make partial application/failure semantics documented and tested.

- Ownership: `crates/ferravex-engine/src/commands.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 007. Pool capacity planning and reuse

Improve the existing pool with capacity/available inspection and useful safe retain/reuse or iteration helpers. Preserve generational-handle safety, stale-handle detection and existing allocation guarantees.

- Ownership: `crates/ferravex-engine/src/pool.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 008. Bounded bitset algebra

Add missing useful intersection/difference/union and set-bit iteration operations with explicit length rules, correct trailing bits and no hot-loop allocations.

- Ownership: `crates/ferravex-engine/src/bitset.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 009. Snapshot bracketing

Add checked snapshot bracketing for interpolation and explicit before-oldest/after-newest results. Handle duplicate or nonmonotonic ticks and wrapped buffers without extrapolating silently.

- Ownership: `crates/ferravex-engine/src/snapshots.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 010. Spatial-query saturation reporting

Make bounded query truncation observable and add a caller-buffer query path where absent. Preserve deterministic order, finite coordinate checks and current spatial-index semantics.

- Ownership: `crates/ferravex-engine/src/spatial.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 011. Readable tag predicates

Add composable required/all, optional/any and excluded/none tag predicates over existing tag types, with validated construction and direct matching examples.

- Ownership: `crates/ferravex-engine/src/tags.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 012. Weighted-selection validation

Add actionable validation and checked selection for zero/invalid/overflowing weights; retain deterministic existing streams and provide a reusable prepared distribution when the current representation supports it.

- Ownership: `crates/ferravex-engine/src/weighted.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 013. Shuffle-bag cycle fairness

Add explicit cycle/reset inspection and an opt-in no-immediate-repeat boundary policy for shuffle bags, handling one/two-element bags without biasing or changing existing default output.

- Ownership: `crates/ferravex-engine/src/shuffle.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 014. Explainable work budgets

Add cost-aware reservations/refunds and consumed/denied diagnostics for existing bounded work budgets. Reject overflow and preserve original limits and deterministic order.

- Ownership: `crates/ferravex-engine/src/work_budget.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 015. Angle-aware smoothing

Provide checked shortest-arc angular smoothing and a well-defined snap/reset path using existing smoothing primitives. Test wraparound, zero/large dt and invalid values without changing legacy defaults.

- Ownership: `crates/ferravex-engine/src/smoothing.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 016. Transactional resource replacement

Add typed take/replace or checked-update ergonomics that preserve the previous resource when validation fails. Keep capacity invariants, type-specific errors and existing initialization methods intact.

- Ownership: `crates/ferravex-engine/src/resources.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 017. Atomic event batches

Add checked batch enqueue with explicit all-or-nothing capacity behavior and useful accepted/rejected outcomes. Preserve individual-send overflow policies and queue metrics.

- Ownership: `crates/ferravex-engine/src/events.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 018. Frame-event reader lag diagnostics

Expose reader lag/missed-frame status and safe cursor reset/drain semantics so consumers can distinguish empty from expired data. Preserve frame ordering and bounded retention.

- Ownership: `crates/ferravex-engine/src/frame_events.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 019. System execution layers

Expose deterministic dependency layers and dependency inspection derived from the existing graph, with cycle/missing-node diagnostics and stable ordering. Do not add multithreaded execution.

- Ownership: `crates/ferravex-engine/src/system_order.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 020. State-transition trace

Add an optional bounded transition trace or observation adapter for successful commits, including sequence/cause context. Keep guards, pending replacement and once-per-commit hooks compatible.

- Ownership: `crates/ferravex-engine/src/state.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 021. Asset manifest fingerprints

Expose a stable deterministic read-only manifest of normalized asset paths, lengths and content fingerprints suitable for tooling/reload diagnostics. Preserve bundle encoding and Arc identity on no-op edits.

- Ownership: `crates/ferravex-engine/src/assets.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 022. Streaming diagnostics export

Add useful NDJSON/streaming records and severity/counter summaries without changing the existing versioned JSON schema. Preserve escaping, bounded context and writer-error propagation.

- Ownership: `crates/ferravex-engine/src/diagnostics.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 023. Unbiased checked random ranges

Add missing checked integer-range sampling using rejection rather than modulo bias, with explicit invalid/full-width cases. Preserve every existing RNG algorithm/output and named-stream contract.

- Ownership: `crates/ferravex-engine/src/random.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-engine --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 024. Transactional ECS bulk removal

Add a useful checked bulk entity removal or prevalidated batch operation that handles duplicates/stale handles deterministically and avoids partial mutation on validation failure. Preserve safe borrowing and get_many behavior.

- Ownership: `crates/ferravex-ecs/src/lib.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-ecs --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 025. Robust vector normalization

Add explicit checked normalization/length-limiting conveniences robust to zero, non-finite and extreme finite vectors, using existing types. Keep original arithmetic and normalization methods compatible.

- Ownership: `crates/ferravex-math/src/vector.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-math --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; source reviewed; combined math tests and Wasm check passed, workspace/demo validation pending.

### 026. Checked matrix inversion and projection

Add or strengthen fallible inverse/projection helpers with documented singularity/finite behavior and actionable failure results. Cover degenerate cameras, tiny scales and round-trip points.

- Ownership: `crates/ferravex-math/src/matrix.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-math --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; source reviewed; combined math tests and Wasm check passed, workspace/demo validation pending.

### 027. Quaternion angular motion helpers

Add checked angular distance/rotate-towards helpers using the shortest arc, with clear units and antipodal/near-identical behavior. Preserve current interpolation APIs.

- Ownership: `crates/ferravex-math/src/quaternion.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-math --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; source reviewed; combined math tests and Wasm check passed, workspace/demo validation pending.

### 028. Linear and sRGB color helpers

Add explicit linear/sRGB conversions and checked color handling for game/UI authoring. Document alpha handling, clamp policy and HDR behavior; preserve existing Color constructors and lerp.

- Ownership: `crates/ferravex-math/src/shapes.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-math --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; source reviewed; combined math tests and Wasm check passed, workspace/demo validation pending.

### 029. Reliable translational sweeps

Improve checked collision/sweep edge behavior for zero movement, initial overlap, grazing and extreme finite inputs. Add a useful explicit hit classification if current APIs conflate these cases; preserve solver semantics.

- Ownership: `crates/ferravex-dynamics/src/collision.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-dynamics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 030. Safe image-region operations

Add checked crop/blit or region operations needed by atlas/tools, with clear bounds/stride/alpha rules and transactional invalid-input behavior. Reuse existing image types and avoid redundant APIs.

- Ownership: `crates/ferravex-graphics/src/image.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-graphics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 031. Validated rigid-body authoring

Add ergonomic checked body-definition validation/building with field-specific errors for mass, pose, velocity and dimensions. Keep public struct literal compatibility and existing constructors.

- Ownership: `crates/ferravex-physics/src/body.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-physics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 032. Physics query result helpers

Extend query ergonomics with nearest/all-hit caller-buffer results or explicit query diagnostics where missing. Preserve QueryFilter behavior, sensor/layer rules, current-pose guarantees and 2D/3D feature builds.

- Ownership: `crates/ferravex-physics/src/api.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-physics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 033. Character motor recovery diagnostics

Improve character-motor support/recovery reporting and safe reset/teleport behavior, particularly moving support, slopes and initial overlap. Add behavioral 2D/3D regressions and document how hosts reset interpolation.

- Ownership: `crates/ferravex-physics/src/character/`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-physics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 034. Checked joint configuration

Provide actionable validation and safe live limit/motor configuration for existing joints. Reject non-finite/inverted limits transactionally and preserve solver/handle contracts.

- Ownership: `crates/ferravex-physics/src/joints.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-physics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 035. Explicit physics material mixing

Add checked material authoring and documented combination helpers for friction/restitution, including invalid/extreme values and zero-friction behavior. Preserve default mixing.

- Ownership: `crates/ferravex-physics/src/material.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-physics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 036. Mass-property diagnostics

Add checked mass/inertia construction and useful diagnostics for degenerate shapes or overflow. Verify analytic simple shapes and preserve existing valid results.

- Ownership: `crates/ferravex-physics/src/mass.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-physics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 037. Sleep and wake inspection

Expose safe sleep/wake state and bounded counters useful to physics labs, and fix concrete wake propagation gaps on supported world edits. Preserve contact lifecycle and solver ordering; do not edit world submodules owned implicitly by this parent unless necessary and reported.

- Ownership: `crates/ferravex-physics/src/world.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-physics --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 038. Ability availability explanations

Expose non-mutating typed availability information (charges, cooldown, resource cost, busy reservation) for HUD/AI. Ensure preview agrees with begin/activate and cannot reserve or consume resources.

- Ownership: `crates/ferravex-play/src/ability.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 039. Action interruption reasons

Add checked action-timeline interruption/remaining-phase inspection so hosts can explain unavailable actions and cancel predictably. Preserve accepted-event serials and bounded hit-window behavior.

- Ownership: `crates/ferravex-play/src/action.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 040. Inspectable status effects

Expose bounded iteration/status views of active effects, durations, stacks and periodic progress, with transactional refresh/removal semantics. Keep damage math and wire-independent types compatible.

- Ownership: `crates/ferravex-play/src/attributes.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 041. Round outcome summaries

Add useful read-only round/countdown/outcome summaries and safe reset semantics for menus, avoiding duplicated phase logic. Cover terminal states, zero dt and repeated transitions.

- Ownership: `crates/ferravex-play/src/rounds.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 042. Encounter admission preview

Provide non-mutating spawn-admission decisions with explicit cost/alive-limit/pacing reasons and next-availability information. Keep deterministic director credit accrual and existing admit behavior.

- Ownership: `crates/ferravex-play/src/encounter.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 043. Objective progress and blockers

Expose objective progress and prerequisite blockers for HUD/journals without modifying snapshots or event serials. Add safe iteration and prove read-only inspection matches unlock/completion behavior.

- Ownership: `crates/ferravex-play/src/objective.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 044. Atomic environment construction

Make compound stairs/ramp/bridge authoring transactional on capacity/invalid geometry failure and expose useful bounds/count planning. Reuse existing BoxPiece representation and preserve valid geometry.

- Ownership: `crates/ferravex-play/src/environment.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 045. Vehicle telemetry and safe reset

Add checked configuration validation and read-only planar/flight telemetry useful for HUD/recovery, plus safe pose/velocity reset helpers where absent. Preserve handling defaults and deterministic stepping.

- Ownership: `crates/ferravex-play/src/vehicle.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 046. Racing driver recovery status

Expose deterministic driver progress/recovery/wrong-way reasoning and reliable reset after teleport/restart. Improve actual stuck-edge behavior without changing normal-route behavior unnecessarily.

- Ownership: `crates/ferravex-play/src/racing.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 047. Checked steering and damping

Add reusable bounded acceleration/approach helpers for Vec2/Vec3 movement and verify partition stability and finite results. Keep collision-response helpers compatible.

- Ownership: `crates/ferravex-play/src/motion.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 048. Tuning profile diffs

Add an inspectable validated diff/preview between tuning profiles so tools can report changed controls/assists before replacement. Preserve fingerprint and save compatibility.

- Ownership: `crates/ferravex-play/src/tuning.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 049. Stable adaptive-quality decisions

Expose quality-decision reasons and reset/warm-up state; improve hysteresis against transient spikes without obscuring sustained overload. Add deterministic traces showing stability and recovery.

- Ownership: `crates/ferravex-play/src/performance.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 050. Directional UI focus

Add spatial directional focus navigation among enabled visible widgets, with stable tie-breaking and explicit wrap policy. Preserve tab order, hit testing and existing UI ownership semantics.

- Ownership: `crates/ferravex-play/src/ui.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 051. Action descriptors for dynamic hints

Expose read-only action metadata and validated binding enumeration suitable for remapped control hints. Preserve typed ActionHandle provenance, context gating and edge consumption.

- Ownership: `crates/ferravex-play/src/input/mod.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 052. Input binding conflict diagnostics

Add non-mutating config validation/conflict reporting with action/context detail and transactional import behavior. Distinguish legitimate context reuse from conflicting active bindings.

- Ownership: `crates/ferravex-play/src/input/config.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 053. Touch-layout safe-area validation

Add checked safe-area adaptation and overlap/out-of-bounds diagnostics for touch layouts, preserving normalized layouts and avoiding jumps on resize.

- Ownership: `crates/ferravex-play/src/input/touch/layout.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 054. Camera framing constraints

Improve checked group/subject framing for aspect ratio, margins and degenerate targets. Add clear framing results and tests for portrait, ultrawide and near-zero bounds.

- Ownership: `crates/ferravex-play/src/camera/composition.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 055. Camera obstruction diagnostics

Expose useful obstruction/clearance outcomes and safe probe configuration, fixing reproducible initial-overlap or zero-length edge cases. Preserve collision-query abstraction.

- Ownership: `crates/ferravex-play/src/camera/collision.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 056. Deterministic shake envelopes

Add useful impulse/envelope inspection and explicit reset/cancel semantics, ensuring deterministic decay, bounded aggregation and zero-intensity/no-motion behavior.

- Ownership: `crates/ferravex-play/src/camera/shake.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 057. Follow-camera lookahead tuning

Add validated lookahead limits/dead-zone or tuning update ergonomics where missing, preventing overshoot after stops/reversals. Preserve reset/snap and existing authored defaults.

- Ownership: `crates/ferravex-play/src/camera/follow.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 058. Accessible presentation impulses

Add a bounded presentation-only impact envelope for hit emphasis/flash decay with explicit reduced-motion/flash policy behavior. Keep simulation time untouched, event dedup intact and sound/haptics preserved.

- Ownership: `crates/ferravex-play/src/feedback.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 059. Audio voice-priority policy

Add an opt-in bounded voice admission/stealing policy with explicit outcomes for critical versus cosmetic cues. Keep hot mixing allocation-free, fades compatible and default playback behavior stable.

- Ownership: `crates/ferravex-play/src/audio/mixer.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 060. Smoothed acoustic environment changes

Add checked gradual environment/occlusion transitions to avoid abrupt spatial changes, with reset and finite-state behavior. Preserve authored defaults and sample-rate independence.

- Ownership: `crates/ferravex-play/src/audio/environment.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 061. Synth envelope release controls

Expose validated note/envelope release or retrigger controls that avoid discontinuities, reusing synthesis primitives. Test finite samples, exact silence and bounded work.

- Ownership: `crates/ferravex-play/src/audio/synth.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 062. Animation-track sampling diagnostics

Add checked authoring/inspection helpers that identify malformed keyframe ordering and support reusable sample-into workflows without changing Track/SpriteClip valid interpolation.

- Ownership: `crates/ferravex-play/src/animation.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 063. Animation event catch-up reporting

Make event overflow/skipped-cycle behavior inspectable and preserve exact once-per-crossing semantics across seek, speed change, pause and large dt. Add bounded iteration where useful.

- Ownership: `crates/ferravex-play/src/animation/playback.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 064. Animation-graph transition explanations

Expose non-mutating transition eligibility/selected-edge diagnostics and stable parameter inspection for tools. Preserve trigger consumption, transition order and graph-player behavior.

- Ownership: `crates/ferravex-play/src/animation/graph.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 065. Animation crossfade continuity

Improve interrupted crossfade continuity and validate masked/additive pose application transactionally. Add meaningful regressions for repeated retargets, mismatched joints and zero-duration transitions.

- Ownership: `crates/ferravex-play/src/animation/pose.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 066. Foot-contact hysteresis

Improve stable foot planting across marginal contact changes and resets with validated thresholds and inspectable correction. Preserve unreachable-target bounds and finite two-bone results.

- Ownership: `crates/ferravex-play/src/animation/contact.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 067. Resumable navigation searches

Expose bounded search progress/cancellation or restart ergonomics with explicit exhausted/unreachable outcomes. Preserve deterministic tie-breaking and caller work budgets.

- Ownership: `crates/ferravex-play/src/navigation/search.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 068. Route progress and projection

Add checked nearest-point/progress sampling for routes, handling closed paths, duplicate points and discontinuous recovery. Keep navigation geometry and existing route APIs compatible.

- Ownership: `crates/ferravex-play/src/navigation/route.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 069. Navigation mesh authoring diagnostics

Expose actionable malformed polygon/adjacency diagnostics and transactional construction checks; cover winding, degeneracy and invalid portals while preserving valid mesh paths.

- Ownership: `crates/ferravex-play/src/navigation/mesh.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 070. Stable perception target selection

Add opt-in target-switch hysteresis/selection explanation using existing memory and candidates, avoiding oscillation while respecting expiry, masks and visibility budgets.

- Ownership: `crates/ferravex-play/src/ai/perception.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 071. Enemy intent inspection

Expose useful attack windup/recovery/target-loss intent status and tighten cancellation/reset behavior. Keep accepted combat events deterministic and avoid firing after invalid target loss.

- Ownership: `crates/ferravex-play/src/ai/enemy.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 072. Squad reservation expiry

Add explicit reservation inspection/expiry or safe ownership renewal for cover coordination, preserving deterministic selection and preventing abandoned reservations from blocking squads.

- Ownership: `crates/ferravex-play/src/ai/squad.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 073. Decision-graph explanations

Add checked decision graph diagnostics and read-only transition explanations, retaining priority and elapsed-time rules. Test cycles where allowed, invalid edges and deterministic guards.

- Ownership: `crates/ferravex-play/src/ai/decision.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 074. Budgeted emitter bursts

Add explicit burst admission/dropped-particle outcomes with predictable capacity and seeded emission. Preserve warmed buffer reuse and normal emission timing.

- Ownership: `crates/ferravex-play/src/particles/emitter.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 075. Trail discontinuity handling

Add explicit break/teleport/reset behavior so trails never bridge respawns; preserve lifetime, ordering and bounded storage under repeated discontinuities.

- Ownership: `crates/ferravex-play/src/particles/trail.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 076. Ribbon taper and finite geometry

Add checked width/taper authoring and stable degenerate-segment handling so trails generate finite predictable geometry at corners and reversals without new frame allocations.

- Ownership: `crates/ferravex-play/src/particles/ribbon.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 077. Beam endpoint stability

Add validated beam update/reset helpers and robust zero-length/camera-parallel geometry behavior, preserving bounded tessellation and deterministic appearance.

- Ownership: `crates/ferravex-play/src/particles/beam.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 078. Machine-readable scene schema

Expose deterministic schema introspection suitable for coding agents: field names/types, constraints and defaults through std-only structured views/export. Keep schema/cooked formats unchanged.

- Ownership: `crates/ferravex-play/src/authoring/schema.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 079. Prefab expansion diagnostics

Report useful prefab dependency/cycle/depth paths and enforce bounded transactional expansion. Preserve valid prefab output and authored source locations where available.

- Ownership: `crates/ferravex-play/src/authoring/expand.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 080. Cooked scene integrity checks

Strengthen checked decoding for malformed lengths, duplicate IDs and invalid UTF-8/finite values; add read-only format metadata without changing valid wire bytes.

- Ownership: `crates/ferravex-play/src/authoring/cooked.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 081. Scene path lookup and identity

Add checked unambiguous hierarchical path lookup/inspection with explicit duplicate-name policy and stale-handle behavior. Reuse existing hierarchy traversal and keep batch edits transactional.

- Ownership: `crates/ferravex-play/src/scene.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 082. Preference override composition

Add validated layered preference/override composition and a useful diff for defaults versus user choices, preserving existing save schema and unknown-field policy.

- Ownership: `crates/ferravex-play/src/persistence/preferences.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 083. Record comparison summaries

Expose new-best/tie/worse comparison and delta summaries for records with deterministic ordering and checked arithmetic. Preserve serialized format and record acceptance rules.

- Ownership: `crates/ferravex-play/src/persistence/records.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 084. Ghost interpolation boundaries

Improve checked ghost sampling at start/end/duplicate ticks and discontinuities so replays do not interpolate across teleports. Expose explicit sample state and preserve existing recordings.

- Ownership: `crates/ferravex-play/src/replay/ghost.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 085. Atomic inventory exchange

Add transactional exchange/transfer helpers over bounded inventory with preflight capacity and missing-item diagnostics. Preserve totals on every failed exchange.

- Ownership: `crates/ferravex-play/src/gameplay/inventory.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 086. Crafting requirement preview

Expose non-mutating crafting availability/missing-ingredient results using existing recipes and inventory; ensure preview matches actual atomic craft and handles overflow.

- Ownership: `crates/ferravex-play/src/gameplay/crafting.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 087. Magazine status and reconfiguration

Add useful reload progress/status and checked capacity/reserve changes with explicit ammo conservation. Preserve fire/reload/cancel behavior and avoid duplicate completion events.

- Ownership: `crates/ferravex-play/src/gameplay/magazine.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 088. Wave schedule preview

Expose read-only upcoming-wave/countdown/remaining-spawn status and a safe reset path, preserving deterministic spawn scheduling and bounded catch-up.

- Ownership: `crates/ferravex-play/src/gameplay/waves.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 089. Interaction rejection reasons

Provide non-mutating typed interaction eligibility and interruption reasons for hold/range/focus rules; ensure unavailable actions never consume progress or produce accepted events.

- Ownership: `crates/ferravex-play/src/gameplay/interaction.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 090. Checkpoint recovery outcomes

Add explicit recovery/restore outcomes and useful next/current anchor inspection while preserving rules fingerprints and transactional snapshot restore. Cover stale/mismatched/terminal checkpoints.

- Ownership: `crates/ferravex-play/src/gameplay/checkpoint.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-play --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 091. Consistent renderer widget states

Improve shared enabled/disabled/focus widget ergonomics and clear visual state rendering, using current play UI types. Preserve hit regions and avoid breaking existing Ui callers; do not edit ui/shared.rs.

- Ownership: `crates/ferravex-render/src/ui.rs`.
- Prerequisites: 050.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 092. Remapping-aware player hints

Add bounded formatting/current-device hint APIs based on actual bindings, including keyboard, mouse, controller and touch. Preserve take_tick, focus suppression and draw_controls; report rather than assume task 51 signatures.

- Ownership: `crates/ferravex-render/src/input/player.rs`, `crates/ferravex-render/src/input/player/editor.rs`, `crates/ferravex-render/src/input/player/tests.rs`.
- Prerequisites: 051.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 093. Game-audio suspension controls

Add explicit game-audio pause/resume/reset behavior and cue acceptance outcomes using existing mixer controls. Keep silent/device-unavailable operation safe and preserve original GameAudio callers.

- Ownership: `crates/ferravex-render/src/audio/game.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; parent review and combined validation pending.

### 094. Content reload state inspection

Expose reload status, last-good revision and structured failure context to hosts, ensuring failed reloads preserve live content and accepted generations advance once.

- Ownership: `crates/ferravex-render/src/content.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 095. Visibility workload counters

Add bounded per-frame visible/culled/overflow diagnostics and safe reusable query storage. Preserve culling correctness for invalid bounds and existing scene data.

- Ownership: `crates/ferravex-render/src/visibility.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 096. Checked material parameters

Add typed/checked parameter updates and actionable shader/material mismatch errors where absent. Keep last-good state on failed changes and preserve existing material ABI.

- Ownership: `crates/ferravex-render/src/material.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 097. Retained-rendering capacity telemetry

Expose mesh/instance/draw capacity and rejection counters useful for performance tools, with explicit reset/snapshot APIs. Preserve checked validation, buffer reuse and allocation-sensitive draw paths.

- Ownership: `crates/ferravex-render/src/retained.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 098. Font layout measurements

Add consistent checked width/baseline/line-height measurement utilities for bitmap text, covering scaling, unsupported glyphs and multi-line labels. Match actual raster geometry and avoid allocations.

- Ownership: `crates/ferravex-render/src/font.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 099. Live preference application

Improve validated live preference application/diff outcomes so hosts can consistently update motion, flash, audio and graphics settings without restarting or partially applying invalid values.

- Ownership: `crates/ferravex-render/src/preferences.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 100. Browser save failure semantics

Improve explicit browser-store unavailable/quota/corrupt handling and typed results while preserving last-good saves and existing host ABI. Add narrow integration tests in a unique test file if needed.

- Ownership: `crates/ferravex-render/src/storage/browser.rs`.
- Prerequisites: none.
- Acceptance: cargo test -p ferravex-render --offline --locked -j 2 (narrow owned-module/tests); scoped rustfmt; relevant rustdoc and Clippy.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 101. Safer generated-project customization

Add useful validated template options for tick rate/window title and a machine-readable generation manifest, while preserving no-overwrite/path safety, standalone workspaces and runnable 2D/3D defaults. Update generated guidance to actual APIs only.

- Ownership: `scripts/new-game.py`, `scripts/test_new_game.py`, `templates/`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 102. Structured developer-command results

Add stable machine-readable command outcomes and actionable unknown-package/argument errors for AI callers, retaining dry-run semantics, exit propagation and at most two Cargo jobs.

- Ownership: `scripts/ferravex.py`, `scripts/test_ferravex_cli.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 103. Doctor environment provenance

Report resolved executable paths, versions and bounded probe failures consistently; distinguish missing tools, unusable tools and optional capabilities. Preserve Python 3.11 minimum and read-only behavior.

- Ownership: `scripts/doctor.py`, `scripts/test_doctor.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 104. Changed-scope validation planning

Add explicit changed-path validation planning/dry-run with dependency-aware groups and honest completeness semantics. Keep full mode authoritative and never reuse a stale pass based on file presence.

- Ownership: `scripts/validate.py`, `scripts/test_validate.py`, `.github/workflows/ci.yml`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 105. Catalog capabilities and schema

Add validated optional capability metadata for controls, pause, replay/content features and presentation, verified against source. Preserve host/package IDs and audit existing migration evidence without claiming runtime support from static checks.

- Ownership: `examples/catalog.json`, `scripts/audit-catalog.py`, `scripts/test_catalog.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 106. Fresh smoke artifact provenance

Record binary/capture freshness and hashes plus exact launch parameters in smoke reports; refuse stale captures as success and preserve distinct blocked/failed/prebuilt states.

- Ownership: `scripts/smoke-games.py`, `scripts/test_smoke_games.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 107. Comparable performance reports

Add explicit baseline comparison and noise/sample metadata for existing performance runs, preserving raw measurements and distinguishing insufficient samples from regressions. Do not invent universal budgets.

- Ownership: `scripts/perf-games.py`, `scripts/test_perf.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 108. Wasm size budget reports

Add checked baseline comparison, configurable per-artifact budgets and JSON reports with uncompressed/compressed sizes. Preserve truthful missing-artifact failures and use stdlib only.

- Ownership: `scripts/report-size.py`, `scripts/test_report_size.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 109. Reproducible web build manifests

Record deterministic staged artifact hashes, build profile and source provenance; ensure copied host assets and optional authoring variants are correct. Do not regenerate dist during shared work; test in temporary fixtures.

- Ownership: `scripts/build-web.py`, `scripts/build-owned.py`, `scripts/test_build_manifest.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 110. Reliable local web serving

Improve correct MIME/range/cache behavior and explicit loopback/error diagnostics for local Wasm testing. Preserve standard-library serving and existing default routes; test requests against temporary fixtures.

- Ownership: `scripts/serve-web.py`, `scripts/test_serve_web.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 111. Symbol-aware documentation search

Improve exact/prefix Rust symbol and qualified-path ranking, stable ties and meaningful snippets while retaining local-only search and current index contract.

- Ownership: `scripts/site_search.py`, `scripts/test_search.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 112. Executable documentation checker

Create a stdlib tool that validates local links/anchors and extracts explicitly marked compilable Rust examples for reproducible checks, with JSON diagnostics, scoped paths and no executing arbitrary prose or shell blocks.

- Ownership: `scripts/check-docs.py`, `scripts/test_check_docs.py`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received and source reviewed; targeted results recorded, combined validation pending.

### 113. Accessible catalog filtering

Improve keyboard/filter focus, empty states and shareable URL filter state; retain all game links, accessible counts and existing no-JS fallback. Consume optional metadata defensively.

- Ownership: `web/catalog.js`, `web/index.html`, `web/tests/catalog.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 114. Transactional browser settings import

Improve settings import preview/validation and status announcements so malformed backups never partly overwrite preferences. Preserve sharing, screenshot and frame-limit controls.

- Ownership: `web/player.js`, `web/tests/player.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 115. Pointer and touch cancellation recovery

Fix concrete pointer-cancel, lost capture, multi-touch or focus edge gaps with deterministic host regressions; preserve release gating and prevent stuck actions across resize/suspend.

- Ownership: `web/input.js`, `web/tests/input.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 116. Browser audio interruption recovery

Improve suspended/interrupted audio state transitions, explicit resume outcomes and mute consistency while preserving user activation requirements and avoiding duplicate contexts/voices.

- Ownership: `web/audio.js`, `web/tests/audio.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 117. Browser storage quota diagnostics

Expose actionable quota/unavailable/corrupt results with atomic last-good behavior and bounded input checks. Preserve existing save envelope and host imports.

- Ownership: `web/storage.js`, `web/tests/storage.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 118. Wasm loading progress

Add optional bounded progress notifications with unknown-length support and observer-error isolation, preserving single-download MIME fallback, cancellation, retry and WasmLoadError compatibility.

- Ownership: `web/loader.js`, `web/tests/loader.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 119. Lifecycle suspension precedence

Make overlapping visibility, page lifecycle and graphics-loss suspension reasons explicit and robust to out-of-order restore/disposal. Preserve input/audio reset and prevent double resume.

- Ownership: `web/lifecycle.js`, `web/tests/lifecycle.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 120. Fullscreen focus restoration

Improve focus restoration, rejection feedback and cleanup across fullscreen enter/exit and removed hosts. Preserve keyboard accessibility and user-gesture behavior.

- Ownership: `web/fullscreen.js`, `web/tests/fullscreen.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 121. Retained WebGL capability diagnostics

Expose bounded capability/resource failure diagnostics and recoverable creation behavior without changing renderer imports or successful rendering. Test actual failure branches with focused GL fixtures.

- Ownership: `web/retained.js`, `web/tests/retained-diagnostics.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 122. Effects buffer reuse

Improve warmed buffer reuse and explicit capacity/invalid-upload handling; preserve effect visuals and shader ABI, and verify no repeated buffer allocation in steady state.

- Ownership: `web/effects.js`, `web/tests/effects-buffers.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 123. Consistent visual preference updates

Ensure live reduced-motion/reduced-flash/quality changes apply consistently across resize, restoration and preset changes. Keep gameplay untouched and preserve last-good valid settings.

- Ownership: `web/visuals.js`, `web/tests/visuals.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 124. Exportable runtime diagnostics

Add a bounded, versioned downloadable diagnostic snapshot for support/AI debugging using actual counters and capability states, excluding saved game data or secrets. Keep the details panel accessible.

- Ownership: `web/details.js`, `web/tests/details.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 125. Actionable content-tool errors

Improve content editing/reload error presentation, dirty-state and last-good feedback with accessible status. Preserve authored formats and actual export contract; no speculative schema calls.

- Ownership: `web/content-tools.js`, `web/tests/content-tools.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 126. Shader error navigation

Add useful line-aware shader error presentation and explicit last-good/revert state while retaining shader source safely and preserving the active shader on failed reload.

- Ownership: `web/material-tools.js`, `web/tests/material-tools.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 127. Replay scrubber state clarity

Improve keyboard scrub/seek status and mismatch/finished/error presentation without confusing paused playback with completion. Preserve actual replay host ABI and input ordering.

- Ownership: `web/replay-tools.js`, `web/tests/replay-tools.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 128. Multiplayer connection state UX

Expose clear connecting/ready/disconnected/retry outcomes and prevent stale callbacks from resurrecting old connections. Keep authoritative simulation and existing protocols unchanged.

- Ownership: `web/multiplayer-tools.js`, `web/tests/multiplayer-tools.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 129. Peer negotiation cancellation

Add bounded negotiation timeout/cancellation and stale-session protection for manual WebRTC pairing, with cleanup and actionable error states. Preserve current offer/answer format and no-server design.

- Ownership: `web/owned/peer.js`, `web/tests/peer-link.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; shared wiring added and scoped checks passed, full integration pending.

### 130. Audio ring underrun recovery

Improve bounded underrun/overrun recovery and telemetry with deterministic ring-buffer tests; avoid stale sample bursts, allocation in process and misleading healthy counters.

- Ownership: `web/audio-ring.js`, `web/audio-worklet.js`, `web/tests/audio-ring.mjs`.
- Prerequisites: none.
- Acceptance: Targeted stdlib/Node tests for owned behavior; scoped formatting and whitespace checks.
- Status: implementation received; parent review and combined validation pending.

### 131. Emberline: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Forgiving jump buffering/coyote timing, readable landing/checkpoint feedback and a clean retry flow; preserve authored course and beatable autoplay. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 146 owns those.

- Ownership: `examples/emberline/src/`, `examples/emberline/README.md`.
- Prerequisites: 038, 039, 047, 057, 058, 075, 089, 090, 092, 093, 098, 091.
- Acceptance: cargo test -p emberline --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 132. Neon Swarm: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Clear spawn/attack telegraphs, target and damage feedback, readable wave/progression HUD and accepted-action audio; preserve deterministic combat and autoplay. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 147 owns those.

- Ownership: `examples/neon-swarm/src/`, `examples/neon-swarm/README.md`.
- Prerequisites: 038, 039, 040, 042, 058, 070, 071, 074, 088, 092, 093, 091, 098.
- Acceptance: cargo test -p neon-swarm --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 133. Pocket Rally: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Progress/turn/recovery cues, confident start countdown and clear lap/record comparisons; preserve vehicle handling and portrait HUD fit. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 148 owns those.

- Ownership: `examples/pocket-rally/src/`, `examples/pocket-rally/README.md`.
- Prerequisites: 004, 045, 046, 054, 057, 068, 083, 084, 092, 098, 091.
- Acceptance: cargo test -p pocket-rally --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 134. Salvage Pinball: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Responsive flipper/launch feedback, clear ball-save/drain/bonus progression and low-noise scoring presentation; preserve physics and bounded input buffering. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 149 owns those.

- Ownership: `examples/salvage-pinball/src/`, `examples/salvage-pinball/README.md`.
- Prerequisites: 039, 041, 047, 058, 059, 074, 075, 092, 093, 098, 091.
- Acceptance: cargo test -p salvage-pinball --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 135. Orchard Guard: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Explain build/upgrade affordability and tower stats, telegraph waves, clarify target/slow feedback and preserve between-wave saves and accessible pause flow. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 150 owns those.

- Ownership: `examples/orchard-guard/src/`, `examples/orchard-guard/README.md`.
- Prerequisites: 038, 040, 042, 043, 058, 070, 083, 085, 086, 088, 092, 091, 098.
- Acceptance: cargo test -p orchard-guard --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 136. Marble Run: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Readable checkpoint/next-gate/recovery feedback, trial splits and best-time deltas, smooth respawn camera; preserve all four courses and authored reload. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 151 owns those.

- Ownership: `examples/marble-run/src/`, `examples/marble-run/README.md`.
- Prerequisites: 045, 054, 055, 057, 068, 083, 084, 090, 092, 094, 091, 098.
- Acceptance: cargo test -p marble-run --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 137. Dune Skimmer: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Readable boost/heat/traction/gate cues and reliable safe recovery, with less ambiguous penalty and result presentation; preserve custom shaders. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 152 owns those.

- Ownership: `examples/dune-skimmer/src/`, `examples/dune-skimmer/README.md`.
- Prerequisites: 038, 045, 046, 054, 057, 068, 075, 083, 090, 092, 091, 098.
- Acceptance: cargo test -p dune-skimmer --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 138. Crystal Cavern: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Remapped interaction prompts, useful objective blockers/progress, stable focus highlights and clearer completion/retry feedback; preserve exploration. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 153 owns those.

- Ownership: `examples/crystal-cavern/src/`, `examples/crystal-cavern/README.md`.
- Prerequisites: 043, 054, 055, 057, 058, 070, 081, 089, 092, 098, 091.
- Acceptance: cargo test -p crystal-cavern --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 139. Siege Foundry: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Clear aim/power/ammo availability and accepted-shot/target feedback, legible narrow HUD and deliberate rebuild/results flow; preserve ballistic challenge. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 154 owns those.

- Ownership: `examples/siege-foundry/src/`, `examples/siege-foundry/README.md`.
- Prerequisites: 038, 039, 047, 054, 058, 074, 077, 087, 092, 098, 091.
- Acceptance: cargo test -p siege-foundry --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 140. Sky Courier: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Readable next-ring guidance, flight/boost feedback and checkpoint recovery cues, smooth camera and clear result summaries; preserve flight feel. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 155 owns those.

- Ownership: `examples/sky-courier/src/`, `examples/sky-courier/README.md`.
- Prerequisites: 043, 045, 054, 055, 057, 068, 074, 075, 083, 090, 092, 091, 098.
- Acceptance: cargo test -p sky-courier --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 141. Strike: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Remapping-aware control prompts, clear local pause/pointer capture and weapon/replay/network state, accepted combat feedback; never freeze remote authority for local menus. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 156 owns those.

- Ownership: `examples/strike/src/`, `examples/strike/README.md`.
- Prerequisites: 038, 039, 040, 058, 064, 065, 070, 071, 087, 092, 093, 091, 098.
- Acceptance: cargo test -p strike --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 142. 2D Physics Lab: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Useful paused single-step/reset controls, sleep/contact/query diagnostics and clear selected-body feedback; preserve demonstration scenarios. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 157 owns those.

- Ownership: `examples/physics-2d/src/`, `examples/physics-2d/README.md`.
- Prerequisites: 001, 002, 031, 032, 033, 034, 035, 036, 037, 092, 095, 098, 091.
- Acceptance: cargo test -p physics-2d --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 143. 3D Physics Lab: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Readable simulation inspector with paused stepping, contact/sleep/query counts and clear reset, plus stable camera controls; preserve scenarios. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 158 owns those.

- Ownership: `examples/physics-3d/src/`, `examples/physics-3d/README.md`.
- Prerequisites: 001, 002, 031, 032, 033, 034, 035, 036, 037, 054, 055, 092, 095, 091, 098.
- Acceptance: cargo test -p physics-3d --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 144. Courtyard: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Better orientation/interaction and objective feedback, reliable progress-preserving local menus, clear peer state and checked scene authoring; preserve custom renderer and transport. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 159 owns those.

- Ownership: `examples/owned-lab/src/`, `examples/owned-lab/README.md`.
- Prerequisites: 009, 021, 043, 044, 068, 070, 078, 079, 080, 081, 091, 098.
- Acceptance: cargo test -p owned-lab --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 145. Orbit Breaker: API migration and game feel

Rewrite this demo appropriately around the integrated new APIs, retaining its identity and working existing features. Clear charge/weapon/encounter/score feedback, readable pause/results and responsive action buffering; preserve software framebuffer and native autoplay. Adopt relevant new APIs after inspecting actual definitions; remove duplicated equivalent host logic. Preserve native/Wasm exports, save/replay formats, authored content, autoplay and screenshot flags. Add meaningful local behavioral tests. Coordinate optional presentation preferences and dynamic hints where available. Do not edit web host files: task 160 owns those.

- Ownership: `examples/owned-orbit/src/`, `examples/owned-orbit/README.md`.
- Prerequisites: 038, 039, 040, 041, 042, 058, 085, 087, 088, 089, 091, 098.
- Acceptance: cargo test -p owned-orbit --offline --locked -j 2; targeted Clippy/rustfmt; existing autoplay and reset tests.
- Status: implementation received; parent review and combined validation pending.

### 146. Emberline: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 131; do not change gameplay source or shared JS.

- Ownership: `web/emberline/index.html`, `web/tests/journey200-emberline-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-emberline-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 147. Neon Swarm: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 132; do not change gameplay source or shared JS.

- Ownership: `web/neon-swarm/index.html`, `web/tests/journey200-neon-swarm-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-neon-swarm-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 148. Pocket Rally: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 133; do not change gameplay source or shared JS.

- Ownership: `web/pocket-rally/index.html`, `web/tests/journey200-pocket-rally-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-pocket-rally-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 149. Salvage Pinball: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 134; do not change gameplay source or shared JS.

- Ownership: `web/salvage-pinball/index.html`, `web/tests/journey200-salvage-pinball-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-salvage-pinball-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 150. Orchard Guard: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 135; do not change gameplay source or shared JS.

- Ownership: `web/orchard-guard/index.html`, `web/tests/journey200-orchard-guard-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-orchard-guard-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 151. Marble Run: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 136; do not change gameplay source or shared JS.

- Ownership: `web/marble-run/index.html`, `web/tests/journey200-marble-run-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-marble-run-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 152. Dune Skimmer: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 137; do not change gameplay source or shared JS.

- Ownership: `web/dune-skimmer/index.html`, `web/tests/journey200-dune-skimmer-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-dune-skimmer-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 153. Crystal Cavern: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 138; do not change gameplay source or shared JS.

- Ownership: `web/crystal-cavern/index.html`, `web/tests/journey200-crystal-cavern-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-crystal-cavern-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 154. Siege Foundry: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 139; do not change gameplay source or shared JS.

- Ownership: `web/siege-foundry/index.html`, `web/tests/journey200-siege-foundry-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-siege-foundry-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 155. Sky Courier: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 140; do not change gameplay source or shared JS.

- Ownership: `web/sky-courier/index.html`, `web/tests/journey200-sky-courier-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-sky-courier-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 156. Strike: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 141; do not change gameplay source or shared JS.

- Ownership: `web/strike/index.html`, `web/tests/journey200-strike-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-strike-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 157. 2D Physics Lab: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 142; do not change gameplay source or shared JS.

- Ownership: `web/physics-2d/index.html`, `web/tests/journey200-physics-2d-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-physics-2d-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 158. 3D Physics Lab: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 143; do not change gameplay source or shared JS.

- Ownership: `web/physics-3d/index.html`, `web/tests/journey200-physics-3d-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-physics-3d-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 159. Courtyard: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 144; do not change gameplay source or shared JS.

- Ownership: `web/owned/index.html`, `web/tests/journey200-owned-lab-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-owned-lab-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 160. Orbit Breaker: accessible browser host

Improve this game's browser entry point with correct accessible canvas description, useful keyboard/remapping guidance, visible focus and readable narrow-screen controls/instructions. Audit title/menu/focus/escape flow; preserve all boot imports, actual Wasm ABI, player tools, lifecycle and custom renderer hooks. Avoid duplicating global player controls or claiming unsupported touch behavior. Add a focused host behavior/structure test that checks real accessibility and route contracts. Coordinate revised controls with task 145; do not change gameplay source or shared JS.

- Ownership: `web/owned-orbit/index.html`, `web/tests/journey200-owned-orbit-host.mjs`.
- Prerequisites: none.
- Acceptance: node web/tests/journey200-owned-orbit-host.mjs; preserve existing host tests; local source/route review.
- Status: implementation received; parent review and combined validation pending.

### 161. Emberline: deterministic gameplay regression scenarios

After task 131, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/emberline/tests/journey200.rs`, `examples/emberline/src/journey200_tests.rs`, `examples/emberline/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 131.
- Acceptance: cargo test -p emberline --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 162. Neon Swarm: deterministic gameplay regression scenarios

After task 132, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/neon-swarm/tests/journey200.rs`, `examples/neon-swarm/src/journey200_tests.rs`, `examples/neon-swarm/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 132.
- Acceptance: cargo test -p neon-swarm --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 163. Pocket Rally: deterministic gameplay regression scenarios

After task 133, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/pocket-rally/tests/journey200.rs`, `examples/pocket-rally/src/journey200_tests.rs`, `examples/pocket-rally/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 133.
- Acceptance: cargo test -p pocket-rally --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 164. Salvage Pinball: deterministic gameplay regression scenarios

After task 134, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/salvage-pinball/tests/journey200.rs`, `examples/salvage-pinball/src/journey200_tests.rs`, `examples/salvage-pinball/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 134.
- Acceptance: cargo test -p salvage-pinball --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 165. Orchard Guard: deterministic gameplay regression scenarios

After task 135, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/orchard-guard/tests/journey200.rs`, `examples/orchard-guard/src/journey200_tests.rs`, `examples/orchard-guard/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 135.
- Acceptance: cargo test -p orchard-guard --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 166. Marble Run: deterministic gameplay regression scenarios

After task 136, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/marble-run/tests/journey200.rs`, `examples/marble-run/src/journey200_tests.rs`, `examples/marble-run/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 136.
- Acceptance: cargo test -p marble-run --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 167. Dune Skimmer: deterministic gameplay regression scenarios

After task 137, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/dune-skimmer/tests/journey200.rs`, `examples/dune-skimmer/src/journey200_tests.rs`, `examples/dune-skimmer/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 137.
- Acceptance: cargo test -p dune-skimmer --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 168. Crystal Cavern: deterministic gameplay regression scenarios

After task 138, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/crystal-cavern/tests/journey200.rs`, `examples/crystal-cavern/src/journey200_tests.rs`, `examples/crystal-cavern/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 138.
- Acceptance: cargo test -p crystal-cavern --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 169. Siege Foundry: deterministic gameplay regression scenarios

After task 139, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/siege-foundry/tests/journey200.rs`, `examples/siege-foundry/src/journey200_tests.rs`, `examples/siege-foundry/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 139.
- Acceptance: cargo test -p siege-foundry --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 170. Sky Courier: deterministic gameplay regression scenarios

After task 140, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/sky-courier/tests/journey200.rs`, `examples/sky-courier/src/journey200_tests.rs`, `examples/sky-courier/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 140.
- Acceptance: cargo test -p sky-courier --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 171. Strike: deterministic gameplay regression scenarios

After task 141, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/strike/tests/journey200.rs`, `examples/strike/src/journey200_tests.rs`, `examples/strike/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 141.
- Acceptance: cargo test -p strike --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 172. 2D Physics Lab: deterministic gameplay regression scenarios

After task 142, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/physics-2d/tests/journey200.rs`, `examples/physics-2d/src/journey200_tests.rs`, `examples/physics-2d/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 142.
- Acceptance: cargo test -p physics-2d --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 173. 3D Physics Lab: deterministic gameplay regression scenarios

After task 143, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/physics-3d/tests/journey200.rs`, `examples/physics-3d/src/journey200_tests.rs`, `examples/physics-3d/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 143.
- Acceptance: cargo test -p physics-3d --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 174. Courtyard: deterministic gameplay regression scenarios

After task 144, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/owned-lab/tests/journey200.rs`, `examples/owned-lab/src/journey200_tests.rs`, `examples/owned-lab/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 144.
- Acceptance: cargo test -p owned-lab --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 175. Orbit Breaker: deterministic gameplay regression scenarios

After task 145, add meaningful reproducible regression scenarios for the migrated game: real start/pause/resume/restart, a success or loss path, quick input across zero/catch-up ticks where accessible, and 30/60/144 Hz-equivalent supplied elapsed time. Use public pure simulation APIs or a private test module; do not fake outcomes, expose production internals solely for tests, weaken old assertions or change game rules. Find and report production defects for parent/last polish owner. Preserve replay/network authority semantics.

- Ownership: `examples/owned-orbit/tests/journey200.rs`, `examples/owned-orbit/src/journey200_tests.rs`, `examples/owned-orbit/src/lib.rs (only test-module declaration if required)`.
- Prerequisites: 145.
- Acceptance: cargo test -p owned-orbit --offline --locked -j 2 with the added scenario tests; exact exercised paths and limits.
- Status: implementation received; parent review and combined validation pending.

### 176. Facade and API navigation refresh

Update the intentionally small facade and quickstart for the final new APIs; choose few common exports, document specialized module paths, and compile native/Wasm examples. Coordinate root registrations explicitly.

- Ownership: `crates/ferravex/src/lib.rs`, `crates/ferravex/src/prelude.rs`, `crates/ferravex/examples/quickstart.rs`, `docs/QUICKSTART.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 177. Journey200 API migration cookbook

Write verified before/after migration recipes for actual final APIs, with links to all fifteen rewritten demos and explicit compatibility boundaries. Compile the claimed examples; do not invent APIs or copy stale receipts.

- Ownership: `docs/MIGRATION_200.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 178. Coding-agent task and API map

Update agent guidance to the final implementations: bounded ownership, API discovery, deterministic examples, command outputs and failure recovery. Remove obsolete active-work instructions only after parent confirmation; preserve durable policies.

- Ownership: `AGENTS.md`, `docs/AGENT_GUIDE.md`, `CONTRIBUTING.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 179. Gameplay composition examples

Create a runnable headless recipe composing actual ability, objective, inventory/crafting and feedback-inspection APIs with accepted events and deterministic ticks. Cover a complete small scenario with assertions.

- Ownership: `crates/ferravex-play/examples/journey200_gameplay.rs`, `docs/JOURNEY_200_GAMEPLAY.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 180. Renderer and UI composition example

Create a runnable renderer example using dynamic control hints, measured layout, preference application and diagnostics with actual final APIs. Keep a useful headless geometry/assertion mode or testable pure layout.

- Ownership: `crates/ferravex-render/examples/journey200_hud.rs`, `docs/JOURNEY_200_PRESENTATION.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 181. Save and replay debugging guide

Document and demonstrate actual asset fingerprints, structured diagnostics, replay mismatch, save failure and snapshot inspection in one runnable std-only debugging example.

- Ownership: `docs/JOURNEY_200_DEBUGGING.md`, `crates/ferravex-engine/examples/journey200_diagnostics.rs`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 182. Performance evidence guide

Explain the new counters/budgets, reproducible baseline comparisons, percentile/jitter metrics and warmed-allocation checks using actual commands/results. Clearly separate measured evidence from targets or device claims.

- Ownership: `docs/JOURNEY_200_PERFORMANCE.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 183. Accessible gameplay integration guide

Document real keyboard/remapping/touch/contrast/motion/flash/pause behaviors across demos and browser hosts; include verified integration recipes and platform limits, not a compliance claim.

- Ownership: `docs/JOURNEY_200_ACCESSIBILITY.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 184. Published documentation navigation

Register final Journey200 guides and update public navigation and starter instructions to actual APIs. Preserve all existing guides/game routes/search and verify generated local links; do not deploy.

- Ownership: `scripts/build-site.py`, `website/content/getting-started.md`, `website/content/first-game.md`, `README.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 185. Scaffold-to-demo parity verification

Add executable verification of generated 2D/3D projects against final engine APIs: lifecycle, input, build paths and headless tests. Generate into unique target directories, check native/Wasm compile and report limitations; report cross-owner fixes to parent.

- Ownership: `scripts/test_template_parity.py`, `docs/JOURNEY_200_TEMPLATES.md`.
- Prerequisites: integrated engine, workflow and demo migration tasks 001–145.
- Acceptance: Compile/run every claimed Rust example, check local links and actual tool commands; report exact checks.
- Status: implementation received; parent review and combined validation pending.

### 186. Emberline: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/emberline/src/`, `examples/emberline/README.md`, `web/emberline/index.html`, `target/journey-200/visual/emberline/`.
- Prerequisites: 131, 146, 161 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 187. Neon Swarm: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/neon-swarm/src/`, `examples/neon-swarm/README.md`, `web/neon-swarm/index.html`, `target/journey-200/visual/neon-swarm/`.
- Prerequisites: 132, 147, 162 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 188. Pocket Rally: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/pocket-rally/src/`, `examples/pocket-rally/README.md`, `web/pocket-rally/index.html`, `target/journey-200/visual/pocket-rally/`.
- Prerequisites: 133, 148, 163 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 189. Salvage Pinball: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/salvage-pinball/src/`, `examples/salvage-pinball/README.md`, `web/salvage-pinball/index.html`, `target/journey-200/visual/salvage-pinball/`.
- Prerequisites: 134, 149, 164 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 190. Orchard Guard: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/orchard-guard/src/`, `examples/orchard-guard/README.md`, `web/orchard-guard/index.html`, `target/journey-200/visual/orchard-guard/`.
- Prerequisites: 135, 150, 165 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 191. Marble Run: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/marble-run/src/`, `examples/marble-run/README.md`, `web/marble-run/index.html`, `target/journey-200/visual/marble-run/`.
- Prerequisites: 136, 151, 166 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 192. Dune Skimmer: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/dune-skimmer/src/`, `examples/dune-skimmer/README.md`, `web/dune-skimmer/index.html`, `target/journey-200/visual/dune-skimmer/`.
- Prerequisites: 137, 152, 167 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 193. Crystal Cavern: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/crystal-cavern/src/`, `examples/crystal-cavern/README.md`, `web/crystal-cavern/index.html`, `target/journey-200/visual/crystal-cavern/`.
- Prerequisites: 138, 153, 168 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 194. Siege Foundry: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/siege-foundry/src/`, `examples/siege-foundry/README.md`, `web/siege-foundry/index.html`, `target/journey-200/visual/siege-foundry/`.
- Prerequisites: 139, 154, 169 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 195. Sky Courier: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/sky-courier/src/`, `examples/sky-courier/README.md`, `web/sky-courier/index.html`, `target/journey-200/visual/sky-courier/`.
- Prerequisites: 140, 155, 170 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 196. Strike: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/strike/src/`, `examples/strike/README.md`, `web/strike/index.html`, `target/journey-200/visual/strike/`.
- Prerequisites: 141, 156, 171 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 197. 2D Physics Lab: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/physics-2d/src/`, `examples/physics-2d/README.md`, `web/physics-2d/index.html`, `target/journey-200/visual/physics-2d/`.
- Prerequisites: 142, 157, 172 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 198. 3D Physics Lab: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/physics-3d/src/`, `examples/physics-3d/README.md`, `web/physics-3d/index.html`, `target/journey-200/visual/physics-3d/`.
- Prerequisites: 143, 158, 173 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 199. Courtyard: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/owned-lab/src/`, `examples/owned-lab/README.md`, `web/owned/index.html`, `target/journey-200/visual/owned-lab/`.
- Prerequisites: 144, 159, 174 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

### 200. Orbit Breaker: live playability and final visual polish

Perform a final real browser/native playability pass on this migrated demo after the parent's fresh staging gate. Inspect desktop and 390/320 narrow layouts, start/gameplay/pause/controls/retry/results, focus loss and live preferences as applicable. Fix concrete clipping, ambiguity, timing/feedback or input bugs in owned files, keeping the game's design intact. Capture evidence and preserve all prior regression checks. Use T3 preview with your own tab; never control another agent/user's tab. Source edits require rebuilt rechecks; distinguish tested artifacts from pending parent rebuild. Do not claim physical-device, audio-listening or sustained-performance validation without measuring it.

- Ownership: `examples/owned-orbit/src/`, `examples/owned-orbit/README.md`, `web/owned-orbit/index.html`, `target/journey-200/visual/owned-orbit/`.
- Prerequisites: 145, 160, 175 plus fresh-integrated-web-build.
- Acceptance: Targeted Rust/host checks for fixes; actual browser play, screenshots and console evidence; parent refresh when source changes.
- Status: implementation received; parent review and combined validation pending.

## Completion gates

- Exactly 200 original task IDs and numbered receipts; retries retain original outcomes.
- Actual code/API review and appropriate behavioral regressions, including feature/target compatibility.
- All fifteen demos use relevant integrated APIs and preserve their existing playable features.
- Workspace Rust tests/Clippy/formatting, dependency audit, native/Wasm builds, host/tool tests and fresh graphics checks.
- Generated templates, documentation examples and website navigation match checked code.
- Performance, hardware, platform, listening and campaign-play coverage is reported honestly.

Task launch/outcome records: `target/journey-200/tasks.json`. Receipts: `target/journey-200/receipts/NNN.md`. Parent results will be recorded after integration.
