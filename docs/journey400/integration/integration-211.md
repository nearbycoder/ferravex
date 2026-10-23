# Parent integration:211 component presence filters

Reviewed the returned receipt and registered source at SHA256
7adecc7bf8528b90fa0cfeccb2f5fb7367da79839fd846dd1a6b5e61c62d3e7f.
Filtered queries precheck live identity, structural predicates and required
accesses before borrowing; legacy type signatures retain their default filter.
Marker filtering never fetches values, while fetched aliases remain validated.

Parent full ECS package run passes52unit tests,9runnable and3compile-fail
doctests. Strict all-target/all-feature ECS Clippy, native engine/facade and
all-feature ECS/engine/facade Wasm checks pass. Exact argv/results/source hash
are in integration-211/report.json. API map now points to the registered entry.
No parent production fix or registration was needed.

Scoped acceptance releases task212 on the same source file. Its later edits
are separate work; these results describe the recorded211 hash. Game adoption,
combined release/browser checks and full journey integration remain pending.
