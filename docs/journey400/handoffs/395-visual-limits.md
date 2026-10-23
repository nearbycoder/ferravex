# Sky visual evidence carried forward from Journey200195

Tasks350/365/380/395 must preserve the final responsive HUD, complete help
strings, fixed-size action geometry, real-binding fallback and native Panel
top/Panel bottom route. Exact final baseline is Journey200 web-gate-sky.json:
manifest e3ca61f731ded99cba794f6a7ac620cb51e0ed25de7d7385bc77b59a39272d2a,
source8529497729f1f9062fdd40e3591576808e797e8502a88d320aede3883b7f3f8a.
The195 prerequisite is accepted for dependent work within its recorded scope,
not as complete platform/visual verification.

One uncertainty remains: T3 captures sometimes show black newly exposed canvas
areas after scrolling upward, including a short standard page; native fullscreen
capture dimensions also disagree with its reported CSS viewport/DPR. DOM link
alignment, input/focus checks, progressing frames and nonblack WebGL readPixels
do not prove what a user sees. Do not silently convert this into a pass, assume
all black regions are tooling defects, or patch production CSS speculatively.
Later fresh visual acceptance must assess actual visible scrolling/fullscreen
surfaces with exact artifact provenance and record any remaining uncertainty.

Evidence: target/journey-200/receipts/195.md,
target/journey-200/integration-sky-layout.md, and its81-file evidence index under
target/journey-200/visual/sky-courier. Parent reviewed ordinary result/pause/
recovery screenshots with complete text. Live gameplay and exact-Wasm full glyph
checks passed. The shared regression is web/tests/wasm-sky-glyphs.mjs, and native
link event isolation is web/tests/journey200-sky-panel-scroll.mjs; both must be
preserved/adapted honestly with future APIs. Physical touch/device, native-window
GPU, audio listening and sustained performance were not validated. Task292 owns
the separately recorded PageUp/PageDown shared label gap.
