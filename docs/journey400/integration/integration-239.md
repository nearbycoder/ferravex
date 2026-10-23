# Parent integration: accepted FPS transitions239

Captured exact five returned239 files over validated237 before dispatching240.
Source SHA-256: `92d06ec81473c69c3d8fc988c966cd1797c77fb0528e5fc8edf7f136831133a3`.
Complete inventory/commands are in integration-239/source-capture.json/report.json.
All-feature FPS tests/doctests: {'passed': 110, 'failed': 0, 'ignored': 0}. Strict all-target Clippy,
native all-feature facade and FPS/facade Wasm checks passed. All eight public
doctests execute, including actual authority/local-prediction recomputation with
a bounded caller confirmation ledger. Active relocation240 edits are excluded.

Parent verified exact old movement and weapon arithmetic after removing only
new reported flags/actual CharacterStep.jump result plumbing. Source comparisons
also preserve complete rewind-history code and policy selection/preparation,
reservation, context and Capture/World implementation (preservation.json). The
pre-edit legacy checkpoint/tape fixture and prior234/237 tests remain passing.
Existing GameEvent/RewindStep/StepCounts/StepError literals and formats remain
unchanged; inline sidecar is a private StepOutput storage extension.

Reviewed actual accepted result placement and preflight boundaries. Motor jump
acceptance is returned from the real successful motor call, including buffered/
coyote cases, without a second step or velocity inference. Invalid settings leave
all player state untouched; movement errors retain the legacy aim update while
skipping motion/timers/ammo work. Reload completion/start/fire ordering and bool
returns retain their exact calculations. Completion+fire can share one aggregate.
Dead/respawn/deferred/frozen actors emit no transition aggregate. An accepted
living action can precede later damage/death in that same tick.

Checked retained output emits at most one nonempty aggregate per actor into16
inline slots, after original capacity/state/optional-policy preflight and in the
same roster/ID order as the shared kernel. Records contain resulting executed
tick, actor and full authority life generation at computation. They are neither
historical shot claims nor globally unique feedback identities. Invalid shot
time suppresses fire while independently accepted jump/reload may still report.
No event truncation or third Vec is introduced. Rejected preflight preserves the
previous sidecar with all other output; successful ticks replace it even if empty.

Reconciliation and replay can legitimately recompute the same observation.
Confirmed feedback is caller-owned, keyed by host session/actor/full life/tick
(and kind if dispatched separately), with a retained bounded confirmation window.
Do not replay silent history as live sound or play fire from both Shot and sidecar.
Pure local detailed results are speculative, not authority/tick/session claims.
Sidecar is not serialized by old network or replay adapters; remote confirmed
reload/jump feedback would require its own coordinated versioned delivery design.

No actionable material regression was established in this scoped review. Child
zero-allocation measurements cover actual tested warmed authority steps on both
spawn paths, not arbitrary CombatRules, setup, recording, network or old wrappers.
No small-stack, throughput, cross-platform numeric, live-game/browser/device or
audio acceptance is added. Strike/Courtyard, server/client/replay consumer adoption
remains future work. Served Journey200 release is unchanged.240 must preserve
these accepted-result and retained-output seams while adding authority relocation.
