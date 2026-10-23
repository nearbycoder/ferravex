# Ferravex: 50 improvements for engine ergonomics and polished demos

Completed locally on 2026-10-22. All fifty work items are implemented and integrated, with numbered receipts and parent verification. See the [results and verification report](JOURNEY_50_RESULTS.md) for evidence and platform, device and playtest limits. The descriptions below retain the original task briefs and ownership.

The priorities are a coherent Rust API, useful errors and tools for human/AI authors, and polished examples that demonstrate the APIs. All code remains first-party/std-only. Shared checkout ownership is explicit; agents must not change another task's files, commit unrelated edits, deploy, or generate additional subagents. Parent owns integration, root module registration, cross-task fixes and final validation.

## Shared session contract (frozen for concurrent implementation)

All demo migrations can use `ferravex_core::session::{GameSession, SessionState}`.

- `SessionState`: `Ready`, `Playing`, `Paused`, `Won`, `Lost`; Copy, Debug, Eq.
- `GameSession::new(hz: f64, max_steps: u32, autoplay: bool) -> Self`: Ready unless autoplay; owns a FixedClock.
- `state(&self) -> SessionState`, `is_started(&self) -> bool` (anything except Ready), `is_playing(&self) -> bool`, `is_paused(&self) -> bool`.
- `start(&mut self) -> bool`: only Ready -> Playing, true when changed.
- `restart(&mut self)`: reset fixed clock and enter Playing; `reset(&mut self)`: reset fixed clock and enter Ready.
- `set_paused(&mut self, paused: bool) -> bool`: only Playing/Paused transition; returns true when state changes. `toggle_pause(&mut self) -> bool` same rule.
- `finish(&mut self, won: bool) -> bool`: Playing/Paused -> Won/Lost; Ready or terminal is unchanged, false.
- `advance(&mut self, elapsed_seconds: f64) -> FrameStep`: advance fixed clock only in Playing; otherwise zero steps, zero alpha/dropped time, normal fixed dt. No elapsed time accumulates while inactive.
- `tick(&self) -> u64`, `dt(&self) -> f32` forward clock state.
- Existing simulation-specific Phase enums can remain authoritative for game rules; the session coordinates host start/pause/retry and fixed timing. Preserve replay/network authority: remote matches must not be frozen by a local overlay.
- API changes are additive. Existing FixedClock, renderer game macro, protocols, authored assets and host exports remain compatible. Task 03 owns implementation; root owns module registration. Demo agents need not adopt unrelated new APIs speculatively.

## Work items

### 01. Discoverable engine prelude

Create an intentionally small documented prelude and a compiling quickstart that uses the facade only. Re-export the shared session API from ferravex-core::session and common math/render/input entry points without ambiguous names. Retain existing exports. Show fixed update separated from rendering; no invented macros.

- Ownership: `crates/ferravex/src/lib.rs`, `crates/ferravex/src/prelude.rs`, `crates/ferravex/examples/quickstart.rs`.
- Validation: `cargo test -p ferravex --all-features --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 02. Structured application lifecycle

Add a renderer-independent small App/Game trait runner with startup, fixed update, render/interpolation and shutdown hooks, injected elapsed time, and explicit stop. Build on core GameSession; document error propagation and exactly-once hooks. Preserve existing async renderer games. Add meaningful headless lifecycle tests.

- Ownership: `crates/ferravex-engine/src/app.rs`, `crates/ferravex-engine/examples/app_lifecycle.rs`.
- Validation: `cargo test -p ferravex-engine --lib --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 03. Shared game session API

Implement the exact frozen GameSession contract below; this is the common API all fifteen demo agents consume. Robust pause, restart, finish, no inactive catch-up, state transitions, deterministic fixed ticks. Add extensive transition and irregular-frame tests plus a short compiling rustdoc example. Do this first and keep the agreed signatures stable.

