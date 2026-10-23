# Parent integration: atomic upgrade choices251

Reviewed owned-session point/rank/offer commit ordering, cached redraws, stale
identity precedence, checked credit and reset/exhaustion. Original Upgrades impl
is byte-identical to HEAD; parent preservation hash is recorded in
release-choice251/preservation.json. No root registration needed. Shared legacy
RNG/selection/default Neon auto-upgrade behavior is unchanged.

The whole workspace passed4096 tests/doctests, strict all-target/all-feature
Clippy, compilation/formatting and first-party audit with this final source.
The exact returned GameSession/WaveSequence/Progression/Input recipe hash
c24de6c91d01f50bd9a52b898dd972617417a38537c7686c38bc33255515cdf3
also compiled and executed on the final graph; Cargo artifact/rustc/run evidence
is retained in release-choice251/upgrade-recipe-*. The complete release Wasm/site
build and artifact suites cover this registered production module.

One successful choice updates one rank, debits one owned point and consumes the
whole offer with no external fallible callback. Rejections preserve all state.
Cached/empty/no-point/all-capped/zero-width open consumes no entropy/serial.
Monotonic serial exhaustion never wraps or resets; recreation requires a fresh
caller namespace relative to surviving callbacks. credit intentionally has no
award deduplication: hosts must credit only newly accepted promotions and own
rollback/external reward consistency. This is bounded in-object atomicity, not
external durable exactly-once delivery. Native allocation cases measured zero;
no universal stack/performance claim. Optional between-wave Neon mode342 is
among the117 deferred tasks; current balance remains unchanged. No actionable
material regression established in this scoped review. Full release/CI/browser
and deployment evidence remain separately attributed.
