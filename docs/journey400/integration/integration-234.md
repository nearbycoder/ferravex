# Parent integration: opt-in safe FPS spawning234

Captured the four returned234 files plus closed250 spawn assessments/test over
validated233, retaining its closed physics/network graph. No active camera287,
input291 or subsequent reusable-output237 edits enter this snapshot. Exact
sources, hashes and commands are in integration-234/source-capture.json and
report.json. Source SHA-256: `52d43a83668b8b804ad5b1ed8b612e1ab6a7f0769599adb52e31e7f3e550226b`.

## Validation

All-feature FPS tests/doctests: 78 passed, none failed or ignored. Strict
all-target Clippy with warnings denied, native all-feature facade check and
FPS/facade wasm32-unknown-unknown checks passed. Two public spawn recipes execute
real joins/steps and policy-bound replay/restart through existing timeline APIs.
No runtime browser/game claim follows from Wasm compilation.

Parent compared exact lib.rs/replay.rs test-module tails against validated233;
both are byte-identical (preservation.json). Reviewed narrow kernel extraction
and shared replay validation helpers through retained match-kernel.diff and
replay-seam.diff. The pre-edit legacy checkpoint/tape fixture passes unchanged,
including the encoded tape hash; None still calls the legacy stepping path.

## Review and boundaries

Policy is immutable/stateless, uses only authored positions and stable source
index ties, and returns no selection rather than fabricating origin/ID-modulo
fallback. The checked path prevalidates arena, roster, commands and complete
clearance/LOS budgets before tick or mutable rules hooks. Actual character
clearance remains full 3D; nearest-player distance is explicitly XZ, all other
living actors are hostile references, and occupancy is conservative open AABBs.
Exposure uses actual arena eye-to-eye rays plus implicit ground. Geometry cannot
predict movement into a reserved volume later in the same tick.

Accepted due respawns reserve in ascending ID order from tick-entry positions.
Deferred actors remain dead at countdown zero, with no respawn event or new life.
Frozen, auto-respawn-disabled and invalid-weapon gates remain in the shared
kernel. The explicit policy path has defined command/roster bounds/order;
legacy raw/public mutation behavior is not silently tightened. External
CombatRules callback side effects and panics remain outside rollback guarantees.

A distinct policy replay schema/version binds every configuration field and the
existing arena context. Wrong policy/context rejects before restore/step. The
old replay schema, checkpoint, event/input codecs and v5 protocol are preserved;
only validated shared decoding/comparison and recorder construction were
extracted. Arbitrary custom CombatRules are not captured by this PracticeRules
adapter. Well-formed but differing events retain existing post-step replay fault
behavior; this is not a transactional simulation rewind. Caller must record the
complete actual policy tick through the policy Capture adapter.

No actionable material regression was established in this scoped review. The
measured zero-allocation cases cover selection/preflight/context, not the current
allocating event output, initial joins or checkpoint restoration. Stack usage on
small targets and cross-platform numeric determinism remain unmeasured.237 owns
retained output next and has the narrow spawn wrapper grant documented in the
orchestration ledger.230/231 server/client and game consumers remain future
adoption. No shared served release, browser evidence or platform status changed.