- Ownership: `crates/ferravex-core/src/session.rs`, `crates/ferravex-core/tests/session.rs`.
- Validation: `cargo test -p ferravex-core --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 04. Fallible fixed-clock setup

Add a checked fallible clock constructor with an actionable Error/Display while keeping FixedClock::new compatible. Add useful allocation-free tick iteration to FrameStep without changing existing public struct literals. Verify 30/60/144 Hz equivalence, invalid configuration, catch-up and overflow boundaries. Do not alter module declarations or FrameStats (root owns declarations).

- Ownership: `crates/ferravex-core/src/lib.rs (FixedClock, FrameStep and their tests only)`.
- Validation: `cargo test -p ferravex-core --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 05. Typed input action handles

Add validated typed action handles/lookup to ActionMap so game code can resolve once and sample without repeated string lookup or unchecked usize IDs. Retain existing APIs and contexts. Handle stale/foreign IDs safely, document ownership and edge semantics, add behavioral tests.

- Ownership: `crates/ferravex-play/src/input/mod.rs`, `crates/ferravex-play/src/input/tests.rs`.
- Validation: `cargo test -p ferravex-play --lib input --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 06. Predictable player-input sampling

Improve PlayerInput ergonomics around once-per-frame sampling and once-per-tick edge consumption. Add a small explicit snapshot or tick-input helper using existing action APIs; fix real repeat/focus/pause hazards discovered. Keep editor, presets, storage and public behavior compatible. Regression-test quick taps, catch-up, pause/refocus and repeated sample calls.

- Ownership: `crates/ferravex-render/src/input/player.rs`, `crates/ferravex-render/src/input/player/tests.rs`.
- Validation: `cargo test -p ferravex-render --lib input --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 07. Camera discontinuity and reset API

Make follow/chase camera initialization, teleport/restart snaps and smoothing behavior explicit and easy to use. Add appropriate reset/snap helpers and tests for large/invalid dt and discontinuity, preserving authored camera feel and existing interfaces. Document an actual integration example.

- Ownership: `crates/ferravex-play/src/camera/follow.rs`, `crates/ferravex-play/src/camera/mod.rs`, `crates/ferravex-play/src/camera/tests.rs`.
- Validation: `cargo test -p ferravex-play --lib camera --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 08. Smooth audio bus controls

Add ergonomic click-free bus/master gain transitions using the existing mixer, with finite validation, cancellation and silence/mute behavior. Keep hot mixing allocation-free and existing immediate controls compatible. Test mid-ramp retargeting, sample-rate behavior and mute. No platform audio files.

- Ownership: `crates/ferravex-play/src/audio/mixer.rs`, `crates/ferravex-play/src/audio/mod.rs`.
- Validation: `cargo test -p ferravex-play --lib audio --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 09. Accessible gameplay feedback

Add a validated reduced-motion/reduced-flash feedback policy and helpers for common accepted gameplay events. Preserve gameplay outcomes and audio semantics when visual feedback is reduced. Bound IDs/events and test duplicates, restart clearing, disabled shakes/flashes and intensity. Preserve existing default feel.

- Ownership: `crates/ferravex-play/src/feedback.rs`.
- Validation: `cargo test -p ferravex-play --lib feedback --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 10. Validated retained meshes

Improve retained-rendering authoring errors: reject malformed mesh indices/non-finite vertex data or invalid transforms before GPU submission, provide actionable errors and safe checked entry points while preserving compatibility. Reuse buffers and preserve allocation-sensitive paths. Add CPU-only malformed/valid input regressions.

- Ownership: `crates/ferravex-render/src/retained.rs`, `crates/ferravex-render/tests/retained_validation.rs`.
- Validation: `cargo test -p ferravex-render --lib retained --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 11. Responsive HUD layout helpers

Add reusable viewport safe-area/anchoring and readable responsive HUD primitives to existing UI layout APIs; handle tiny/zero viewports and long labels. Keep no-frame-allocation contracts. Document actual layouts and test geometry/focus/hit regions independently of GPU. Do not edit renderer ui.rs root.

