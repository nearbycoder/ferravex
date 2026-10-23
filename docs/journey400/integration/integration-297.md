# Parent integration: accepted audio batches 297

Captured the two exact returned files over validated preferences295, with the
single derived audio::batch module registration. Source SHA-256:
`bff6d2538fcbb943cefa82c69dfb82f48ae3529ceb5e4573f6c08722f01f5c96`.
Commands, files and hashes are in integration-297/source-capture.json/report.json.
All-feature tests and doctests passed: math190, play1120, renderer458; fourteen
existing renderer device/platform tests remain ignored. Strict all-target
play/render Clippy, native facade and play/render/facade Wasm checks passed.
The actual fixed-tick GameSession/Magazine/ObjectiveGraph recipe executed in
rustdoc; the accepted ActionMap/FeedbackController integration and warmed
allocation test executed in the complete renderer suite. No listening claim.

Reviewed validation before mutation, exact category/position coalescing,
critical overflow ordering, runtime identity reconciliation, owner borrowing,
lifecycle cancellation and exact mixer admission accounting. GameAudio, mixer,
preferences and all three browser bridges are byte-identical to the base
(preservation.json). This uses the existing ownership identity and submissions;
there is no second mixer, gameplay event inference or hidden audio retry.

A category key is local authoring metadata, not an event identity. Only equal
positions coalesce; positional Click must remain gameplay audio. Critical means
batch storage retention, not guaranteed priority/admission/audibility. Drop,
pause and reset discard pending work; release the exclusive borrow before owner
resume. Invalid queue input remains atomic even over runtime replacement.
Flush consumes suppressed/rejected entries once and reports ordered outcomes.
Counters describe admission, including voices later stolen in the same batch.

No actionable material regression established in this scoped review. Actual
Orbit/Neon/Pinball adoption must preserve accepted-event gates, authored gains,
existing category ordering where required and terminal audio lifecycle. The
feature is registered and compiles through facade/Wasm; game consumers, fresh
batch-driven browser audio, hardware recovery and listening remain pending.
Active networking230, renderer status296 and gameplay241 are excluded. No
served release was replaced by this library-only integration.
