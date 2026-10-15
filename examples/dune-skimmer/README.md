# Dune Skimmer

A low-poly 3D hover time trial through a sandstone ruin circuit. Finish three laps through fourteen ordered gates, preserve momentum through the bends, and spend boost on straights. The teal gate is your next checkpoint.

```sh
cargo run --release -p dune-skimmer
```

| Control | Action |
| --- | --- |
| W / Up | Accelerate |
| S / Down | Brake and reverse |
| A / D / Left / Right | Steer |
| Space | Boost; energy recharges when unused |
| R | Restart the race |
| Touch | Hold the left/right buttons to steer, GO to accelerate, BOOST to accelerate and boost |

On touch screens the controls appear after the first touch. After finishing, tap the screen or press R to race again.

Gates must be crossed forward, inside their horizontal and vertical opening, in order. Skipping gates or driving backwards cannot earn a lap. Rocks and gate posts repel the craft and scrub speed. Loose sand slows the skimmer, disables boost, and gives a visible warning. Straying far from the circuit automatically returns the craft to the last earned gate with a three-second time penalty. Steering still works at a standstill so missed gates can be revisited.

The simulation runs at 120 Hz with bounded fixed-step catch-up. Chase-camera smoothing, interpolation, body banking, hover motion, boost response, and bounded dust particles make movement legible. All geometry is generated once and reused. The environment uses Ferravex's portable flat-lit material and its tessellated ground plane, which preserves distance fog across large surfaces. This example deliberately uses small arcade hover dynamics rather than a rigid-body vehicle model.

## Validation

```sh
cargo test -p dune-skimmer
cargo clippy -p dune-skimmer --all-targets -- -D warnings
cargo check -p dune-skimmer --target wasm32-unknown-unknown
cargo run --release -p dune-skimmer -- --autoplay --smoke-frames 3600 --screenshot /tmp/dune-skimmer.png
```

Autoplay uses the same throttle, steering, and boost input path as human play. The headless tests cover complete three-lap progress, forward-only 3D gate apertures, checkpoint recovery and shortcut rejection, obstacle impacts, sand drag, and boost recharge. `--smoke-frames` uses reproducible 60 Hz frame durations, while the game still simulates at 120 Hz. Gameplay and result screenshots are in `screenshots/`.