- Ownership: `crates/ferravex-render/src/ui/shared.rs`, `crates/ferravex-play/src/ui.rs`.
- Validation: `cargo test -p ferravex-play --lib ui --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 12. Actionable scene diagnostics

Expose source location, offending field/token and useful diagnostic text for scene parsing through additive APIs, retaining existing binary formats and compatibility. Test malformed values, unknown fields, Unicode and multiple-line content. Keep cooked/schema/expand files untouched.

- Ownership: `crates/ferravex-play/src/authoring/parser.rs`, `crates/ferravex-play/src/authoring/mod.rs`, `crates/ferravex-play/tests/authoring_diagnostics.rs`.
- Validation: `cargo test -p ferravex-play --test authoring_diagnostics --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 13. Typed asset retrieval

Improve Bundle/LiveBundle authoring ergonomics with checked lookup errors, normalized path handling and transactional reload/edit diagnostics. Avoid duplicating existing insert/diff/subtree functionality. Add an actual useful checked retrieval or validation API and tests preserving last-good bundles on failure, stable generation, bounds and deterministic encoding.

- Ownership: `crates/ferravex-engine/src/assets.rs`.
- Validation: `cargo test -p ferravex-engine --lib assets --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 14. Recoverable native saves

Harden native save writes and recovery using std-only atomic replacement where supported, preserving last-good content and meaningful errors. Inspect current implementation first; close concrete crash/truncation/malformed-file gaps. Tests must use unique temporary paths and clean up. Do not change save wire formats or browser storage.

- Ownership: `crates/ferravex-render/src/storage/native.rs`, `crates/ferravex-render/src/storage/tests.rs`.
- Validation: `cargo test -p ferravex-render --lib storage --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 15. Replay mismatch diagnostics

Improve replay playback/verification APIs to identify the first mismatching tick and expected/actual state or input checksum with structured diagnostics; handle seek/reset/empty/finished playback explicitly. Retain wire format and existing playback APIs. Add real divergent-playback and reset regressions.

- Ownership: `crates/ferravex-play/src/replay/mod.rs`, `crates/ferravex-play/src/replay/tests.rs`.
- Validation: `cargo test -p ferravex-play --lib replay --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 16. Safe ECS access ergonomics

Improve ECS authoring with checked multi-entity/component access or a useful query/get convenience that catches stale handles and aliasing. Inspect existing APIs to avoid duplicates. Preserve unsafe-free invariants and allocation/performance characteristics; document failure cases and test stale IDs, duplicate access and despawn/reuse.

- Ownership: `crates/ferravex-ecs/src/lib.rs`, `crates/ferravex-ecs/README.md`.
- Validation: `cargo test -p ferravex-ecs --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 17. Explainable system ordering

Add actionable named cycle/dependency diagnostics and deterministic order inspection to the existing system-order API. Keep original errors/signatures available; add checked diagnostic methods rather than breaking callers. Test cycle path, missing nodes, stable tie-breaks, disconnected systems and edge insertion rollback.

- Ownership: `crates/ferravex-engine/src/system_order.rs`.
- Validation: `cargo test -p ferravex-engine --lib system_order --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 18. Typed resource initialization

Add ergonomic typed get-or-insert/try-initialize and required-resource diagnostics to Resources. Preserve capacity and transactional failure semantics, prevent initialization when already present, and return useful errors for capacity/missing type. Add docs and behavioral tests.

- Ownership: `crates/ferravex-engine/src/resources.rs`.
- Validation: `cargo test -p ferravex-engine --lib resources --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 19. Observable event backpressure

Improve queue/event ergonomics with allocation-free drains/iterators and explicit overflow/backpressure metrics. Preserve ordering, bounded capacity and current send/read compatibility. Test zero capacity, clear/reset metrics, multi-frame consumption and each overflow mode.

- Ownership: `crates/ferravex-engine/src/events.rs`, `crates/ferravex-engine/src/frame_events.rs`.
- Validation: `cargo test -p ferravex-engine --lib --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 20. State transition ergonomics

Extend StateMachine with documented transition inspection/application helpers so enter/exit handling happens once and rejected transitions retain current state. Add actionable diagnostics or guards consistent with current API rather than duplicate GameSession. Test cancel, same-state, pending replacement and invalid transitions.

- Ownership: `crates/ferravex-engine/src/state.rs`.
- Validation: `cargo test -p ferravex-engine --lib state --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 21. Physics query filters

