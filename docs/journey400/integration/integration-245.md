# Parent integration: inspectable pity delivery 245

Captured the two exact returned files over validated243; no registration or
manifest edits. Source SHA-256:
9ad9d583932609cd594e263769c31ebf8bd5ba7d1f492450c7b1a39c6028b645.
All-feature math190, play1174 and renderer470 tests/doctests passed. Fourteen
existing renderer device/platform tests remain ignored. Strict all-target
play/render Clippy, native facade and play/render/facade Wasm checks passed.
Commands and hashes: integration-245/{source-capture,report}.json.

Reviewed actual offer/settlement/reset/exhaustion ordering and executable
ProjectileBudget/Inventory/seeded-reward recipes. Existing four-byte Copy
PityCounter and exact legacy attempt sequence remain unchanged. Immediate
outcomes distinguish ineligible/deferred, misses, natural and guaranteed
success; natural success takes precedence at the guarantee boundary.
PityDelivery is deliberately not Copy/Clone and holds only one pending offer.
Accepted misses advance immediately; an offered reward freezes committed misses
until actual delivery is settled. Refused insertion retains the same offer,
reason, token and caller-owned payload; retry does not reroll or re-admit a hit.
Reset explicitly returns/abandons the pending offer and clears misses while
retaining sequence/exhaustion. No hidden callback or destination mutation.

Token stream IDs are caller-scoped, nonzero and must be unique across independently
routed lifetimes. Sequence IDs are never reused by the wrapper; after u64::MAX
new accepted attempts fail closed, while pending settlement remains usable.
Stale/foreign/duplicate settlement rejects unchanged. The host validates pending
before insertion, keeps exclusive ownership through insertion/settlement and
truthfully reports one insertion. This prevents duplicate settlement, not a
repeated external insertion performed before validation; panic/crash recovery and
durable exactly-once delivery are outside this helper.

The executed integration recipe confirms two host-supplied projectile targets,
rejects a repeated target, reaches a guarantee, refuses inventory insertion twice,
frees capacity, retries the retained payload and emits one delivery receipt.
The RNG consumes exactly two natural rolls. These are injected confirmed-hit
facts, not browser gameplay evidence. The separate Orbit guard model preserves
its existing destruction-roll BEFORE capacity-check ordering: full drop storage
is deferred with no new pity debt. Migration must preserve that authored policy
unless explicitly adding a new mode. Existing games are untouched.

Parent preservation checks record unchanged legacy attempt, roots, session/input,
combo/progression and Orbit/Pinball/Neon sources. No actionable material regression
established in this scoped review. Fixed storage is source-bounded; no independent
allocator/performance benchmark is claimed. Game adoption, actual browser
presentation, audio/hardware and durable host reward storage remain pending.
Served296 artifacts are unchanged.
