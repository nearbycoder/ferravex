# Parent integration: combo scoring receipts 242

Captured exact returned combo.rs over validated296-joined, with no registration
changes. Source SHA-256:
`4d4fe68216e526aa54fcba63b5b44c8f8f484209262dc5f264774336e770c57d`.
All-feature math190, play1153, renderer470 tests/doctests pass; fourteen existing
renderer platform/device tests remain ignored. Strict all-target play/render
Clippy, native facade and play/render/facade Wasm checks pass. Provenance and
commands: integration-242/source-capture.json/report.json. The exact receipt
recipe (hash9c24f73ede2c652f19eecba8644a49c5f3465b237b11579e7b9cd382cb189f66)
was compiled and executed against this frozen graph; fresh Cargo JSON artifact
paths and rustc/run commands are retained as combo-recipe-*.

Reviewed zero-base no-op, post-increment uncapped tier versus capped multiplier,
explicit overflow flags, actual helper-total delta, rejection before mutation,
expiry precision and reset scopes. Legacy award still returns nominal points
and saturates totals, including when the actual increase is zero. try_award
rejects either counter overflow atomically; it cannot undo an action accepted
elsewhere. Exact-fit maxima are not falsely flagged as lost increments.
Multiplication fits u64 under the unchanged u32 base/100 multiplier bounds.
Expiry remains the inclusive1e-9 internal threshold and reports removed chain
once. Preview/read-only lifetime do not refresh or accept an action.

The actual recipe combines GameSession/ActionMap/Magazine acceptance with score
receipts; rejected empty-magazine shots and skipped/paused ticks grant nothing.
Restart suppresses held input and chooses Run reset. This is accepted-shot
headless scoring evidence, not a hit/win/playtest. Parent preservation checks
confirm session/input/module roots and actual Orbit/Pinball/Neon game sources
byte-identical to the base; no game formulas were silently migrated.

No actionable material regression established in this scoped review. Orbit has
its own separate u32 score, so u64 helper total_change must not be misrepresented
as that counter's increase. Pinball currently has flat accepted-contact scoring
and no ScoreChain; introducing a chain mode is separate authored work. Consumers
must preserve accepted-event gates, deduplication, gains and reset choices.
Game adoption, browser/rendered results, hardware/audio and sustained performance
remain pending. Served296 artifacts were not rebuilt for this library change.
Concurrent client231, progression243 and reward244 are excluded from the capture.
