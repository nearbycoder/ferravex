# Parent integration: live UI preferences 295

Exact returned source and the narrow parent fixture correction were captured
over validated289/293. Source SHA-256:
`9654ba70b0b618efa89760554d91ff3eeb083d59961f24209462274a5b7ded52`.
All-feature tests/doctests passed: math190, play1120, renderer445; fourteen
existing renderer platform/device tests remain ignored. Strict all-target
play/render Clippy, facade native and play/render/facade Wasm checks passed.
The four actual theme/reflow tests use Ui/flow/text measurement and shared
paint/hit rectangles; existing public preference and feedback behavior remains.

The original shared bridge fixture required two additions for the new channel:
a numeric-success UI export and the frozen versioned player snapshot. All17
original assertions pass after this correction, as do all12 new source-only
UI regressions. No assertion was removed. Logs are ui295-parent-host-fixture.log
and ui295-parent-node.log in the parent target/journey-400 directory.

## Fresh linked release and browser check

Built the isolated source with scripts/build-web.py release; zero external
packages, thirteen runtime games and the two existing authoring variants
(Marble Run/Orchard Guard). All667 inventoried source files match the capture
and build manifest; all59 served files match its hashes. Manifest SHA-256:
`da868c18639373ade14ec2faacea90429a2800e4f1025af80e321baee377d183`.
Gate: target/journey-400/web-gate-ui295.json, separate from the original
Journey200 playability gate. Server8042 serves only this immutable release.

All15 actual modules instantiate and pass180 UI setter cases plus120 preserved
feedback setter cases. The new export is additive. Existing imports/exports are
retained against archived pre-Orbit-v3 Wasm; Orbit's two already-accepted status
exports are also accounted for. Feature exports match manifest variants. Initial
checker runs correctly stopped on a missing prior stage and a mistaken authoring
ID assumption; the final checker uses archived bytes and manifest variants.
The ABI test checks real return codes, not inaccessible TLS state readback.

T3 navigation/snapshots worked, but two Details clicks timed out and evaluation
reported no automation host. Switched to the agent-browser skill using an
isolated session, recording that limitation. Actual DOM selections on Strike,
Courtyard and Orbit applied Large text/High contrast, retained them after reload,
and kept uiPreferencesError empty. Reset restored defaults. Strike's dialog
was inspected at1280x800 and320x568; the large high-contrast dialog remained
inside the narrow viewport. Screenshots/JSON/hash records are in
integration-295/browser-evidence.json. Both owned browser sessions were closed;
no user's tabs or prior46017 server were changed. No memory/time/outcome edits.

## API and review boundaries

Reviewed complete version/finite/range validation before publishing UI state,
opaque Rust snapshot construction, theme token derivation and independent
legacy/UI refresh/error paths. JS snapshots are frozen and copied; invalid reads
and rejected exports remain observable while preserving last-good UI state.
Resize/context/pageshow/visibility recovery and disposal have focused source
coverage. The UI channel does not replace or couple the motion/flash channel.
Existing player storage/import/export formats remain unchanged.

Games must explicitly consume ui_preferences().theme() and reflow. This work
does not automatically resize or recolor existing demo HUDs or world geometry.
At320px and2x text, the tested full Controls caption reports explicit overflow;
1.5x is the browser Large setting. Fixed CardLayout styling and internal button
padding have documented adoption limits. Game-level consumer migration,
physical devices, audio, native GPU/fullscreen and sustained performance remain
pending. No actionable material regression established in this scoped review.
Concurrent authority230, status296, audio297 and gameplay241 are excluded from
this release snapshot.
