# Journey400 315 parent integration

Captured exact returned clock and unique regression files over validated316. The child receipt omitted final source hashes; parent measured clock.rs13777682a0c705d1d301620c638ac79d0772d339309f84574bf05d6c2bcf1912 and journey400_315.rs2ab2c7eeb99797959a1c59164cf044c5688893e2e4362c5502148a27fad461aa before capture. Inventory `integration-315/source-capture.json` SHA-256 is 186267936f4da05671e429e4606fc01aab5132db621e654b5d99290f5226a9b5. Active history314, encoder317 and inspector285 work is excluded. Existing clock registration suffices.

Parent passed109 net unit/integration/doctests and37 existing FPS library tests with zero failures or ignores. Strict all-target/all-feature net/FPS Clippy, native all-feature facade compilation and net/FPS/facade all-feature Wasm compilation passed. Exact commands/logs are in `integration-315/report.json`. The actual supplied-time probe/observation/diagnostics/render/reset recipe compiles and executes; existing FPS exchange/replication remains compatible.

Reviewed detailed rejection ordering and commit boundaries. Legacy bool observation delegates to the detailed result without changing estimator acceptance/formulas. Invalid bounds/timestamps/probes/anchors/RTT/tick extension fail before clock state commits; even matching expired probes remain available until overwritten/reset. ProbeNotRetained intentionally combines histories the four-slot store cannot distinguish. No rejection counter or fictitious reason is added.

Diagnostics distinguish unknown, invalid inspection timestamp, inspection before a retained timestamp and elapsed age. Ages refer to caller-supplied local time, not an inferred wall clock; plausible cross-domain timestamps cannot be detected. RTT/offset freshness is time since the latest accepted update, while smoothed values retain older contributions. Existing rate-dependent delay and estimate domains remain explicit and distinct.

Reset clears estimator/probes/anchor/freshness while retaining the nonce cursor, allowing a first new accepted observation to rebase lower time/tick origins. TickUnwrapper has a separate reset. Finite nonce/timestamp reuse cannot authenticate epochs, so outer session filtering remains mandatory. No unchecked offset injection, drift algorithm or packet version was introduced.

Tests cover all rejection reasons and whole-clock preservation, overwritten/expired/duplicate probes, exact2^53 limits, nonce/tick wrap, equal/half-space tick rules, smoothing/jitter/delay, reset/lower-origin behavior, private extended-overflow boundary and warmed allocation observations. Existing huge-tick f64 estimation precision is preserved; history314 owns the separate precise bracket addition.

No FPS clock consumer, telemetry UI, network driver or browser artifact changed. Driver adoption and live adverse-network/browser/device/performance acceptance remain separate. Reset is a reconnect boundary, not a substitute for authoritative session checks.