Replace error-prone long query arguments with an additive builder/config for ray/sphere casts (layers, ignored body, sensor policy if supported). Keep existing functions compatible and current-pose guarantees. Add 2D/3D useful query tests, invalid finite data rejection and docs. Do not edit world.rs.

- Ownership: `crates/ferravex-physics/src/api.rs`, `crates/ferravex-physics/tests/query_filters.rs`.
- Validation: `cargo test -p ferravex-physics --test query_filters --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 22. Reliable contact lifecycle

Audit contact start/stop events on body removal, sensor/layer changes, sleeping and empty/invalid steps. Fix concrete lifecycle holes and add ergonomic access only where needed. Preserve solver and bounded allocations. Test deterministic event ordering and stale-handle reuse. Do not edit api.rs or solver submodules.

- Ownership: `crates/ferravex-physics/src/world.rs`, `crates/ferravex-physics/tests/contact_lifecycle.rs`.
- Validation: `cargo test -p ferravex-physics --test contact_lifecycle --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 23. Reproducible named RNG streams

Extend seeded random ergonomics with stable named/tagged child streams and checked range helpers; existing deterministic streams and seed algorithm must remain compatible. Avoid platform hashers. Test unrelated subsystem independence, invalid/empty ranges and saved state roundtrip if added. Document version/seed behavior.

- Ownership: `crates/ferravex-engine/src/random.rs`.
- Validation: `cargo test -p ferravex-engine --lib random --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 24. Machine-readable diagnostics

Add deterministic machine-readable diagnostics export and useful error context for AI coding agents using std only. Escape arbitrary Unicode/control characters correctly; retain bounded storage/dedup semantics and stable diagnostic codes. Provide formatter/export APIs and tests that verify valid schema and counters, with no network service.

- Ownership: `crates/ferravex-engine/src/diagnostics.rs`.
- Validation: `cargo test -p ferravex-engine --lib diagnostics --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 25. Safer scene hierarchy authoring

Improve Scene with checked named lookup/hierarchy editing or reusable traversal that makes game code simpler; close transactional prefab/reparent failure gaps found. Avoid changing scene binary formats, handle stale IDs and deep hierarchies without recursive overflow, add actionable SceneError display and behavioral tests.

- Ownership: `crates/ferravex-play/src/scene.rs`.
- Validation: `cargo test -p ferravex-play --lib scene --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 26. One local developer command

Create a stdlib Python developer CLI routing list, run GAME, build-web, test and check through existing scripts/Cargo. Derive games from catalog, validate inputs, propagate exit codes, support --dry-run with JSON where useful. No shell=True, no new deps, no network/install/deploy actions. Delegate doctor/new subcommands to scripts/doctor.py and scripts/new-game.py once present; these are other agents' files. Unit-test argument routing without launching builds.

- Ownership: `scripts/ferravex.py`, `scripts/test_ferravex_cli.py`.
- Validation: `python3 -B scripts/test_ferravex_cli.py`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 27. Environment doctor

Create a read-only stdlib environment doctor for Rust/Cargo/wasm target, Python/Node, native display/graphics prerequisites and optional tools. Offer human and --json output with actionable commands, required versus optional status and correct exit code. Never install or modify user settings. Test mocked missing tools and failure handling.

- Ownership: `scripts/doctor.py`, `scripts/test_doctor.py`.
- Validation: `python3 -B scripts/test_doctor.py`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 28. Runnable project scaffolding

Create first-party 2D and 3D standalone game scaffolds using ferravex facade, new GameSession and native/browser-compatible game entry points. Stdlib generator validates name/destination, refuses overwrite, uses local path dependencies, and writes useful README/agent notes. Templates should compile against checkout, no workspace mutation or external dependencies. Test generation/path quoting/overwrite rejection.

- Ownership: `scripts/new-game.py`, `scripts/test_new_game.py`, `templates/game-2d/`, `templates/game-3d/`.
- Validation: `python3 -B scripts/test_new_game.py`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 29. AI coding-agent guide

Write concise truthful root agent instructions and an AI coding-agent guide with repo map, canonical API entry points, commands, bounded ownership, fixed timestep/input/feedback patterns and debugging recipes. Link Journey50 contracts and tool commands. Preserve dependency policy. Do not mandate approvals or deployments; don't claim unverified features. Keep AGENTS.md short enough to load each task.

- Ownership: `AGENTS.md`, `docs/AGENT_GUIDE.md`, `CONTRIBUTING.md`.
- Validation: `Review commands and paths against actual code`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 30. API migration cookbook

Write practical compiling-style before/after recipes for the new shared GameSession, facade prelude and app lifecycle plus input, feedback and reusable systems. Root docs/JOURNEY_50.md contains contract; inspect actual implementations before naming extra APIs. Explain migration for 15 demos and compatibility. No placeholder or false completion claims. Parent will run rustdoc/compile check final.

- Ownership: `docs/MIGRATION_50.md`, `docs/QUICKSTART.md`.
- Validation: `Check snippets against existing Rust APIs`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 31. Emberline migration and polish

Migrate the demo loop to the frozen GameSession API, preserving testable simulation. Improve platformer start/retry flow, jump/landing feedback and goal/checkpoint readability. Remove duplicated host pause/restart/timing state where appropriate. Add meaningful restart/pause/terminal-state simulation tests and update README with controls/APIs. Preserve autoplay and wasm exports.

- Ownership: `examples/emberline/`, `web/emberline/index.html`.
- Validation: `cargo test -p emberline --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 32. Neon Swarm migration and polish

