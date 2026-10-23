# Parent integration: shield state and safe resizing 247

Captured exact source/test over validated246. Frozen source SHA-256:
17b071a6e99af163340580c84c91795ad831d81682b182d415c64388087dcce0.
All-feature math190, gameplay1219 and renderer470 tests/doctests passed;
fourteen existing renderer platform/device tests remain ignored. Strict all-target
play/render Clippy, native facade and play/render/facade Wasm checks passed.
Exact commands, logs and inventory: integration-247/{source-capture,report}.json.
No root/module/manifest registration needed. Served296 release is unchanged.

The exact receipt recipe hash
e2453a7018a52246543d579bf7b607d6d5498bf2966c9f7a8fa3b7bd0af9f6a0
compiled and executed against this frozen graph using fresh Cargo JSON artifacts.
shield-recipe-* retains all build/rustc/run commands and logs. Real GameSession,
ActionMap, ActionTimeline and geometric melee-range checks feed HitWindow serial
admission; repeated active overlap cannot repeat damage or reset shield delay.
One accepted25-damage hit depletes20 shield and costs5 health; feedback is counted
only after admission. Four updates are issued and executed after a zero-tick frame.
This is supplied-time/headless input and geometry evidence, not a browser fight.

Reviewed validation before mutation, independent state axes, f64 delay accounting,
legacy arithmetic, detailed endpoint receipts and both reconfiguration policies.
Old field list and Copy remain; Rust layout/ABI is not newly guaranteed. Wrapper
bodies now use detailed methods. A frozen independent original-model regression
compares damage/value/timer bits over3000 pairs, including signed zero, subnormals,
zero maximum/rate and capped steps. Existing Shield test is preserved.

Zero maximum is full AND broken. Broken may coexist with delayed or recharging;
recharging means enabled/below-full/no-delay eligibility, not guaranteed stored
gain. Disabled recharge still ages delay. Zero damage preserves delay; positive
confirmed damage resets it even when empty. Absorbed/unabsorbed use original f32
routing; nominal absorbed damage can round away in stored charge. Actual value
change uses promoted stored endpoints, which remain available separately.
Recovery/depletion depend on actual zero-crossing; fill requires an actual stored
increase to full. Delay expiry/zero dt/sub-ULP/full no-ops cannot repeat cues.
Receipts are per-operation facts, not a persistent event-deduplication ledger.

Reconfiguration validates all settings through the original constructor before
assignment. PreserveValue clamps shrinking capacity and grants no charge on growth.
PreserveRatio deliberately scales charge with f64 intermediates/f32 storage; an
old zero maximum has zero fraction. Current wait becomes min(old_remaining,
new_delay), never extends/restarts on growth or rate-only/repeated edits. This is
not reconstruction of elapsed time since the last hit. Subsequent positive hits
use the new delay. Reconfiguration transitions describe edits; use accepted
advance receipts for recharge cues. Restart uses a new authored Shield plus host
input/hit/session reset. Invalid edits, damage and time leave all state unchanged.

Recharge still rounds per call to f32. Partition tests use explicit case-specific
tolerance; bitwise partition independence and guaranteed gain from tiny steps are
not claimed. Fixed scalar storage has no queue/capacity error; no independent
allocation/performance benchmark was run. Existing Neon20/8/4 setup, update order,
range/dash/hurt gates and ceil-to-integer health policy are untouched. Future
upgrades must choose value-versus-ratio intentionally and use actual accepted hits.
Shield cannot validate stale network target identities on its own.

No actionable material regression was established in this scoped review.
Preservation JSON verifies prior charge/reward/progression/root/input/game bytes.
Actual game adoption, browser HUD/recharge presentation, physical devices, audio
and sustained performance remain separate work. Active stagger248/projectile249
and local-link232 recovery are excluded from this immutable graph.
