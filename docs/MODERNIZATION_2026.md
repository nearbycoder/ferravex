# Modern game presentation and Glowtide

Release work dated 2026-10-27. The catalog now contains eighteen playable games
and labs. This update includes a presentation pass on the seventeen existing
games, shared renderer and simulation performance improvements, and the new
[Glowtide](../examples/glowtide/README.md) roguelite.

## What changed

The shared native GL and browser WebGL2 renderer can now bloom emissive world
geometry, tone-map and grade the scene, then composite a crisp HUD. Each game
chooses its look. Low quality disables post-processing; reduced flash caps bloom
and suppresses grain and chromatic aberration. The common input/preferences
contracts remain in use. See [visuals](VISUALS.md).

The [UI kit](UI.md#modern-ui-kit) adds a mixed-case pixel face, measured styled
text, anti-aliased shapes, cards, meters and buttons. Easing, springs, staggered
entrances, counters and screen transitions live in the shared motion APIs.
Games keep their own palettes, rules and screen layouts.

| Games | Presentation direction |
| --- | --- |
| Emberline, Orchard Guard, Pocket Rally, Copper Greens | Warm light, soft panels and shadows, clearer menus and counters |
| Neon Swarm, Crystal Cavern, Salvage Pinball | Glowing arena lights, crystals and table lamps; sharper foreground UI |
| Marble Run, Dune Skimmer, Sky Courier | Scene grading, atmospheric depth and readable race/flight information |
| Siege Foundry, Strike, physics labs | Foundry glow, impact feedback, styled controls and results |
| Courtyard | Its own WebGL2 lighting, sky and post-processing path |
| Orbit Breaker | Its own software rasterizer, modern glyphs and bounded bloom buffers |

Glowtide brings automatic weapons, eight weapon evolutions, ten passives,
four playable spirits, boss encounters, local progression and five- or
ten-minute tides. Its seeded rules run at fixed ticks. Gameplay input edges
are consumed on actual ticks, upgrade choices go through rule input, and
terminal ticks stop catch-up. The title, character selection, Shrine, pause,
upgrade, chest and results screens adapt to small portrait and wide screens.

The homepage features Glowtide, and the catalog, raw browser gallery,
per-game instructions, search corpus and documentation guide include it.
Gallery previews were captured from the changed games and losslessly compressed.

## Performance evidence

The [performance report](PERFORMANCE.md) separates the engine-only comparison
against the original games from the cost of the new presentation. In its measured
workloads, ECS queries roughly halved their CPU time and the CCD workload fell
from 2.62 ms to 1.86 ms. Those results are local measurements, not a promise about
other hardware. The original-game Wasm modules grew by about 0.59% before the
new visual work; this is not a download-size reduction.

Renderer regressions cover zero-allocation warmed primitive drawing, bounded
cube-cache churn, post/glow/HUD composition and modern UI drawing. Glowtide's
regression exercises 600 simulation ticks while maintaining more than 300 foes
without heap allocations. Some other games retain allocation-producing HUD or
formatting paths; there is no catalog-wide zero-allocation claim.

## Verification and limits

The integration workflow uses:

```sh
python3 -B scripts/validate.py --mode full --jobs 2 --require-complete
```

This selects dependency audits, workspace formatting/checks/tests/Clippy,
compiled and executed guide examples, Python/Node tests, release Wasm builds,
actual-Wasm host tests, fresh native captures, native GPU pixel tests, scaffold
validation, and generated-site/search/HTTP tests. Its report distinguishes a
complete passing run from checks skipped after a failed prerequisite.

Additional review uses Chromium to load the actual browser hosts and checks
graphics status, post-processing and console errors. Native and browser captures
are inspected at desktop, portrait-phone and short landscape sizes. Automated
input and fixed-delta simulation checks are distinct from human playtesting.
Physical touch devices, gamepads, audible sound, Windows/macOS/iOS interactive
runs and broad hardware performance have not been established by these local
checks. See [platform support](PLATFORMS.md).