Migrate to GameSession. Polish wave transitions, threat/readable hit feedback and consistent win/loss/retry flow. Respect actual input edge consumption and pause/refocus. Improve use of existing feedback/AI APIs rather than duplicating systems. Preserve deterministic beatable autoplay and add relevant regression tests; update README.

- Ownership: `examples/neon-swarm/`, `web/neon-swarm/index.html`.
- Validation: `cargo test -p neon-swarm --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 33. Pocket Rally migration and polish

Migrate to GameSession. Improve countdown/restart, lap/checkpoint HUD and off-track recovery feedback, preserving valid lap detection and persistence rules. Ensure pause does not consume race time and restart clears camera/feedback debt. Add behavioral tests and update README; retain autoplay.

- Ownership: `examples/pocket-rally/`, `web/pocket-rally/index.html`.
- Validation: `cargo test -p pocket-rally --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 34. Salvage Pinball migration and polish

Migrate to GameSession. Polish launch/drain/retry prompts, flipper response and scoring feedback using shared systems. Keep ball simulation fixed-step and ensure held launch cannot repeatedly trigger across catch-up ticks. Test lives/reset/end-of-game transitions, preserve autoplay, document.

- Ownership: `examples/salvage-pinball/`, `web/salvage-pinball/index.html`.
- Validation: `cargo test -p salvage-pinball --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 35. Orchard Guard migration and polish

Migrate to GameSession. Improve build/upgrade affordances, invalid-placement reasons, wave readiness and win/loss flow; use existing scene/navigation/save systems. Verify pause/restart/economy do not double-trigger or leak. Preserve authored content/autoplay, add behavioral regression tests, update README.

- Ownership: `examples/orchard-guard/`, `web/orchard-guard/index.html`.
- Validation: `cargo test -p orchard-guard --all-features --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 36. Marble Run migration and polish

Migrate to GameSession. Polish course selection, countdown/checkpoint/respawn feedback and camera reset continuity. Preserve the four courses, course completion and content reload; pause must not advance trial time. Add behavioral tests, preserve autoplay and update README.

- Ownership: `examples/marble-run/`, `web/marble-run/index.html`.
- Validation: `cargo test -p marble-run --all-features --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 37. Dune Skimmer migration and polish

Migrate to GameSession. Improve boost availability/overheat feedback, race progress/readability and restart/recovery transitions. Preserve custom shaders and fixed vehicle simulation; clear stale input/camera feedback on restart. Add behavioral tests and update README, preserve autoplay.

- Ownership: `examples/dune-skimmer/`, `web/dune-skimmer/index.html`.
- Validation: `cargo test -p dune-skimmer --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 38. Crystal Cavern migration and polish

