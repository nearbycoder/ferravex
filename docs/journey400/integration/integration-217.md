# Journey400 217 parent integration

Reviewed quaternion decomposition/canonical half-turn sign, normalized local-axis
correction and ordinary nonwrapped angle limits. Existing J200027 normalization
arithmetic is retained. The explicit exact singular flag and near-half-turn
conditioning limits remain documented; no hidden epsilon snap or continuity
history is inferred. No production repair or registration needed.

Parent checks pass, exact commands in `integration-217/report.json`:

- Fourteen new swing/twist tests plus 24 existing angular/geometry regressions.
- Five executable quaternion doctests, including the actual aim/roll recipe.
- Strict scoped math Clippy.
- Thirty-one current prepared projection/affine regressions.
- Math/graphics all-feature Wasm compilation.
- Owned formatting/whitespace and API-map links.

Fingerprints: `integration-217/source-hashes.json`. The recipe preserves an
existing aim direction while limiting local roll. Exact recovered angle and
idempotence become ill-conditioned near a half-turn swing; applications needing
continuity must choose their own pose/history policy. No weapon, physics joint,
online behavior or game defaults were changed. Native/Wasm math composition is
checked; no game/browser/physical-device/performance or whole-journey claim.
