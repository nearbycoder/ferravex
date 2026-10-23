# Parent integration: distinct weighted offers 244

Captured exact loot source/test over validated245, preserving pity245 and
progression243. Source SHA-256:
0d429448652087473f17b89f767149fade32164b821ea6d7f42e4ed1da7d69ff.
All-feature math190, gameplay1193 and renderer470 tests/doctests passed;
fourteen existing renderer device/platform tests remain ignored. Strict all-target
play/render Clippy, native facade and play/render/facade Wasm checks passed.
Exact commands, logs and inventory: integration-244/{source-capture,report}.json.
No module/manifest registration was needed. Served296 release is unchanged.

Reviewed actual preflight, bounded mapping, scratch removal, ordered selection,
report precedence and public regressions. Legacy constructors, SplitMix64,
choose and roll function bodies are byte-identical; old seeded output/advancement
goldens pass. The existing multiply-high mapping has small rounding bias for
some totals; preserving it is deliberate compatibility, not a new unbiased claim.
New rolling uses upper32-bit samples and rejects the incomplete modulo cycle
before selecting from remaining weights. Each selected index loses its entire
scratch weight, so there is no duplicate-draw retry. Authored weights remain
unchanged and zero weights never appear. Different indices with equal caller
reward values are still distinct. Sequential conditional probabilities hold for
independent uniform samples; conditioning on full completion under finite global
budget can censor the distribution. Deterministic tickets map modulo each
remaining total and are not an unbiased conversion of arbitrary u32 input.

At most256 requested indices,256 scratch weights (1024 bytes) and4096 random
samples per call. At most256 deterministic tickets are consumed even from a huge
slice. Request/capacity/budget preflight precedes all mutation. Capacity is
min(request,positive-entry-count), not the potentially shorter exhaustion prefix.
Zero request is a no-op with valid budget; budget ceiling still validates first.
Every accepted or rejected RNG output is counted; no extra sample after completion
or table exhaustion. Only the written prefix changes, including on exhaustion.
Partial prefixes and RNG advancement are committed, and each later call starts a
fresh pool; concatenating calls is not a continuation. Complete takes precedence
over table exhaustion, then sample/ticket exhaustion. No biased fallback.

Tests cover exact rejection thresholds including u32::MAX total, sparse/maximum
pools, forced rejection and partial prefixes, error precedence, output-tail/RNG
atomicity, uniqueness across seeds and weighted ticket domains. The instrumented
native allocator observes a positive Box control and zero calls in measured
maximum, error and partial draw paths. This is bounded source/test evidence,
not a universal stack-size or performance benchmark.

The executed accepted-wave recipe admits a real WaveSequence actor, receives
host-confirmed defeat, consumes one emitted clear reward through Progression,
filters capped Upgrades indices, caches the offer prefix and spends one earned
point on an offered eligible rank. Two-wave regression proves no repeat reward
and cap exclusion. Combat confirmation is supplied by this headless host; no
physical kill/game victory is claimed. Consumers must retain one offer set
across redraws, handle short/empty prefixes without rerolling or spending, and
own stable reward identity, stale-command validation and atomic spending policy.
Existing Neon automatic upgrades and Orbit destruction/pity defaults are untouched.
Task251 and later optional game modes can adopt these APIs deliberately.

No actionable material regression established in this scoped review. Preservation
JSON verifies prior accepted gameplay/root/input/game bytes and legacy reward
functions. Actual game migration, browser choices, hardware/audio and sustained
performance remain separate work.
