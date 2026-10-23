# Journey400 215 parent integration

Returned math module and its root registration inspected. Parent added the
additive graphics `Camera::prepare_projector(self, Viewport, ScreenYAxis,
FarPlane) -> Result<PreparedProjector3D, ProjectionError>` adapter in
`crates/ferravex-graphics/src/camera.rs`, reusing the new math API with the
camera's actual `ClipDepth`. No math-to-graphics dependency, matrix duplication,
root reexports, feature/dependency changes or camera-default changes.

The executable graphics rustdoc demonstrates an offset logical viewport, screen
anchor and near-plane picking. Parent behavioral tests cover both graphics clip
depths producing the same world-space ray, offset projection/unprojection,
explicit infinite-far rejection, invalid viewport, retained camera and upward Y.

All checks passed, exact commands in `integration-215/report.json`:

- Fourteen new math integration/allocation tests and fourteen current library
  regressions.
- Three math projection doctests.
- Two new graphics adapter tests and twelve existing software rendering tests.
- One graphics camera doctest.
- Strict all-target graphics Clippy and all-feature math/graphics Wasm compilation.
- Scoped owned-file formatting/whitespace and agent-guide link validation.

Source hashes are recorded in `integration-215/source-hashes.json`. Agent guide
navigation points to the production API and its convention/near-plane limits.
Other authoring, scene and game consumers remain unadopted. This validates native
and Wasm compilation and headless geometry; no GPU/browser/physical-device test,
new release build or whole-journey integration claim follows. The Courtyard
release snapshot intentionally excludes this concurrent Journey400 change.
