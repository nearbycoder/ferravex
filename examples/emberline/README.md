# EMBERLINE

Carry a lantern through an abandoned woodland mill, light the two save lanterns, and ride the lift out before the clock beats you. Spikes have opinions, gravity has jokes, and somewhere above the Furnace a moth is waiting for someone good enough to reach it. This asset-free 2D platformer uses first-party Ferravex character, session, camera, animation, particle, audio, checkpoint, record and time-trial systems. Native play uses Linux X11/GLX; browser play uses the shared WebGL2 host.

```sh
cargo run --release -p emberline --offline --locked
cargo run --release -p emberline --offline --locked -- --autoplay --smoke-frames 620 --screenshot /tmp/emberline-win.png
```

## Start, controls and retry

Interactive play opens a start card with the three goals (light the lanterns, grab embers, ride the lift), your current move/jump bindings, the medal targets and, once you have cleared the course, your personal best. **Enter** starts, resumes a paused run, or retries after completion. **A/D** or **Left/Right** move; **Space**, **W**, or **Up** jump. Tap for a short hop or hold for a gap-clearing arc. **Escape** pauses/resumes; **R** restarts the whole run, including while paused. **F2** opens the shared controls editor. Default controller controls are left stick/D-pad movement, A jump/confirm, Start pause and Back restart.

Touch the start/completion/pause card to continue with the default layout. During play, touch controls provide left, right, jump and a labeled pause button. Controls can be remapped in the shared editor. Canvas hints read the current bindings for the most recently used input device, including start/resume/retry. Long binding lists display **SEE CONTROLS** instead of a truncated key name; unavailable device bindings say **Unbound**. Default touch labels appear only while their actions still use those channels. F2 remains the reserved controls-editor shortcut.

Opening controls or losing window/page focus gates interactive simulation and cancels pending buffered jumps; closing controls does not override a manually paused session. Gameplay audio pauses with these local gates, resumes only for fresh events, and resets on respawn/retry. A completion cue can finish after reaching the lift. Restart clears movement, collected embers, score, checkpoints, trial splits, temporary platforms, camera history, animation and feedback.

In the browser, Tab/Shift+Tab leave the canvas when Controls is closed. The host reserves Tab for page navigation even if it appears in a saved gameplay binding; choose another key. Pause before moving focus to page controls: moving DOM focus alone does not pause the run.

Below 700 logical pixels wide, a compact overlay keeps the clock, embers, lantern
pips, air-jump state, points, toasts, start/pause/results cards and current control
hints separate from the scaled world. Long toasts and coaching lines wrap onto two
whole-word lines instead of shrinking. At 320/390 widths with at least 360 pixels of canvas height, text uses
14–20 logical pixel sizes and default touch buttons are at least 44 pixels in
each dimension. The drawn buttons and touch regions share the same layout;
custom touch settings still belong to Controls. Long remaps use the complete
**SEE CONTROLS** fallback. The course and camera retain their authored aspect
ratio, so landscape remains useful for seeing larger scenery and characters.
This source change requires a fresh parent Wasm build and visual recheck.

The three sections are Treeline, Old Mill and Furnace; entering a new one shows a short banner with your split time. Avoid orange spikes and fissures, land beside the two numbered lanterns to save respawn points, and step into the green **LIFT** on the ground. Spike hit boxes are inset by 4 units at each side and 6 at the tips, so grazing a triangle's corner is forgiven. Unlit lanterns are empty cages; lit ones hold a flame and glow (shape, not color alone), carry a number plate, and the next unlit lantern shows a short **SAVE POINT** marker kept fully on screen. A newly lit lantern produces one confirmation cue, a warm burst and a toast. Falling returns to the last activated checkpoint (or the trailhead) while keeping earned embers, score and trial time. There is no lives-based loss state.

## HUD, toasts and onboarding

The play HUD keeps one job per corner. Left: the section name, two lantern pips and the air-jump state (**AIR JUMP n/3**, **READY** or **USED**). Center: the run clock, colored by the medal it is still on pace for with a small G/S/B badge, above a course track showing you, both lanterns and the lift flag. Right: embers collected, points and, while a chain is live, **CHAIN ×n** with a draining timer bar. A soft band behind the HUD keeps it readable over the sun and mountains.

One toast slot under the HUD carries the important moment (a fall quip, a lit lantern, a section split, the air-jump unlock, the secret), with an opaque backing so it never fights the scenery. Higher-priority toasts are not replaced by section banners. First-time players also get one coaching line at the bottom (how to jump, where to save, how close the air jump is); each line disappears as soon as that lesson is learned, and players with a stored clear never see them.

## Personality, juice and the finish

