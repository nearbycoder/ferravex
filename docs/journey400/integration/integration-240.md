# Parent integration: authority relocation240

Captured exact final240 files over validated239, deliberately excluding active230
network extraction. Source SHA-256:
`91f3fed654e8f14984072c1c9f53b590994de36739a1435d62d3cf5b0627e230`.
Provenance/commands: integration-240/source-capture.json/report.json.
All-feature FPS tests/doctests: {'passed': 128, 'failed': 0, 'ignored': 0}. Strict all-target Clippy,
native all-feature facade and FPS/facade Wasm checks pass. This reruns the final
arena-validation-order change that the child's concurrent-network E0422 prevented
it from verifying. Earlier successes are not substituted for final-source checks.

Parent source comparisons preserve entire spawn/combat/replay/checkpoint modules,
shared step kernel and old rewind life/capture/validate/pose/query/trace methods
(preservation.json). Added relocation does not silently change default respawns,
accepted transition identity or codecs. Existing239 fixtures and real loopback
suites, including240's small remote relocation/interpolation test, pass with this
pre-extraction network graph.230 has been steered to retain/run240's test with
its own final extraction. This report does not certify in-progress230 source.

Reviewed complete preflight, actor-only current-frame refresh, checked full-life
increment and reset/preserve semantics. Only a living existing actor can relocate;
no health/ammo/aim/timer reset, input consumption, tick advance, event emission or
retained-output rewrite occurs. Static capsule clearance must complete its budget;
optional dynamic occupancy is conservative open living-player AABBs, not exact
capsule collision or a sweep. All other player/state and arena bounds validate
before mutation. Invalid destination/options/queries preserve the whole state.

Every accepted relocation, even same-position, advances full life once. Exhaustion
rejects without clearing global history. An existing current frame replaces only
this actor's pose/life; otherwise normal bounded capture adds the frame. Old-life
poses remain stored but existing generation checks reject them; other actors'
history remains valid. Preserve explicitly retains velocity/ground/coyote/buffer/
held continuity; Reset clears them and the next real motor tick reacquires support.
Only accepted results should trigger presentation interpolation snap/camera cut.

Existing snapshots carry low32 life, so2^32 unseen transitions can alias. Adjacent
wrap differs; old same-tick freshness means a later actual snapshot tick is needed
for publication. No immediate send, packet field or Internet guarantee is added.
Old Capture does not record external relocation: continuing that segment correctly
desyncs in the explicit regression. Finish before relocation and begin a fresh
matching capture from the accepted checkpoint. Do not imply old input can replay
an unrecorded teleport or retag earlier retained feedback with a new life.

No actionable material regression was established in this scoped review. Native
warmed relocation/tick allocation tests and existing UDP path pass; no worst-case
query timing, small-stack, cross-platform determinism, physical-device, live-game,
browser/audio or sustained-performance acceptance is claimed.230/295/297 remain
separate active work; shared served runtime unchanged. Original child receipt240
retains its truthful historical final-check blocker; this parent report closes
that blocker only for the frozen graph identified above.
