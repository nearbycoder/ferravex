# Journey400 214 parent integration

Reviewed selected-component capture, sealed tuple implementations, fixed-array
old/new mapping and prepared spawn reuse. All selected read guards are acquired
before cloning, source identity/duplicate validation precedes reads, and capacity
errors retain the complete captured plan. Embedded handles require explicit host
remapping; capture is an owned snapshot, including ordinary Clone shared-interior
semantics. No production repair or new registration was needed.

Parent checks passed, offline/locked with two Cargo jobs:

- All 90 ECS unit tests, 15 runnable doctests and six compile-fail doctests.
- Strict all-target/all-feature ECS Clippy.
- Seventeen deferred ECS command integration tests plus five inline tests against
  the updated ECS implementation.
- Native facade and ECS/engine/facade all-feature Wasm compilation.
- Scoped formatting/whitespace and updated agent-guide links.

Exact commands and logs: `integration-214/report.json`. Final ECS source SHA256
`d7a1c2f3d58b16794ebb8aeb5db28e4a017300ba21b56cb804ab867c678a2588`.
Dependency/source fingerprints are in `integration-214/source-hashes.json`.

The executed actor-template rustdoc provides capture/spawn, explicit partner
remapping, omitted transient audio ownership and reset/recapture behavior. No
existing demo currently requires automatic duplication; adoption stays at an
explicit paused editor/actor-template boundary. No whole-World clone, implicit
reference remap, panic rollback, browser/playtest or whole-journey integration
claim. Current accepted web artifacts remain independent of Journey400 changes.
