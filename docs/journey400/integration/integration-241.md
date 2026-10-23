# Parent integration: expiring command outcomes 241

Captured the two exact returned source/test files over validated297. No root or
manifest registration required. Source SHA-256:
`16c7a6a617d8b6efd492157cd9ee71d60cb00031086a1d97877ea6ef59c6c51e`.
Commands/hashes: integration-241/source-capture.json/report.json.
All-feature math190, play1130 and renderer458 tests/doctests pass, with fourteen
existing renderer platform/device tests ignored. Strict all-target play/render
Clippy, native facade and play/render/facade Wasm checks pass. The exact receipt
recipe was compiled and executed against this frozen graph using Cargo JSON
artifact paths; its checked source hash is
`241124e5ea56d2d8a53da9f2817b8bf638110f2b9619fe74f8bb320c49e1da12`.
Build/compile/run commands, output and hashes are retained under integration-241
as buffer-recipe-*. The native recipe reports six issued ticks, four executed
updates and one gated command handoff across two runs.

Reviewed the old one-slot buffer against the new checked implementation: legacy
wrappers share the same storage transitions, positive configured lifetime and
finite dt bounds, zero-point-independent payload behavior and exact inclusive
remaining <=1e-9 expiry. Remaining-life reads and closed gates are nonconsuming.
Expiration/replacement/execution return the exact Copy payload once, with no
comparison/Debug bound or callbacks. Zero-delta advancement can expire a newly
pushed sub-epsilon lifetime, as before; an immediate take can precede expiry.
Invalid dt validation occurs before every state mutation, even when empty.

The retry found existing partial implementation despite the prior model-capacity
failure, preserved/reviewed it, added the opaque Copy-only/NaN-payload public
regression and reran all scoped evidence. This parent capture records the actual
final files rather than assuming the failed attempt wrote nothing. Gameplay
module registration/LoopError, session, ActionMap, PlayerInput, preferences and
audio batch sources remain identical to the base (preservation.json).

No actionable material regression established in this scoped review. The recipe
uses real GameSession tick contexts and ActionMap edges, with a supplied authored
readiness gate; it is not physical landing/jump gameplay evidence. Executed(T)
means buffer removal for caller execution, not acceptance by a motor or network
authority. Emit feedback only after those real rules accept. Clear pending work
explicitly on reset; held levels must not be pushed every update. Local overlays
must not stop remote authority/replay progression. Game adoption, rendered/input
hardware/browser execution, listening and sustained performance remain pending.
Concurrent client231, semantic status296 and combo242 are excluded. Served295
bridge artifacts were not rebuilt or replaced by this library integration.
