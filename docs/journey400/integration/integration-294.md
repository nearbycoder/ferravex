# Parent integration: opt-in touch gestures294

Exact returned touch.rs/test captured over validated288. Source SHA-256:
`b21fe9bddd4d9b39135c7e23c34210bd46955d52886992bf847f8e331a7ac350`.
Provenance, exact commands and results are in integration-294/source-capture.json,
report.json and test-totals.json. All-feature math/play/render tests/doctests pass:
{'math': {'passed': 190, 'failed': 0, 'ignored': 0}, 'play': {'passed': 1103, 'failed': 0, 'ignored': 0}, 'render': {'passed': 424, 'failed': 0, 'ignored': 14}}. Strict all-target play/render Clippy, native facade and
play/render/facade Wasm checks passed. Exact TouchSurface/ActionMap/GameSession
recipe compiled and executed against Cargo-reported artifacts from this graph;
source matches receipt294 (touch-recipe-report.json).

Parent reviewed timed capture observation and validation, profile replacement,
legacy/timed mixing, End/Cancel behavior and capture/layout reset. Existing
set_layout, handle, flush, stick and active implementations compare exactly with
validated288. handle_scaled's old controls path compares exactly after removing
only private optional gesture initialization/discard; no immediate button/stick/
look math changed (preservation.json). Phase/Event/Control/Output/InputError and
channels/formats remain compatible. Gesture metadata shares existing bounded
slots; no additional queue or per-event allocation is introduced.

Disabled by default; timed calls require explicit validated profile and one finite
nonnegative globally monotonic supplied-seconds domain across all pointers. Equal
time is valid. Rejected time/input/capacity preserves map, captures and watermark.
Timed processing reuses the existing controls path exactly once, observing the
actual finger start independently from stick center/look scale. End can observe
its supplied displacement without adding legacy End look motion. Tap uses maximum
excursion/time; drag latches excursion/minimum-time; End prioritizes swipe, then
drag, then tap. Cancel never emits a terminal gesture. Untimed accepted events
discard only the affected capture's optional recognition. Changed profile/layout
or clear releases captures and resets time; unrelated pending action edges remain
caller-owned. Observations are synchronous caller outputs, not accepted gameplay
or replay commands. Caller must clear retained drag presentation on cancellation.

No actionable material regression was established in this scoped review. Zero
allocations apply to the measured native paths, not hardware/browser latency.
Renderer PlayerInput still uses the preserved untimed path/private surface;
profile setter and supplied-time/observation bridge need deliberate future
adoption. No gesture ABI, shared browser build, game behavior, physical-device,
audio or sustained-performance acceptance is claimed. Active239/289/293 and
parent Skeleton getter/animation registration are outside this immutable graph.
