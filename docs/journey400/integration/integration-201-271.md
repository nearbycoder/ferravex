# Parent integration: tasks 201 and 271

2026-10-22. Both implementations are registered and reviewed at source level.
This is a bounded API integration result, not completion of their downstream
demo migrations or the combined Journey400 build.

Task 201: inspected the range-backed context iterator, wrapping ID arithmetic,
session delegation and actual App hook dispatch. The default contextual hook
preserves old implementations; scheduled and executed accounting stay separate.
Added `TickBatch` and `TickContext` to the facade root and prelude, and migrated
the prelude's executable input example to per-tick contexts. The agent-owned
core/App implementations required no further registration or repair.

Task 271: inspected constructor validation, atomic reservations, inward rounding,
minimum content preservation, fixed storage, world fitting and camera offset.
Its types are registered under `ferravex_play::ui`. The current Canvas2D adapter
needs the explicitly documented camera offset; task 286 will own a direct
renderer adapter. No game was rewritten to this new layout API in this pass.

Parent commands, all passed:

- `cargo test -p ferravex --offline --locked -j 2`: four unit tests and two
  executable doctests, including the updated facade-only batch example.
- `cargo test -p ferravex-engine --lib app::tests --all-features --offline --locked -j 2`:
  21 tests, including contextual/legacy hooks and early termination accounting.
- `cargo check -p ferravex -p ferravex-engine -p ferravex-core --lib --all-features --target wasm32-unknown-unknown --offline --locked -j 2`:
  passed, transitively compiling the registered play layout and render dependency.
- `cargo clippy -p ferravex -p emberline --all-targets --offline --locked -j 2 -- -D warnings`:
  passed after facade additions and the separate Emberline hint correction.
- Scoped rustfmt and whitespace checks passed. The first check identified only
  export-list wrapping in prelude.rs; formatting that file resolved it.

An additional `cargo test -p ferravex-play --lib ui::layout --offline --locked -j 2`
passed all 11 selected layout tests (`271-parent-layout.log`).

Logs are `201-facade-test.log`, `201-app-parent.log`, `201-wasm-check.log` and
`parent-facade-ember-clippy.log` in this directory. Agent receipts retain deeper
native/allocator/geometry checks. No broad workspace rerun or browser build was
performed in that initial pass. Subsequently the parent compact release captured
both APIs and built all 15 demo Wasm modules; see
`../journey-200/integration-compact-release.md` for the exact immutable source and
artifact hashes. This does not establish adoption of every new API by games.