Migrate to GameSession. Improve interactable highlighting/prompts, collection/progression feedback and gate completion/retry flow. Ensure an input edge cannot collect/interact repeatedly during catch-up; pause/refocus cleanly. Preserve shaders, add behavioral tests and README, preserve autoplay.

- Ownership: `examples/crystal-cavern/`, `web/crystal-cavern/index.html`.
- Validation: `cargo test -p crystal-cavern --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 39. Siege Foundry migration and polish

Migrate to GameSession. Improve aim/power/ammunition readability, accepted-shot and destruction feedback, and rebuild/retry/win/loss flow. Keep authoritative physics fixed-step and input edges exactly once. Test reset resources/terminal-state behavior, preserve autoplay, update README.

- Ownership: `examples/siege-foundry/`, `web/siege-foundry/index.html`.
- Validation: `cargo test -p siege-foundry --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 40. Sky Courier migration and polish

Migrate to GameSession. Improve gate navigation, delivery progress/countdown and failure/retry feedback with shared APIs. Keep flight simulation fixed-step and prevent paused timers or stale restart input. Add meaningful behavioral tests, preserve autoplay, update README.

- Ownership: `examples/sky-courier/`, `web/sky-courier/index.html`.
- Validation: `cargo test -p sky-courier --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 41. Strike migration and polish

Migrate appropriate offline/practice host lifecycle to GameSession while preserving authoritative network/replay tick semantics (do not pause a network match by accident). Improve onboarding, pointer capture/resume, crosshair/hit/readable round feedback and menu return/retry. Inspect separate modes carefully. Preserve multiplayer protocol and smoke flags; add targeted behavioral tests and update README.

- Ownership: `examples/strike/`, `web/strike/index.html`.
- Validation: `cargo test -p strike --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 42. 2D Physics Lab migration and polish

Migrate to GameSession with correct sandbox pause and manual stepping semantics. Improve drop/shape controls, active settings/status and reset feedback. Keep fixed physics stepping, bounded bodies and deterministic autoplay. Add tests for reset/pause/drop behavior and update README.

- Ownership: `examples/physics-2d/`, `web/physics-2d/index.html`.
- Validation: `cargo test -p physics-2d --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 43. 3D Physics Lab migration and polish

Migrate to GameSession. Improve orbit camera reset, drop/shape/pause controls, readable active settings and reset feedback. Preserve fixed physics and bounded scene, ensure pause/manual stepping works. Add behavior tests and README, preserve autoplay.

- Ownership: `examples/physics-3d/`, `web/physics-3d/index.html`.
- Validation: `cargo test -p physics-3d --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 44. Courtyard migration and polish

Migrate suitable interactive host lifecycle to GameSession while preserving original software/headless export and authoritative FPS/multiplayer tick semantics. Improve training onboarding/objectives and pause/resume/input capture feedback. Keep WebRTC/network formats and original renderer intact. Test host reset/pause behavior, preserve CLI/headless paths, update README.

- Ownership: `examples/owned-lab/`, `web/owned/index.html`, `web/owned/platform.js`.
- Validation: `cargo test -p owned-lab --offline --locked`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 45. Orbit Breaker migration and polish

Migrate headless/interactive loop to GameSession appropriately. Polish launch/life-loss/sector-clear/retry feedback, preserve continuous collision and original framebuffer presentation. Fix quick-tap launch/restart edge behavior with catch-up. Keep CLI/Wasm exports compatible and platform-input regression passing; update README and add behavior tests.

- Ownership: `examples/owned-orbit/`, `web/owned-orbit/index.html`, `web/owned-orbit/platform.js`.
- Validation: `cargo test -p owned-orbit --offline --locked; node examples/owned-orbit/tests/platform-input.mjs`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 46. Resilient browser loading

Improve browser Wasm loading with explicit cancellation/stage-aware errors and actionable retry without duplicate downloads/start side effects. Preserve current loadWasm signature and streaming-MIME fallback. Test timeout, non-OK response, compile/link failure, user cancellation and cleanup using first-party mocks.

