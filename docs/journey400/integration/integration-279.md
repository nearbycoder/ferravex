# Journey400 279 parent integration

Captured the returned shared UI source and external regression over the verified Controls/Orbit release source before280 began. Exact contents are recorded in `integration-279/source-capture.json`; ongoing retained-instance and physics edits were excluded. Both public APIs are registered through existing `ui` reexports; no root registration change was needed.

Reviewed press-origin ownership, canceled/removed/disabled gesture tails, accepted confirmation reporting and conservative per-frame aggregation. Existing PlayerInput global-confirm consumption remains distinct from unrelated gameplay edges. The result is observational; consumers still classify fresh scene requests after finishing UI and queue commands until actual ticks. It is not a general action-journal filter or secondary-pointer router.

Parent frozen-source checks passed, exact commands/logs in `integration-279/report.json`:

- Renderer all-feature tests/doctests:323 passed, zero failures,14 existing platform-dependent tests ignored.
- New interaction public API/allocation regression, existing UI/Controls/wrapped-text regressions and both executable public doctests, including the actual zero-tick Buy/Drop tick-queue recipe.
- Strict renderer all-target/all-feature Clippy.
- All-feature native facade compilation and renderer/facade Wasm compilation.

No game input adapter, renderer host ABI or browser artifact was changed by this integration. Orchard/Lab adoption must follow the documented ordering and caller queue-full policy, preserving accepted-event feedback and authoritative progression. No browser/native-window/device, visual, listening/audio or sustained-performance claim follows from native/headless allocation checks. Read-only semantic inspection280 is the next separate task; transaction281 must preserve ownership/modal state during rollback.
