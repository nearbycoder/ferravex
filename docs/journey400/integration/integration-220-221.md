# Journey400 220–221 parent integration

The parent captured the exact returned physics sources before dependent222 started editing. `integration-220-221/source-capture.json` records the full source inventory and six implementation/test fingerprints. Checks ran inside that frozen source tree, preserving active ownership in the shared checkout.

Reviewed direction-free signed query witnesses/filtering/bounded output and reset prevalidation, explicit motion policies, revision invalidation and existing wake/cache refresh behavior. No additional root registration was required: both dimensional modules expose the methods and the existing facade exports physics. Body handles retain the documented same-world requirement. Reset does not promise a collision-safe destination or immediate replacement of last-step contact snapshots.

Parent checks passed, commands and logs in `integration-220-221/report.json`:

- 291 physics unit/integration/doctests, including 22 overlap-query regressions, 30 reset regressions, old contact/compound/joint tests and all 44 doctests. Zero failures or ignored tests.
- Separate dim2 and dim3 library plus both numbered regression suites.
- Strict physics all-target/all-feature Clippy.
- All-feature physics/facade Wasm compilation and separate dim2/dim3 Wasm library compilation.

The separate actual facade composition recipe queries a single wall, applies its signed displacement under an explicit caller policy, commits a checked body reset, and calls existing202 `InterpolationPair::snap` only after success. It then rejects an invalid reset and preserves both body and presentation state. Its fresh native Cargo artifact, compile/run commands and source hash are recorded in `integration-220-221/recipe-report.json`; runnable source is `pose-reset.rs`. This is a single-wall example, not a general placement solver.

Authored game reset adoption remains separately owned. No source registrations, dependency changes, simulation defaults, browser artifacts or game physics were changed by parent integration. Native/Wasm correctness does not establish browser play, performance or physical-device behavior; broader Journey400 integration remains pending.

The first standalone recipe compile omitted Cargo's `debug/deps` search directory and could not resolve the facade's transitive crates. The parent runner now supplies that directory from its explicit target root and the recipe passes. The initial diagnostic remains in `recipe-compile-first.log`; this was a runner setup failure, not a production physics change.
