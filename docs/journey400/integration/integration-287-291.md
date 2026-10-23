# Parent integration: camera transitions287 and typed input291

Captured exact returned implementations/tests over validated275/283. Camera287
requires NdcDepth from closed215; the snapshot adds its projection module/test,
closed217 quaternion module/test and minimal projection/camera registrations.
Earlier215/217 reports retained source-hashes.json rather than source folders;
the first parent setup probe assumed a folder and stopped before capture. Parent
then verified the actual recorded hashes before copying. Full source inventory:
integration-287-291/source-capture.json. Source SHA-256:
`33b87df145e1043852ebf7d15efacdfde3610da3c87f96ec7dc748767f4a8470`.
No active237,288 or294 changes enter this frozen graph.

## Executed checks

- All-feature math: 171 tests/doctests passed, none failed/ignored.
- All-feature play: 1,068 tests/doctests passed, none failed/ignored.
- All-feature renderer: 424 tests/doctests passed, none failed; 14 existing ignores.
- Strict all-target/all-feature play/render Clippy, native facade check and
  play/render/facade wasm32-unknown-unknown checks passed.
- Exact291 input/GameSession recipe compiled and ran against this graph using
  fresh Cargo-reported artifacts (input-recipe-report.json). Its source hash
  matches the returned recipe. Camera287's two executable examples ran in the
  complete package doctests, including real Rig3 snap/cut after accepted teleport.

## Parent review

Reviewed291 count/output/handle preflight, copy-before-clear implementation,
duplicate behavior, relative delta/held semantics and documented lifecycle/reset
handoff. Both outputs and pending actions remain unchanged on a late invalid
handle; duplicates get the same snapshot and idempotent clearing. InputError is
unchanged; the new error is separate to preserve exhaustive legacy matches.
Checked exact method-source equality for legacy feed/feed_delta/peek_handle,
consume_tick/consume_tick_handle, suppress_held and lose_focus against the prior
validated snapshot (input-preservation.json). A parent inspection probe used the
wrong name focus_lost and asserted before comparison; it was corrected to the
actual lose_focus method without editing implementation or test assertions.

Reviewed287 prepared RH camera basis/matrix decomposition, quaternion canonical
sign and checked shortest-arc helpers, displayed-pose retarget, hold/resume,
projection compatibility, sample-on-copy atomicity and generation resets. Eye,
positive view distance and FOV interpolate; interior up is orthonormal while exact
endpoints retain authored values. No fabricated collision diagnostics are returned.
Lens descriptors declare intent; they do not validate actual renderer matrices.
Valid endpoints can still yield an unrepresentable interior target and explicit
NumericalLimit, with no state/time advance. Host must choose a cut/reflow policy,
not retry identical input indefinitely. Presentation deltas remain separate from
simulation ticks and suspended wall time. A safe endpoint does not prove a safe
intermediate camera path or velocity continuity.

No actionable material regression was established in this scoped review. The
input recipe explicitly tests issued/executed differences and does not claim a
game win. Warmed allocation observations apply to selected native paths only.
Camera288 and real renderer/game bridges remain follow-up consumers. No served
runtime rebuild, browser screenshot, game rewrite, device/audio/performance or
cross-platform numeric determinism claim is added. No global checkout/workspace
all-green claim follows from this frozen source check.
