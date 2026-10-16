# Marble Run

Built with first-party Ferravex APIs. Native play uses the platform host; browser
play uses the shared WebGL2 host.

A 3D rolling-ball campaign with four hand-authored courses. Choose any course
immediately, collect all its crystals and reach the finish arch. Falling restores
the latest checkpoint and preserves crystals; the timer keeps running.

| Course | Challenge | Gold target |
|---|---|---|
| Aqueduct | Warm-up: wide stone paths and gentle slalom | 35 seconds |
| Needlewalk | Narrow rails, offset pickups and committed switchbacks | 34 seconds |
| Clockwork | Three rotating physical sweepers separated by thin bridges | 40 seconds |
| Frostline | Three sloped descents and slippery ice hairpins | 32 seconds |

Gold requires the target time with no falls or recoveries. Silver allows 1.5×
the target time and up to two falls; every other successful finish earns Bronze.
Best times save independently per course and content version. The finish panel
offers Next Course and Retry for Gold; completing Frostline returns to the picker.
Targets are initial tuning values, not measured human playtest results.

```sh
cargo run --release -p marble-run
cargo run --release -p marble-run -- --autoplay --smoke-frames 3000 --screenshot /tmp/marble-finish.png
cargo test -p marble-run
cargo run --release -p marble-run -- --content examples/marble-run/content/clockwork.fxscene
```

- WASD / arrows: camera-relative roll.
- Space: brake.
- Hold Tab: see the entire course.
- R: restart the time trial.
- C / controller X: open or close the course picker without losing the run.
- V / controller left bumper: recover to the latest checkpoint (counts as a fall).
- Touch: use the stick/brake; tap Courses, Recover and the finish-panel buttons.
- Menus: mouse/touch, arrows/Tab + Enter, or controller D-pad + A.

The current course's card resumes an unfinished run; R or Retry for Gold starts
it over. The picker pauses both the timer and moving obstacles. Its four-card
layout also fits a 320 × 152 logical-pixel play area.

The marble uses an original `ferravex-physics` sphere with bounded CCD, friction,
impulses and torque. Ice uses per-platform friction; sweepers use kinematic rigid
bodies that transfer motion through real contacts. Simulation and hazard motion
pause together in the course picker or Details panel. Physics runs at 120 Hz;
render poses and sweepers interpolate between ticks. Meshes are built on course
load, including the combined striped sweeper beams. There are no downloaded
assets, textures or invisible collision rails.

`sim.rs` keeps gameplay independent of rendering. Tests complete all four courses
using ordinary rolling inputs, exercise sweeper contact transfer, verify every
checkpoint is safe, and check pickups, recovery and braking. The release-Wasm
contract covers desktop/phone course picking, settings blocking, all four
finishes and campaign progression. Autoplay steers through the same player
physics path; it does not teleport, collect remotely or bypass collisions.

The shared content editor follows the selected course. Retry retains live edits;
switching to another course loads its embedded scene. Native `--content` watches
the supplied file until switching courses.

The overhead camera preserves its follow distance beside every pole instead of
treating the open course's poles as walls that should shorten its boom. Narrow
viewports use separate title/status rows and scale the touch controls to fit.
The browser's shared Details panel adjusts resolution independently of lighting;
device resolution preserves high-density phone sharpness by default. These
choices save per game. See [visual settings](../../docs/VISUALS.md).

Native Linux and browser build paths are implemented. Touch input is provided
but has not been tested on a physical mobile device.
