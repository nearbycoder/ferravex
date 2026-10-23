# Journey400 216 parent integration

Reviewed registered PreparedAffine3D, checked linear inverse reuse, reflection
sign, wide point/vector arithmetic and two-pass atomic caller-buffer operations.
No material defect established or production repair required. New module
registration is present; no extra exports/dependencies are needed.

Parent checks pass, exact commands/logs in `integration-216/report.json`:

- Seventeen affine behavior/allocation tests.
- Fifty-six existing vector, checked matrix and geometry regressions.
- Complete executable affine mesh-preparation rustdoc recipe.
- Strict math Clippy for library and task tests.
- Twelve existing graphics render tests and two prepared-projection adapter tests.
- Math, graphics and renderer all-feature Wasm compilation.
- Scoped formatting/whitespace and updated API-map link validation.

Fingerprints: `integration-216/source-hashes.json`. The recipe prepares a mirrored
sheared triangle, transforms points/directions/normals into caller storage,
checks perpendicularity/orientation, retains state after invalid edits and resets.
No new hierarchy or second inverse implementation was introduced.

Renderer adoption stays separate. Existing software raster accepts near-affine
rows within 1e-5, uses full inverse failure to skip instances and zero-normal
fallback. Immediate lighting/AO has its own identity/zero fallback. PreparedAffine3D
intentionally requires the exact affine row, rejects degenerate normals, and can
accept a forward transform whose irrelevant inverse translation overflows. A
blind replacement would alter those existing renderer acceptance policies. The
agent-guide row and receipt identify this boundary for explicit authoring or
later renderer consumers; no renderer arithmetic was silently replaced.

Native/Wasm composition and the actual runnable authoring recipe are checked.
No fresh game adoption, browser/GPU/audio/physical-device/performance or whole
journey integration claim. Current accepted Courtyard release excludes these
concurrent math additions.
