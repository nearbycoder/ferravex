# Salvage Pinball

A warm, geometric salvage-yard table: save three steel balls, hit powered pop bumpers, and recover the highest score you can. Every hit is a real Rapier contact. Each bumper awards 100 points on a new contact, with a brief debounce to avoid counting persistent contact repeatedly.

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

The three dots represent balls remaining, including the ball in play. A ball returning down the launch lane can be served again. A drain consumes one ball; after three drains, tap or press R to restart. Autoplay uses the same launch and flipper controls and restarts after a completed game.

## Engine exercise

- Ferravex `FixedClock` at 120 Hz with bounded catch-up.
- Ferravex Rapier 2D world, fixed capsule rails, CCD dynamic ball, kinematic rotating capsule flippers and actual contact queries.
- Powered bumpers apply an outward impulse only on physical contact. Flippers transfer momentum through Rapier's contact solver; neither player nor bot can directly steer the ball.
- Matching geometric table art and collision geometry; no external assets, UI framework, textures, or per-frame physics object creation.
- Shared `Demo` flags support reproducible native smoke runs and screenshots. The package also compiles for `wasm32-unknown-unknown`.

Five headless tests cover gated launch, contact-only scoring/debounce, three-ball drain/game-over, physical flipper momentum transfer, and a 60-second autoplay simulation. The autoplay fixture produces 300 points, three bumper hits and three drains on the current native build.

This is a single-table arcade slice. It has no audio, multiball, tilt/nudge, high-score persistence, or networking. Touch controls are implemented but device hardware has not been tested. No changes to shared engine crates were necessary: the exposed Rapier world already supplied the required kinematic-body and contact APIs.
