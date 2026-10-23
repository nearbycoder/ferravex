# Journey400 219 parent integration

Reviewed registered CubicBezier3, finite sample/tangent policy, interval-based
rounded-chord bounds and fixed depth-first tessellation stack. Invalid options
preserve output; valid saturated runs expose only an accepted prefix. The error
bound and covered domain do not certify the unprocessed remainder. No production
repair or extra registration was needed; no navigation subsystem was duplicated.

Parent checks passed, exact commands/logs in `integration-219/report.json`:

- All 161 current math unit/integration tests, including 18 curve regressions
  and allocation/buffer/work-bound cases.
- All 51 math doctests, including the native curve recipe.
- Strict scoped math Clippy and math/graphics all-feature Wasm compilation.
- Compiled and executed the actual cross-crate Route3 recipe from the receipt;
  one binary passed, `integration-219/route-recipe/report.json`. It checks complete
  tessellation, installs only the reported prefix into existing Route3, then
  samples start, arc-length halfway and completed endpoint.
- Owned formatting and API-map link validation.

Fingerprints: `integration-219/source-hashes.json`. Only Complete certifies the
whole curve under the documented conservative rounding-aware bound. Caller
budgets bound work/output, and callers decide whether a partial preview is useful.
No arc-length inversion, automatic road generation or collision/gate guarantee.
Existing authored Dune/Sky routes and game behavior remain unchanged; their
adoption is optional and owned separately. No fresh game/browser/GPU/physical
validation or whole-journey acceptance is inferred from these native/Wasm checks.
