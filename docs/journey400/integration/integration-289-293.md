# Parent integration: skeleton remapping289 and gamepad calibration293

Captured exact four returned implementation/test files over validated294, with
parent's read-only Skeleton::joints getter and minimal animation::retarget and
input::calibration registrations. Parent verified the getter is the sole scene
change and existing affine216 dependency is unchanged. Source SHA-256:
`545f7a469a2a305ca3ce8a81604986114c0ab70b270ec5600f2c31f71735d92e`.
Full provenance/commands: integration-289-293/source-capture.json/report.json.

All-feature math/play/render tests/doctests pass:
{'math': {'passed': 190, 'failed': 0, 'ignored': 0}, 'play': {'passed': 1120, 'failed': 0, 'ignored': 0}, 'render': {'passed': 438, 'failed': 0, 'ignored': 14}}. Strict all-target play/render Clippy, native facade and
play/render/facade Wasm checks passed. The actual two-skeleton animation/world/skin
recipe and actual calibrated host journal/PlayerInput/GameSession recipe execute
in the complete package doctests. No hardware or rendered game claim follows.
Parent verified old animation pose/playback, Linux reader, platform journal and
PlayerInput sources byte-identical to prior snapshot (preservation.json).

## Skeleton review

Reviewed prepared real-skeleton identity/lifetimes, parallel metadata validation,
name/hierarchy matching, local right-delta offsets and staged output. Strict name
sets or explicit unmatched-target rest fallback are distinct. Immediate parents
must match; sibling reordering is accepted, implicit reparenting is not. Existing
scene::Transform, checked matrix/quaternion and PreparedAffine3D own math; supported
scale is exactly uniform and positive. No second skeleton, lossy decomposition,
IK/root-motion or format change is introduced. Parent getter exposes existing
validated joint data by immutable borrow only.

Construction owns normalized rest copies/indexed offsets and can allocate. Pose
mapping stages all normalized source and target locals before copying output,
preserving every output bit/tail on error. No hot name lookup/stateful reset.
Shared skeleton borrows bind identity, but arbitrary same-length mislabeled raw
pose arrays cannot be detected. Local rest metadata and inverse-bind relationship
remain caller-authored; existing world/skin APIs have their own later overflow
and scratch-on-error contracts. Construction/name work and stack scratch are
bounded; native zero-allocation mapping does not prove small-stack suitability.

## Calibration review

Reviewed complete profile validation, asymmetric signed half-ranges, f64 radial
remap/clamping and full connected sample validation before host feeds. Selected
profile is connection-local, explicit per-slot producer state, with retained raw
sample so replacement/reset reapply raw data without compounding calibration.
Nonfinite connected axis/button rejects the whole submission; disconnect ignores
stale invalid values and neutralizes safely. Changed supplied epoch triggers the
existing disconnect/reconnect boundary and held suppression; it is not a stable
hardware identity or the journal's generated epoch.

Pure calibration copies buttons unchanged; normal host boundary clamps as before.
No corrected axis fabricates a trigger/hat/button. Default mapping and old polling
are unchanged. One host-thread producer must own the slot, apply calibration once
and choose any separate PlayerInput response curve/deadzone explicitly. Selection
while disconnected rejects; new connections discard prior selection. There is no
global exclusive-slot registry, thread-affinity enforcement or drop-time I/O.

No actionable material regression was established in this scoped review. The
portable adapter is wired through actual gamepad_sample for embedding hosts;
Linux automatic polling and browser exports do not select it automatically.
Game adoption/retargeted rendering, browser ABI consumers, physical controllers,
other OS execution, audio and sustained performance remain unverified. Active
FPS230/240 and preferences295 are excluded. Served Journey200 release unchanged.
