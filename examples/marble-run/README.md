# Marble Run

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A complete 3D rolling-ball time trial across a suspended stone aqueduct. Collect six gold crystals, activate two checkpoints, and reach the teal finish arch. Falling restores the latest checkpoint and preserves collected crystals; the timer keeps running.

```sh
cargo run --release -p marble-run
cargo run --release -p marble-run -- --autoplay --smoke-frames 3000 --screenshot /tmp/marble-finish.png
cargo test -p marble-run
```

- WASD / arrows: camera-relative roll.
- Space: brake.
- Hold Tab: see the entire course.
- R: restart the time trial.
- Touch: drag the lower-left stick and hold the lower-right brake; tap the finish card to restart.

The marble uses an original `ferravex-physics` sphere with bounded CCD, friction, applied impulses and torque. Physics runs at 120 Hz through `ferravex_core::FixedClock`; render poses interpolate between physics ticks. All procedural meshes are built once, with flat facets, directional light and distance fog. The level uses no downloaded assets and no textures. Its narrow lanes have visual edge markings rather than invisible safety walls.

`sim.rs` keeps gameplay independent of the graphics loop. Tests exercise physical settling, checkpoint recovery, required pickups, physical rotation/braking, and a complete zero-fall run through ordinary analog inputs. Autoplay steers toward waypoints through exactly the player physics path; it does not teleport, collect remotely or bypass collisions.

Native Linux and browser build paths are implemented; rerun compilation, autoplay and interactive checks against the restored runtime and solver. Touch input is provided but has not been tested on a physical mobile device.
