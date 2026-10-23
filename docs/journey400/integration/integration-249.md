# Parent integration: projectile contact outcomes249

Exact source/test captured over validated248. Source SHA-256:
2fb26f53f93b733827d88da2aa628ef71ddd6636caf011bfef145ea04075c34f.
Math190, play1242 and renderer470 tests/doctests passed;14 existing renderer
platform/device tests remain ignored. Strict all-target play/render Clippy,
native facade and play/render/facade Wasm checks passed. Commands/hashes/logs:
integration-249/{source-capture,report}.json. No registration required.

Reviewed production target ordering, vector validation/normalization, reflection,
report mappings and actual sweep/Vitals/feedback recipes. Existing HitResult and
legacy method signatures remain exhaustive/compatible. Inactive precedes duplicate;
zero pierces admits one final hit. Invalid normals reject before inactive checks.
Separating/tangent contacts keep velocity/bounces; approaching zero-bounce contact
stops. Reflection uses the original arithmetic without an output clamp: some
reflected outputs can exceed the legacy input bound and reject on a later call.
Sampled legacy bit-equivalence tests preserve that behavior. Defensive storage
exhaustion is honestly documented as unreachable through valid construction.

IDs remain opaque host-provided generation identities; use full Entity::to_bits,
validate current existence/session externally and never use slot alone. No global
identity/anti-replay ledger or collision query was invented. Nominal contact
acceptance consumes budget even if later health/damage refuses, with no refund.
Copy forks a lifetime; replacement creates a fresh lifetime. Reports are fixed
storage and state counting is O(N<=256), without allocation profiling claims.

Executed real sphere/plane sweep recipes distinguish miss, duplicate, immune
health and accepted damage; feedback follows actual Vitals delta, not budget
admission alone. Headless supplied geometry is not live weapon/gameplay evidence.
Actual traversal ordering/progress caps and optional Neon weapon variants remain
deferred tasks254/342. Prior pity245 consumers pass. Preservation JSON verifies
unchanged roots/input/game/earlier gameplay bytes. No actionable material
regression established in this scoped review. Game migration, physical devices,
audio and sustained-performance work remain separate. Served296 unchanged.
