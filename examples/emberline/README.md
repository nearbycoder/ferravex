# EMBERLINE

Carry a lantern through an abandoned woodland mill. This asset-free 2D platformer uses first-party Ferravex character, session, camera, animation, audio, checkpoint and time-trial systems. Native play uses Linux X11/GLX; browser play uses the shared WebGL2 host.

```sh
cargo run --release -p emberline --offline --locked
cargo run --release -p emberline --offline --locked -- --autoplay --smoke-frames 620 --screenshot /tmp/emberline-win.png
```

## Start, controls and retry

Interactive play opens a start card. **Enter** starts, resumes a paused run, or retries after completion. **A/D** or **Left/Right** move; **Space**, **W**, or **Up** jump. Tap for a short hop or hold for a gap-clearing arc. **Escape** pauses/resumes; **R** restarts the whole run, including while paused. **F2** opens the shared controls editor. Default controller controls are left stick/D-pad movement, A jump/confirm, Start pause and Back restart.

Touch the start/completion/pause card to continue with the default layout. During play, touch controls provide left, right, jump and a labeled pause button. Controls can be remapped in the shared editor. Canvas hints read the current bindings for the most recently used input device, including start/resume/retry. Long binding lists display **SEE CONTROLS** instead of a truncated key name; unavailable device bindings say **Unbound**. Default touch labels appear only while their actions still use those channels. F2 remains the reserved controls-editor shortcut.

Opening controls or losing window/page focus gates interactive simulation and cancels pending buffered jumps; closing controls does not override a manually paused session. Gameplay audio pauses with these local gates, resumes only for fresh events, and resets on respawn/retry. A completion cue can finish after reaching the lift. Restart clears movement, collected embers, score, checkpoints, trial splits, temporary platforms, camera history, animation and feedback.

In the browser, Tab/Shift+Tab leave the canvas when Controls is closed. The host reserves Tab for page navigation even if it appears in a saved gameplay binding; choose another key. Pause before moving focus to page controls: moving DOM focus alone does not pause the run.

Below 700 logical pixels wide, a compact overlay keeps the score, lantern progress,
start/pause/results cards and current control hints separate from the scaled
world. At 320/390 widths with at least 360 pixels of canvas height, text uses
14–20 logical pixel sizes and default touch buttons are at least 44 pixels in
each dimension. The drawn buttons and touch regions share the same layout;
custom touch settings still belong to Controls. Long remaps use the complete
**SEE CONTROLS** fallback. The course and camera retain their authored aspect
ratio, so landscape remains useful for seeing larger scenery and characters.
This source change requires a fresh parent Wasm build and visual recheck.

The three sections are Treeline, Old Mill and Furnace. Avoid orange spikes and fissures, land beside the two numbered lanterns to save respawn points, and enter the green **EXIT / THE LIFT** on the ground. Lantern labels distinguish unlit, passed and current respawn points; the HUD reports progress, and a newly lit lantern produces one confirmation cue. Falling returns to the last activated checkpoint (or the trailhead) while keeping earned embers, score and trial time. There is no lives-based loss state. The completion card freezes the result until retry.

## Movement and feedback

Movement runs at 120 Hz through the shared `ferravex_physics::character::Character`, with acceleration, a 100 ms coyote window and a 120 ms jump buffer. The example supplies its authored `Terrain` adapter. Consumed press edges reach the motor once: separate taps on adjacent ticks can perform an earned air jump, while holding alone cannot auto-jump on landing. Jump takeoffs, earned air jumps and spring launches produce a brief ember accent and a jump cue. Landings produce small dust puffs and an impact cue whose intensity reflects landing speed and whose surface follows the section. Animation, feedback, scenery time and camera smoothing stop with simulation while paused or complete.

Live motion and flash settings use `ferravex_render::preferences::feedback_policy()` on every presentation frame, including Ready, Paused, completed and zero-tick frames. Reduced motion removes dust, jump accents, ambient drifting embers, collectible bobbing, decorative parallax, idle/run bob and landing squash; it also removes camera velocity look-ahead while retaining essential course following, airborne poses and moving-platform simulation. Reduced flash independently removes the bright jump accent. Checkpoint/respawn messages remain static, readable text, not screen flashes. Disabled effects are cleared immediately and cannot return when settings are restored; only new events can create new accents. The shared intensity scales remaining effects. These preferences do not change collision, trial time, scoring, input, accepted events or footstep timing.

