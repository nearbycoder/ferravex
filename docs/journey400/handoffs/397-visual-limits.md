# Physics2D scrolled visual acceptance gap

Original task197 rebuilt readable diagnostics but the T3 environment displays
black lower WebGL canvas regions after ordinary scrolling. Exact v1 gate is
web-gate-physics2d.json (manifest7fff5ba5...). Parent reproduces this without
Ferravex in target/journey-200/capture-diagnostic/index.html. Six hash-indexed
screenshots compare game, CSS layer hint, WebGL default/preserved/no-AA and
Canvas2D. Only Canvas2D fully paints. See that directory README and original
owner scroll-capture-failure.json. No physical-device/root-cause claim.

For task397 final visual verification, recheck the ordinary full lower
diagnostics, controls overlay, page rail and fullscreen on an environment that
faithfully displays/captures the minimal WebGL fixture. Source geometry/native
and Wasm checks do not establish visible content. Preserve the gap instead of
inferring acceptance from readPixels or adding unproven engine workarounds.