- Ownership: `web/loader.js`, `web/tests/loader.mjs`.
- Validation: `node web/tests/loader.mjs`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 47. Browser suspend and recovery

Harden WebGL lifecycle scheduling and recovery for repeated context loss/restoration and stop/disposal. Add optional visibility suspension support without breaking current hosts or resuming a manually paused game; distinguish reasons. Test exactly one scheduled frame, zero stale callbacks, listener cleanup and failure recovery. Do not edit runtime.js/player.js.

- Ownership: `web/lifecycle.js`, `web/tests/lifecycle.mjs`.
- Validation: `node web/tests/lifecycle.mjs`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 48. Accessible game catalog

Improve local game catalog discovery, keyboard navigation/filtering, meaningful focus/empty states and narrow-screen readability using native HTML/CSS/JS. Keep all 15 game links and existing assets/branding. No framework/external assets, no website generated files. Add targeted DOM-independent logic checks if helpful; validate actual link targets.

- Ownership: `web/index.html`, `web/catalog.js`, `web/tests/catalog.mjs`.
- Validation: `node web/tests/catalog.mjs if added`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 49. Catalog migration and smoke audit

Create catalog/source/API migration consistency audit for all 15 demos and improve smoke tooling to report each game's build/launch/capture outcomes and clear environment limitations. Validate metadata/paths, optional GameSession migration report semantically enough not a brittle substring gate, and machine-readable summaries. Preserve existing smoke flags and screenshot validation. Test failure cases without launching a display.

- Ownership: `scripts/audit-catalog.py`, `scripts/test_catalog.py`, `scripts/smoke-games.py`, `scripts/test_smoke_games.py`.
- Validation: `python3 -B scripts/test_catalog.py; python3 -B scripts/test_smoke_games.py`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

### 50. Unified validation and CI coverage

Create one stdlib validation command with fast/full modes, grouped Rust/Python/JS/wasm/native checks, accurate skips versus failures and machine-readable report. Respect first-party/offline policy and existing CI platform coverage. Integrate newly added CLI/catalog checks appropriately. Do not claim visual/native checks passed when environment absent. Unit-test command selection/result aggregation; parent owns the final full execution.

- Ownership: `scripts/validate.py`, `scripts/test_validate.py`, `.github/workflows/ci.yml`, `docs/VALIDATION_50.md`.
- Validation: `python3 -B scripts/test_validate.py`.
- Status: implemented and integrated; targeted checks and parent verification passed within the [reported coverage](JOURNEY_50_RESULTS.md).

## Delegation status

All 50 implementation tasks were accepted by T3 Code on 2026-10-22, one child agent per numbered item, using the thread's inherited model and settings. Capacity failures for tasks 30 and 32 were retried successfully. Task 47 left a complete implementation despite a capacity failure; the parent independently validated and adopted it, preserving the original failure in the orchestration ledger.

All fifty numbered receipts are present. Parent integration registered the shared modules, resolved cross-task regressions, rebuilt all fifteen demos and completed native, Wasm and live browser checks. The full runner passed **54 commands with zero failures or skips**, including **1,132 Rust tests and doctests**; all **12 GPU tests** passed separately. The final presentation fix and documentation refresh received targeted follow-up checks. Exact scope and limits are in the [results report](JOURNEY_50_RESULTS.md).

Local task IDs and original outcomes remain in `target/journey-50/tasks.json`; implementation receipts are in `target/journey-50/receipts/`; the full machine-readable report is `target/journey-50/final-validation.json`.

## Integration gates

- Baseline and final workspace Rust tests, clippy and formatting; first-party dependency audit.
- All catalog demos build natively and for Wasm; actual-Wasm and browser-host suites pass.
- CLI, scaffolding, doctor, catalog and validation tools have truthful outputs and negative-case coverage.
- Smoke/play checks distinguish headless, real graphics and device validation; visual quality is not inferred from compilation.
- API migration docs and example controls agree with checked source. No completion claim until the relevant results are collected.
