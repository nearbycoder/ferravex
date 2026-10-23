# Journey400 282 parent integration

Captured the returned typed notice queue over stable223 source with only the parent `pub mod notices` registration. Active UiState transaction and assembly edits were excluded. Exact inventory, registration and source hash are in `integration-282/source-capture.json`.

Parent full play tests/doctests, strict play all-target Clippy, native all-feature facade compilation and play/facade all-feature Wasm compilation passed. Commands/logs are in `integration-282/report.json`. Both public notice recipes execute actual accepted-score coalescing and borrowed objective-message replacement/dismissal; earlier gameplay/UI tests remain included.

Reviewed inline bounded ownership, priority/FIFO policy, explicit duplicates/coalescing/lifetime policy and timing rejection. Critical notices survive equal/lower bursts, category keys are not event deduplication, and both visible/hidden entries age under caller-chosen delta. Numeric overflow and payload allocation/equality/drop behavior remain caller-owned. Ordinary failure retains inputs where reported; destructor or callback side effects are not given a false transaction guarantee.

The module is already reachable as `play::ui::notices` through the existing facade, without dependencies, format changes or a renderer/global clock. Game adoption must emit notices only from accepted events once and keep mandatory results outside transient delivery. No game/host artifact or authored default was changed. Headless timing/storage evidence is not browser/audio/device/performance or cross-platform determinism proof. Parent retains final combined consumer acceptance.
