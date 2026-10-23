# Parent integration: authored camera rails288

Captured exact rail implementation/test and closed219 curve implementation/test
over validated287/291, adding only minimal curves and camera::rail registrations.
Recorded219 source hashes match current files before copying. Source SHA-256
`6f4dc708eb707075daa20268ba830a6ad4436505df478aa3186e51986fd13bdd`.
Full provenance/commands are in integration-288/source-capture.json/report.json.
This frozen graph excludes active animation289, touch294 and FPS239 changes.

All-feature math190,play1085 and renderer424 tests/doctests passed, zero failed.
14 existing renderer tests remain ignored. Strict all-target play/render Clippy,
native facade and play/render/facade Wasm checks passed. Both rail doctests execute
actual cubic tessellation->Route3->rail/player and accepted Rig3 cut handoff. The
curves module's complete/partial semantics also execute in these package checks.

Reviewed bounded64-segment/1024-time queries, checked complete tessellation before
route replacement, borrowed path lifetimes, key/lens continuity and caller-time
loop/boundary rules. Cubics sample polynomial parameter; Route3 samples measured
polyline distance. Authored orientation/FOV uses Transition3, not tangent-derived
roll or a duplicate spline/quaternion engine. Partial curve scratch can contain a
prefix, but last-good route remains unchanged; consume only successful written
prefixes. Exact endpoints/cuts bypass rounded interior reconstruction.

Supplied nonnegative absolute time permits backward seek. Once and Loop are
explicit; cut chains are right-continuous, loops report absolute cycle/phase, and
player detects skipped cuts/cycles or backward seek for presentation history.
Floating binary duration/remainder is documented with no epsilon snap; cycle
counts>=2^53 reject. Held display discards supplied time, so host must freeze/rebase
the playhead. Reduced motion selects the final key as a finished cut, independently
of simulation or flash. A rail finishing does not finish gameplay. Batch two-pass
validation preserves all output on late failure. Intermediate view targets can
still fail representability atomically, and camera collision is host-owned.

No actionable material regression was established in this scoped review. Real
optional intro/result adoption, rendered camera framing/collision, fresh browser
screenshots and device/audio/performance acceptance remain pending. No served
runtime, authored live-rig default, simulation/input format or renderer was changed.
Parent-added Skeleton getter and animation retarget registration are outside this
capture and will be validated with289; they are not claimed covered here.
