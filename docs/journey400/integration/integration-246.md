# Parent integration: edge-aware charge actions 246

Captured the two exact returned files over validated244, preserving reward244,
pity245 and progression243. Frozen source SHA-256:
5f9104fd8aede954fd4395a10f631ded54028b7d4d9cad1f0a406df9e61e0308.
All-feature math190, gameplay1207 and renderer470 tests/doctests passed;
fourteen existing renderer device/platform tests remain ignored. Strict all-target
play/render Clippy, native facade and play/render/facade Wasm checks passed.
Exact commands/inventory/logs: integration-246/{source-capture,report}.json.
No root/module/manifest registration needed; served296 release is unchanged.

The exact receipt recipe hash
07e2abb0c4812145309bc11308a6cbc1eb51d07d10cef4950d8eef3b2b97e1e9
compiled and executed against this frozen graph. Parent helper records fresh
Cargo JSON artifact paths, build/rustc/run commands and logs in charge-recipe-*.
A real ActionMap quick tap survives a zero-tick GameSession advance, resolves
once across catch-up ticks, and produces feedback only after Magazine accepts
its cost. A second empty-magazine request is rejected; an overlay consumes/masks
input while authoritative tick and magazine time continue. Six updates are both
issued and executed; no browser/memory/outcome manipulation is involved.

Reviewed actual step_edges ordering, all boolean combinations, cooldown/release
invariants and the public tests. Legacy new/step/cancel/power/charging executable
bodies are byte-identical; private state remains unchanged. Invalid time rejects
before mutation. Preexisting cooldown ages first; only its remainder is available
for held charging. An old charge closes before current held duration when release
is present or final level is unheld. Release uses only old accumulated power,
with the existing1e-7 minimum tolerance; too-short cancellation starts no cooldown.
An emitted release starts full fresh cooldown which is not aged again that call.
Final held can then start a new charge only if cooldown is zero. A final-unheld
press with no old resolution is a zero-duration tap, never inferred dt credit.
At most one resolution is emitted per aggregate snapshot. Report power is the
new final state and must not replace the old release payload.

Aggregate flags carry neither sub-tick timestamps nor transition multiplicity.
This documented deterministic attribution cannot reconstruct physical timing.
Repeated caller submission is a new request; the helper has no tick identity
ledger. Consume once per executed input tick and share the snapshot. cancel keeps
cooldown; reset clears progress/cooldown and keeps configuration. Hosts must also
reset pending input and game-owned accepted-event state. Recovery/input reports
are requests, not proof of an accepted shot, hit, launch or audible feedback.

Constructor extremes, exact/tolerance minimum boundaries, zero/large dt, cooldown
remainder, all8 flags from idle/charging/recovery, invalid atomicity and long legacy
equivalence sequences passed. Real ActionMap release/repress retains final held
and produces old release plus new charge. No actionable material regression was
established in this scoped review. Preservation JSON records legacy methods and
unchanged session/input/roots/reward/game sources. Fixed scalar state is bounded;
no allocation instrumentation/performance benchmark was run for this feature.

Source inspection confirms Orbit deliberately auto-charges while Ready, then
releases on an accepted launch request. Replacing that with physical held would
change authored serving and is not done. Pinball uses a one-tick launch request,
not this helper; later optional charge adoption must preserve default rules.
Game migrations, actual browser charged mechanics, physical devices, audio and
sustained performance remain separate work. Active shield247/stagger248 are
excluded from this immutable graph.