Optional embers build a five-second score chain, with multipliers every two pickups up to x4. Each ember scores once. Falling breaks the chain while preserving total score. Collect three embers to unlock one extra jump per landing; the HUD shows readiness. Inventory uses the shared bounded `Inventory`.

Green springboards on three raised side platforms launch optional high routes. Their `LaunchPad` rules fire on contact edges with a cooldown; jump release does not cut the spring arc. Two high bonus platforms warn for 0.65 seconds before disappearing and regrow after three seconds through `CrumblePlatform`. Death restores temporary platforms and movement state.

Ordered trial splits mark the mill, furnace and exit. Gold/silver/bronze targets are 12/18/30 seconds. Pausing consumes no trial time; falling keeps the clock running. The two checkpoint lanterns use ordered `Checkpoints` respawn anchors, so earlier lanterns cannot move saved progress backward.

## Session and host APIs

The private `Game` simulation owns `ferravex_core::session::GameSession` with `GameSession::new(120.0, 12, autoplay)`. Its `advance` method consumes fixed-tick input, updates game rules, calls presentation after each accepted tick and returns simulated seconds for camera smoothing. The renderer calls the same path exercised by session regression tests. `SessionState::Ready`, `Playing`, `Paused` and `Won` govern this demo; hazards respawn rather than entering `Lost`. The shared session methods used here are:

- `start(&mut self) -> bool` starts the ready session.
- `toggle_pause(&mut self) -> bool` handles pause/resume; menu transitions also cancel buffered simulation jumps.
- `restart(&mut self)` resets fixed timing and enters play; the demo separately rebuilds its game state and resets presentation.
- `advance(&mut self, elapsed_seconds: f64) -> FrameStep` schedules work only in play, so inactive time cannot accumulate.
- `finish(&mut self, won: bool) -> bool` freezes completed runs. Remaining steps in that render frame are skipped.

The host uses `PlayerInput::sample` with the session's playing state, retaining input focus/settings gates without a second pause flag. Gameplay edges are consumed only on actual fixed ticks; context transitions suppress held input when starting, pausing or restarting. `Canvas2D` preserves the playfield aspect ratio. `Follow2` cuts on restart/respawn and uses a 12-units/second look-ahead velocity dead zone to settle small residual movement. Timed `SpritePlayer` poses and audio advance from accepted simulation ticks. Detailed `CheckpointActivation::Activated` outcomes alone emit lantern confirmation; `Checkpoints::recovery()` supplies the respawn anchor without a duplicate cached position.

The public native entry point remains `emberline::run()`. The `ferravex_render::game!` macro and its Wasm exports are unchanged. `Demo` preserves `--autoplay`, smoke-frame and screenshot flags, plus the browser's shared autoplay launch support. Autoplay starts immediately and issues real movement/jump inputs; it does not teleport or bypass collision.

## Checks

```sh
cargo test -p emberline --offline --locked -j 2
```

Tests cover floor collision, coyote/buffered jumping and expiration, adjacent fresh press edges, moving platforms, score/inventory, checkpoint respawn, spring/crumble routes, sprite animation, real-physics autoplay completion, ready-state gating, pause without catch-up, controls-gate buffered-input cancellation, complete restart clearing, terminal-state freezing, and one-shot checkpoint/landing feedback. Presentation regressions cover live independent preferences during menus/zero ticks, clearing old/disabled effects, unchanged footstep timing, audio suspension, remapped/imported/reset hints, bounded long-label fallbacks and non-consuming touch hints. Autoplay is compared tick-for-tick under motion/flash combinations and zero/fractional intensity. Rustdoc checks the native entry-point example. Browser, Wasm and visual verification are separate parent integration checks; compilation and historical screenshots do not establish current visual results.

## Browser player tools

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