- **Falls** throw a burst of sparks, shake the camera, flash a soft red vignette and pop the hero back in at the checkpoint. A rotating quip names the cause (spikes or fissure) without ever repeating the previous line; three or more falls since the last lantern switch to encouragement.
- **Embers** burst, ring and float a **+points** pop; raising the chain multiplier adds a **CHAIN ×n!** callout.
- **Close calls**: skim within 26 units above spike tips in the air and land safely to score a close call (**SPICY!**, **TOES INTACT!**, ...). They are counted on the results card.
- **Squash and stretch** on takeoff and landing, harder landings add a small shake, and the hero widens their eyes during fast falls.
- **The secret**: a moth hovers high above the Furnace spring. A spring launch (or full jump) plus the earned air jump reaches it once per run for +500 through the chain and a special title.
- **The finish**: the lift car climbs its shaft with you inside under a burst of confetti, then the results card settles in with a silly rank title earned by how you played (for example **SPARK OF SPEED**, **EMBER HOARDER**, **GRAVITY'S FAVOURITE**, **FRIEND OF MOTHS**), the medal and time, your personal-best comparison (**NEW PERSONAL BEST! -0.42S** or the gap to beat), a next-goal hook (the time needed for the next medal, all embers, or a nudge toward the moth) and stats for embers, points, falls, close calls and the moth.

Personal bests use the shared `ferravex_render::storage::Records` book under owner `emberline`, record id `course` and rules context `0x454d_424c_0001` (a new, versioned save slot; no existing save format changed). Only finished interactive runs are recorded; autoplay demonstrations are labeled **DEMO RUN · NOT RECORDED**. A storage failure leaves the game fully playable.

## Movement and feedback

Movement runs at 120 Hz through the shared `ferravex_physics::character::Character`, with acceleration, a 100 ms coyote window and a 120 ms jump buffer. The example supplies its authored `Terrain` adapter. Consumed press edges reach the motor once: separate taps on adjacent ticks can perform an earned air jump, while holding alone cannot auto-jump on landing. Jump takeoffs, earned air jumps and spring launches produce a brief ember accent and a jump cue. Landings produce small dust puffs and an impact cue whose intensity reflects landing speed and whose surface follows the section. Close calls and section splits click; the air-jump unlock, lanterns, the moth and the finish play the success cue.

All juice (particles from the shared `Emitter`/`ParticlePool` drawn by `ParticleRenderer`, rings, text pops, toasts, shake, squash/stretch, the respawn pop-in, the fall vignette and lantern bloom) is created only from accepted simulation events and aged by accepted simulation time, so it freezes with the run while paused. After the finish, only the lift/confetti/results intro advances on render time; the simulation stays frozen. There is no hit-stop: the engine reserves `hitstop_seconds` for presentation and never applies it to a gameplay clock, so impacts use shake and squash instead.

Live motion and flash settings use `ferravex_render::preferences::feedback_policy()` on every presentation frame, including Ready, Paused, completed and zero-tick frames. Reduced motion removes dust, jump accents, ambient drifting embers, collectible bobbing, decorative parallax, idle/run bob, landing squash, particles, rings, camera shake, squash/stretch, the respawn pop-in, the lift ride and the card intro; text pops stay put instead of rising. It also removes camera velocity look-ahead while retaining essential course following, airborne poses and moving-platform simulation. Reduced flash independently removes the bright jump accent, the fall vignette and the lantern bloom. Toasts, pops and the HUD remain readable text that fades rather than flashes. Disabled effects are cleared immediately and cannot return when settings are restored; only new events can create new accents. The shared intensity scales remaining effects. These preferences do not change collision, trial time, scoring, input, accepted events or footstep timing.

Optional embers build a five-second score chain, with multipliers every two pickups up to x4. Each ember (and the moth) scores once. Falling breaks the chain while preserving total score. Collect three embers to unlock one extra jump per landing; the HUD shows readiness. Inventory uses the shared bounded `Inventory`.

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

Debug native builds only (never release or Wasm) read visual-review environment variables: `EMBERLINE_WINDOW=390x700` (logical window size), `EMBERLINE_PAUSE_AT=N` (press pause on render frame N), `EMBERLINE_STOP_JUMPING_AT=N` (autoplay stops jumping, to review falls) and `EMBERLINE_FAKE_BEST=SECONDS` (pretend a stored best at the finish). For example:

```sh
EMBERLINE_WINDOW=390x700 cargo run -p emberline --offline --locked -- --autoplay --smoke-frames 660 --screenshot /tmp/emberline-narrow-win.png
```

## Checks

```sh
cargo test -p emberline --offline --locked -j 2
```

Tests cover forgiving spike corners, fall causes and per-lantern fall counts, exactly-once close calls, the air-jump-gated moth secret, accepted pickup/chain/unlock receipts, ordered split banners, non-repeating quips, rank titles, medal pace, personal-best comparisons, coaching that fades as lessons are learned, two-line narrow wrapping, juice that is blocked/cleared by reduced motion or flash and is reproducible across restarts, toast priority, floor collision, coyote/buffered jumping and expiration, adjacent fresh press edges, moving platforms, score/inventory, checkpoint respawn, spring/crumble routes, sprite animation, real-physics autoplay completion, ready-state gating, pause without catch-up, controls-gate buffered-input cancellation, complete restart clearing, terminal-state freezing, and one-shot checkpoint/landing feedback. Presentation regressions cover live independent preferences during menus/zero ticks, clearing old/disabled effects, unchanged footstep timing, audio suspension, remapped/imported/reset hints, bounded long-label fallbacks and non-consuming touch hints. Autoplay is compared tick-for-tick under motion/flash combinations and zero/fractional intensity. Rustdoc checks the native entry-point example. Browser, Wasm and visual verification are separate parent integration checks; compilation and historical screenshots do not establish current visual results.

## Browser player tools

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
