# Parent integration:207 scoped scheduled jobs

Reviewed the receipt, scope identity/generation contract, explicit cancellation
and payload-disposal policy, retained FIFO/deadlines, and executed-tick respawn
recipe. Existing scheduler module/facade paths are registered. No extra root
registration is required.

Parent all-feature scheduler run passes17unit tests; preserved005 plus new207
allocation suites pass3+5tests; scheduler rustdocs pass5runnable+1compile-fail.
All-feature engine/facade Wasm compilation passes. The child's strict Clippy and
formatting results remain recorded in its receipt. Exact parent commands/logs
and scheduler source hash are in integration-207/report.json. API map links the
actual scope methods and warns that dropping a handle does not cancel jobs.

This accepts the bounded native/API/Wasm integration and runnable consumer
recipe; game adoption, browser behavior and a full Journey400 release remain
pending. No staging or immutable Journey200 artifact was changed.
