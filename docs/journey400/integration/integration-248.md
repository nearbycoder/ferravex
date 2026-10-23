# Parent integration: stagger recovery resistance 248

Captured exact source/test over validated247. Frozen source SHA-256:
c1a2b9d13ff5fc7c636cf110b03633fe6db2e4b7f4922e5d7e52879bfc10ec00.
All-feature math190, gameplay1233 and renderer470 tests/doctests passed;
fourteen existing renderer platform/device tests remain ignored. Strict all-target
play/render Clippy, native facade and play/render/facade Wasm checks passed.
Additionally the actual unchanged Neon consumer passed45 package tests/doctests
against this frozen graph. Exact commands/inventory/logs:
integration-248/{source-capture,report,neon-command}.json and neon.log.
No root/module/manifest registration needed. Served296 release is unchanged.

Reviewed hit validation, onset, active/grace rejection, transition offset ordering,
large-step remainder, zero-grace path, reset and actual Neon consumer gates.
new/hit/advance wrappers preserve signatures and default behavior; zero grace
reduces to legacy hit and per-call poise-decay arithmetic. Original inline test
is byte-identical. Copy remains but policy/current grace add two private f64
fields; there is no stable-layout guarantee or new serialization/ABI. Existing
LoopError variants are unchanged. Policy is validated and immutable; changing
configuration requires a fresh instance, while reset silently clears runtime
poise/timers and keeps tuning.

Positive grace starts at actual stagger recovery, even inside a large dt.
Each update consumes active remainder, then grace, then idle poise decay.
Offsets are f64 seconds from this call's start. At most one recovery and one
positive-grace-end boundary can be crossed; zero grace has no grace-end event.
No new hit/onset is synthesized and zero-time/subsequent reads cannot replay
an edge. During active stagger and grace, validated hits reject entirely,
including zero poise; they neither build hidden poise nor restart timers.
Damage admission remains independently game-owned. active() is false in grace,
allowing movement/attacks; phase() supplies a steady resistance cue without a
repeated flash or required color-only signal.

Validation precedes phase rejection and all mutation. Invalid hit/time retains
all state and later behavior. Tests cover exact boundaries, partial/whole grace,
large steps, maximum/subnormal timers, decay/zero decay, reset, explicit disabled
policy and19200 spam hits across16 cycles without timer extension. Binary test
partitions match; arbitrary poise partitions retain per-call f32 rounding and
are not promised bitwise equal. Scalar state/reports are bounded with no queue;
no allocator/performance measurement or hard real-time guarantee is claimed.

The executable public recipe uses the real ActionTimeline windup/cancel API.
Only accepted StaggerHit::Onset forces cancellation; a grace-rejected hit leaves
a new windup running. These are supplied authoritative poise events in a
headless recipe, not physical hit or rendered resistance evidence. Existing
Neon heavy enemies keep40 threshold/10 decay/0.8 duration and zero grace; the
accepted-damage branch's bool gate still cancels only on onset. A later optional
tougher variant must author its own grace tuning and balance/visual tests.
No default brute rebalance was made. Parent preservation JSON verifies unchanged
Neon/game/root/input/earlier gameplay files.

No actionable material regression established in this scoped review. Actual
optional-mode migration, fresh browser presentation, physical devices, audio and
sustained performance remain separate work. Active local-link232, projectile249
and upgrade251 implementations are excluded from this immutable graph.
