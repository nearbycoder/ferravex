# Salvage Pinball

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A warm, geometric salvage-yard table: save three steel balls, hit powered pop bumpers, and recover the highest score you can. Every scoring hit comes from Ferravex's original physics contact queries. Each bumper awards 100 points on a new contact, with a brief debounce to avoid counting persistent contact repeatedly.

```sh
cargo run -p salvage-pinball
cargo run -p salvage-pinball -- --autoplay --smoke-frames 420 --screenshot /tmp/salvage-pinball.png
cargo test -p salvage-pinball
```

| Control | Action |
| --- | --- |
| A / Left arrow | Hold left flipper |
| D / Right arrow | Hold right flipper |
| Space | Launch the ball from its lane |
| R / RESET | Start a fresh three-ball game |
| Touch / mouse | Tap to launch; hold lower left or right to flip |

The three dots represent balls remaining, including the ball in play. A ball returning down the launch lane can be served again. The first launch of each life lights a five-second ball save that rescues one early drain. Subsequent drains consume one ball; after three drains, tap or press R to restart. Autoplay uses the same launch and flipper controls and restarts after a completed game.

## Engine exercise

- Ferravex `FixedClock` at 120 Hz with bounded catch-up.
- Original `ferravex-physics` 2D world, fixed capsule rails, CCD dynamic ball, hinged motor-driven capsule flippers and actual contact queries.
- Powered bumpers apply an outward impulse only on physical contact. Flippers transfer momentum through Ferravex's contact solver; neither player nor bot can directly steer the ball.
- Matching geometric table art and collision geometry; no external assets, UI framework, textures, or per-frame physics object creation.
- Shared `Demo` flags support reproducible native smoke runs and screenshots. The browser build uses the same Rust game through the shared WebGL2 host.

Headless tests cover gated launch, contact-only scoring/debounce, three-ball drain/game-over, physical flipper momentum transfer, and a 60-second autoplay simulation. Rerun these tests against the recovered solver; historical scores and drain counts are not a validation result for this implementation.

This is a single-table arcade slice. It has procedural game audio and local records, but no multiball, tilt/nudge or networking. Powered contacts are capped at 34 m/s. Touch controls are implemented but device hardware has not been tested. The recovered version uses the original rotational solver, including ball contacts against motor-driven capsule flippers. See `crates/ferravex-physics/README.md` for unsupported shape pairs and bounded CCD behavior.
