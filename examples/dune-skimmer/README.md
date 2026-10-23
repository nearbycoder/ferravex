# Dune Skimmer

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A low-poly 3D hover time trial through a sandstone ruin circuit. Finish three laps through fourteen ordered gates, preserve momentum through the bends, and spend boost on straights. The teal gate is your next checkpoint.

```sh
cargo run --release -p dune-skimmer
```

| Control | Action |
| --- | --- |
| W / Up | Accelerate |
| S / Down | Brake and reverse |
| A / D / Left / Right | Steer |
| Space / Left Shift / Right Shift | Boost while accelerating; energy recharges when unused |
| Escape / controller Start | Pause or resume; countdown and race time stop |
| R | Restart the race |
| Touch | Hold the left/right buttons to steer, GO to accelerate, BOOST to accelerate and boost |

These are keyboard defaults. HUD hints show the first binding for the most recently used input device, including saved remaps; F2 / Controls lists every alternative. An unbound action says `Unbound`, and a hint too large for the bounded buffer directs you to Controls. The browser guide also documents Ctrl+F6 to leave the canvas. Moving to that guide does not itself pause the race.

On touch screens the controls appear after the first touch. After finishing, click/tap the game or use the configured Restart or Confirm action (R / Enter by default) to race again. Controls/Details interactions cannot trigger a pointer retry behind the editor. Restart returns to the countdown with a full energy meter and clears camera shake, dust, engine audio and the previous result. Release held drive controls after a restart or recovery, then press them again to rearm; the HUD prompts while controls are held.

The HUD shows the next gate, earned gates out of 42, a race progress bar and a frozen final time. Boost reports READY, ACTIVE, BLOCKED on loose sand, or OVERHEATED when depleted, with a cooling estimate. The existing energy lockout begins below 3% and rearms strictly above 25%; the meter marks that threshold. This is one energy meter, with no separate heat simulation. Centered gate passes still restore 10% energy. Hold acceleration with boost to use it; release boost to recharge. Vehicle telemetry distinguishes on-track traction, sliding, reversing and loose sand. The compact HUD separates the title, gate and timer rows, and fits hints using checked font measurements.

Gates must be crossed forward, inside their horizontal and vertical opening, in order. Skipping gates or driving backwards cannot earn a lap. Rocks and gate posts repel the craft and scrub speed. Loose sand slows the skimmer, disables boost, and gives a visible warning. Straying far from the circuit shows a recovery countdown and automatically returns the craft to the last earned gate with a visible three-second time penalty. Recovery preserves earned gates, clears banking/boost effects and snaps the camera to the returned craft. Steering still works at a standstill so missed gates can be revisited.

The live timer already includes all recovery penalties. The result separates driving time and added recovery seconds, and explicitly states that impacts cost speed without directly adding time. Gate sounds follow earned crossings; rejected/skipped gates and zero-tick frames do not replay them. Display rounding carries into the next minute instead of showing `00:60.00`.

The simulation runs at 120 Hz with bounded fixed-step catch-up. The private `session::RaceSession` host uses `ferravex_core::session::GameSession::new(120.0, 16, true)` for timing, pause, restart and completion; the authored `Race` retains vehicle and gate rules. `GameSession::set_paused` freezes countdown, race time and recovery timers while controls/settings or focus suspend play. Resuming never spends paused wall time. `finish(true)` freezes the result and `restart()` resets clock debt. Autoplay deliberately keeps running through local overlays for reproducible smoke runs.

Chase-camera smoothing, interpolation, body banking, hover motion, boost response, and bounded dust particles make movement legible. All geometry is generated once and reused. The environment uses Ferravex's portable flat-lit material and its tessellated ground plane, which preserves distance fog across large surfaces. This example deliberately uses small arcade hover dynamics rather than a rigid-body vehicle model.

The course now uses the shared bounded `Route3::set_closed` / `project` APIs instead of a second nearest-segment loop. Autoplay retains the authored curve parameter and existing controls; projection never awards race progress. Recovery uses the checked `PlanarState::reset`, and traction uses `PlanarState::telemetry`. The local ordered gate, continuous energy and three-lap rules remain intact.

Every presentation frame reads `render::preferences::feedback_policy`, including paused, Controls/Details and zero-tick frames. Reduced motion clears existing dust and camera shake, blocks new emissions/impulses, and removes banking, hover/gate bobbing, exhaust deformation animation and speed/boost camera widening. Reduced flash independently freezes the exhaust color pulse. Cleared effects cannot revive when preferences are restored. Simulation, accepted-event sounds and autoplay progression are unaffected. Presentation clocks freeze while the session is paused and reset on restart/recovery; there is no second preference store or Wasm export.

## Validation

```sh
cargo test -p dune-skimmer --offline --locked -j 2
cargo clippy -p dune-skimmer --all-targets --offline --locked -j 2 -- -D warnings
cargo check -p dune-skimmer --target wasm32-unknown-unknown --offline --locked -j 2
cargo run --release -p dune-skimmer -- --autoplay --smoke-frames 3600 --screenshot /tmp/dune-skimmer.png
```

Autoplay uses the same throttle, steering, and boost input path as human play. The headless tests cover complete three-lap progress through the session host, forward-only 3D gate apertures, checkpoint recovery and shortcut rejection, obstacle impacts, sand drag, boost lockout/rearming, pause/resume without catch-up, frozen results, control release after restart/recovery, and cleared camera/dust feedback. The crate rustdoc includes a compiling session lifecycle example. `--smoke-frames` uses reproducible 60 Hz frame durations, while the game still simulates at 120 Hz. Capture fresh gameplay and result frames when validating the recovered renderer; historical screenshots do not establish its current output.

Journey200 regressions additionally cover closed-route projection compatibility, all recovery anchors' obstacle clearance, invalid tick/control inputs, telemetry cues, penalty accounting, remapped/unbound hints, narrow hint measurements, independent visual policies and paused clearing without effect revival. Package tests and strict all-target Clippy passed locally; fresh Wasm linkage, shader/GPU rendering, browser focus and actual narrow-screen play remain parent integration checks.

Exhaust uses the shared material API for small vertex/fragment pulses. Both authored shader files and formulas are preserved: the Rust material adapter routes the vertex clock to `u_params[0].y`, while the fragment clock stays at `.x`, allowing independent motion/flash settings. `?autoplay&material-tools` opens the optional browser shader editor without changing ordinary game URLs. Failed replacements preserve the last valid GPU program. Edited shaders should retain those respective clock channels to honor the visual preferences.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
