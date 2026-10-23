# Parent integration: checked canvas adapter

Read task 286's complete production implementation and receipt. Checked world
size preservation, invalid-input ordering, subpixel suspension, inward rounding,
asymmetric camera translation, legacy centered fits and half-open pointer mapping.
No additional material defect was established. Registration already exists.

Updated only stale camera prose in `crates/ferravex-play/src/ui/layout.rs`:
`Canvas2D::from_fit(...).camera()` now supplies translation automatically, and
`pointer_half_open` is available alongside the existing Viewport mapping.
Manual camera adapters can still use `CanvasFit::camera_offset`; no play behavior
or public signature changed in this parent correction.

Parent checks passed, offline/locked with two jobs:

- `cargo check -p ferravex-render --lib --all-features --target wasm32-unknown-unknown --offline --locked -j 2`
  (`286-parent-wasm.log`).
- `cargo test -p ferravex-render --test journey400_286 --all-features --offline --locked -j 2`:
  all ten regressions passed (`286-parent-tests.log`).
- `cargo test -p ferravex-play --doc ui::layout --offline --locked -j 2`:
  all four selected doctests passed (`271-parent-docs-updated.log`).
- Scoped layout-file rustfmt and whitespace checks passed.

The agent independently executed its renderer doctests and composition recipe;
their evidence remains in receipt 286. This implementation postdates compact
release manifest de25ddf985bb9e8166d0990076274d11f83ff67a00486e4f14a7d8cf8073b734.
It has not been rebuilt into that served release or visually adopted by games.
Later presentation owners must use the actual checked API and recheck native/
browser drawing and input. Task 272 owns the next bounded wrapping-layout helper.
