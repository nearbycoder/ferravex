# Shared Controls readability evidence for planned task292

J200196 reported and parent inspected
`target/journey-200/visual/strike/50-v2-320x480-controls.png`.
The shared editor at320logical pixels ellipsizes action names (`MOVE...`,
`FORW...`, `BACK...`, `VERT...`), bindings and the explanatory save/close hint.
Done, paging and Reset remain reachable. This is existing shared editor
behavior, not a Strike-private layout defect; no source repair is claimed.

The relevant owner path is already task292's
`crates/ferravex-render/src/input/player/editor.rs`. Alongside its visible-control
hints and the separate known PageUp/PageDown labels evidence, provide a bounded
way to read full current action/binding labels at320px. Prefer measured wrapping
or a focused full-label detail row rather than reducing essential type or
removing bindings. Preserve input journals, capture/cancel, pagination, saved
mapping identity and all existing editor tests. Ensure the full save/close
explanation remains readable. Use existing accepted layout helpers only after
checking registrations; do not duplicate a private game editor.

Add a production geometry/glyph regression and a fresh browser check of actual
long/default/Unbound actions with Done/paging/Reset still reachable. Parent owns
shared Wasm rebuild and consumer rechecks. This evidence is a future requirement,
not another launched assignment or an implemented feature. Current game v2 and
shared panel fixes do not resolve it.
