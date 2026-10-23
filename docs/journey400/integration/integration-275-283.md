# Parent combined review: meters275 and minimap283

Captured the exact five returned production/test files from receipts275/283 over
validated integration-250. Added only missing meters/minimap module registrations
in the derived snapshot. Full provenance and commands are in
`integration-275-283/source-capture.json` and `report.json`.

Source SHA-256: `036fb83387f2cd9a6d6162e068363f789d6cf2f47a6c2b203e234f4fca1eb744`.
The initial parent capture asserted on a nonexistent notices registration before
compilation; it is retained at `integration-275-283-initial-capture`. The corrected
capture anchors on the actual flow declaration. No implementation failure was
hidden or test assertion relaxed.

## Executed combined checks

- All-feature ferravex-play: 1,030 unit/integration tests and doctests passed,
  zero failed or ignored.
- All-feature ferravex-render: 424 tests/doctests passed, zero failed,
  14 existing tests ignored.
- Both crates, all targets/features: Clippy with warnings denied passed.
- Native all-feature facade check passed.
- All-feature play/render/facade wasm32-unknown-unknown checks passed.
- Exact returned meter recipe compiled and executed against Cargo-reported
  artifacts from this snapshot. Source hash matches receipt275; commands,
  artifact paths and exits are in meter-recipe-report.json. It exercises accepted
  Magazine reload progress/ammo and actual GameSession tick/pause/step behavior.
- Minimap executable module recipe ran in the package doctests. Its supplied
  room IDs and omitted unknown location are example data, not Crystal adoption.

## Review and practical boundaries

Reviewed pure meter constructors, rectangle/threshold/cell validation, renderer
layout/submission/paint transaction paths and actual paint tests. Continuous
values reject invalid ranges rather than clamp; count geometry is bounded to 64
cells and thresholds to 16. Renderer text remains authored and complete, uses
explicit theme tokens and rejects insufficient geometry or budgets. Known zero,
Empty and Unknown are distinct. Caller formatting, units, gameplay availability,
outer clips and safe-area reservation remain outside the composer. Measured
warmed allocation results apply only to the tested paths. Tiny positive fills
can reject if their required cue geometry is unrepresentable.

Reviewed rotated aspect fitting, world membership, center-ray edge intersection,
projection/inverse conventions, f64 intermediates/f32 representability, two-pass
atomic batch writes and transactional configuration. Maximum batch is 4,096;
output tails remain unchanged. World projection edges are closed, while map/clip
pointer edges are half-open. Rounded inverse uses the documented display-cell
fringe; this is approximate display geometry, not exact geometric predicates.
Positive rotation is clockwise in screen coordinates after the supplied world-Y
convention. No nearest-point shortcut, invented marker, input capture or hidden
world/discovery state is added. Games must authorize supplied/picked locations.

No actionable material regression was established in this scoped parent review.
The snapshot excludes active FPS234, camera287 and input291 changes. No workspace
or current-checkout blanket-green claim follows. Actual Dune/Sky/Siege/Crystal
adoption, fresh native/Wasm rendering, screenshots, physical-device, audio and
performance acceptance remain pending. The served Journey200 release is unchanged.
