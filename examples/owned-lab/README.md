# Courtyard

Flat-shaded stone, cardboard operators with bullseyes, a drill sergeant with opinions, a 30-second qualifier and browser peer sessions.

## Training

Choose **Enter training** to capture the mouse and start the 64 Hz simulation, or **2D overview** for the overhead camera. The panel shows the current drill, a progress bar and the one control you need right now; each hint disappears once that drill is done:

1. Travel five meters; pushing into a wall does not count.
2. Fire an accepted shot; dry fire and weapon cooldown do not count.
3. Finish a reload; opening the menu pauses its timer in offline training.
4. Land a hit on a moving operator. Operators do not fire back in offline training.

**Sgt. Gravel** comments on the radio card: a welcome, a line for every finished drill, streak cheers, gentle nudges after several misses in a row and a verdict for each run. Lines rotate and never repeat back to back.

## Qualifier

Finishing the four drills arms the **qualifier**: a 3-2-1 countdown, then 30 seconds to score as much as you can.

- Body hit 100, headshot 250, elimination +150.
- Consecutive hits build a streak: every three hits raise the multiplier, up to ×4. A miss, or four seconds without a hit, breaks it.
- The final score adds an accuracy bonus (10 points per percent).
- Grades: **S** 12,000 "Courtyard Legend", **A** 7,500 "Certified Sharpshooter", **B** 4,000 "Respectable Recruit", **C** 1,500 "Enthusiastic Liability", **D** "Danger to Pigeons".

After **TIME!** the results card shows the grade, hits, headshots, eliminations, accuracy, best streak, your personal best (or how far you are from it), the points needed for the next grade and the sergeant's verdict. Then the courtyard returns to free practice. To go again, **shoot the brass bell** on top of the central cover; the panel shows its direction and distance. There is also a small secret worth 500 points during a run.

Hits rock operators back, headshots bonk their heads, and eliminated operators tip over like cardboard cutouts with a burst of confetti until they respawn. The crosshair hit marker changes size and color for hits, headshots and eliminations, and score callouts rise beside it. An empty magazine shows the reload binding under the crosshair, and the bottom strip shows **EMPTY · RELOAD**. Offline, health is hidden because operators never fire back; it reappears in peer matches.

Your best qualifier score is saved per browser profile through the shared record store and shown in the pause menu. **Reset** clears the scene and the current run but keeps the personal best. Native and test builds keep bests for the session only. The qualifier only reads accepted offline shots, hits and eliminations on fixed ticks; it never changes the shared FPS match, peer packets or replay bytes. Peer sessions never run or score it.

Completing a run keeps the sandbox running for free practice. WASD moves, mouse aims, click fires, Space jumps and R reloads. Touch/controller layouts and remapping are available in **Controls** (F2).

On short browser screens, the play surface keeps a 640-pixel logical height so the training card, canvas guide, touch actions and health/ammo/time strip have separate space. The labelled **Scroll page** strip beside the scene supports native page panning; keyboard users can focus it and use arrow keys. Leaving the canvas pauses offline training and requires explicit resume. The scene still owns gameplay gestures.

The in-canvas guide shows measured travel, reload time and the next drill with your current device's actual bindings. Wide screens put the radio card top left and the drill panel top right. Narrow screens stack them above the touch actions, and short landscape screens keep a two-row panel to the left of the score callouts, with a shorter results card that leads with your record and the bell. Long bindings wrap across up to three lines, including explicit **Unbound** hints; exceptionally large binding lists point to Controls instead of silently showing a partial final key. A compass and a visible-operator direction/distance help orient first-person play; the overview keeps north at the top. The guide retains a visible target until another is more than two meters closer, and drops it behind courtyard cover. This assistance is offline only and never changes aim or hit detection. Accepted hits show a short crosshair marker and impact cue; completing a drill gives a brief confirmation and cue. Drills remain independent, so an early hit still counts.

**Escape**, **Pause / menu**, releasing mouse capture, hiding the tab or leaving focus pauses offline training. Closing Controls or returning to the tab leaves the menu open until you explicitly resume. A capture failure shows a retry message; **2D overview** remains available. Scene tools edit a paused world. **Reset** clears edits, score, physics, recording, aim and drill progress, returning to Ready; control profiles remain intact.

Connected peer matches stay live while local menus/settings release movement and fire input. Both authority and guest prediction keep their existing fixed ticks and sequence numbers. Reset is disabled until disconnected. Peer predictions do not award offline training milestones. WebRTC pairing and network/replay encodings are unchanged. As before, browser background throttling or graphics context loss can interrupt host frame delivery; a local menu is not a network pause.

The canvas identifies host authority versus guest prediction, waiting for the first input/snapshot, and data stale by two seconds of fixed simulation time. Guests also see their bounded unacknowledged command count. Duplicate/stale packets cannot make stale data appear fresh. Local menus stop offline gameplay voices and discard queued actions; resuming requires a fresh press for held buttons. A tap between ticks survives until the next tick and fires once during catch-up.

Shared **Player tools** motion and flash choices apply on every presentation frame, including menus and frames without a simulation tick. Reduced motion removes existing sparks and confetti, weapon bob and recoil, operator sway, hit wobble, head bonks, bell swing and the countdown punch-in, and freezes the decorative orbit camera. Score callouts stay in place instead of rising, and eliminated operators lie down without animating. Reduced flashes independently removes the hit marker; audio, drills, the qualifier, movement, peer clocks and replay bytes keep their gameplay behavior. Re-enabling effects does not restore cleared particles or markers.

## Rust host API and headless export

`Lab::new()` and the original `ferravex_init` / `ferravex_step` exports retain autoplay for software/headless callers. The native CLI and software renderer are unchanged:

```sh
cargo run -p owned-lab --offline --locked -- --frames 640 --output /tmp/courtyard.png
```

Interactive hosts use the shared `ferravex_core::session::GameSession` through:

```rust
use owned_lab::Lab;
use ferravex_fps::InputFrame;

let mut lab = Lab::new_interactive(); // Ready
lab.update_interactive(1.0 / 64.0, InputFrame::default(), true, 1, 16.0 / 9.0);
lab.update_interactive(30.0, InputFrame::default(), false, 1, 16.0 / 9.0); // pause offline
assert_eq!(lab.game.tick, 1); // inactive time is discarded
assert!(lab.reset_interactive()); // refuses a connected peer reset
```

`session_state()` reports the simulation lifecycle; connected matches remain Playing even when local controls are inactive. `training_progress()` returns a `TrainingProgress` snapshot with distance (capped at five meters), accepted shots, confirmed hits and completed reloads. The match tick remains authoritative for FPS rules, replay and peer packets.

`training_objectives()` returns the shared objective graph's four read-only views in move/fire/reload/hit order; movement uses centimeters out of 500. `peer_status()` returns the current transport role/freshness label. `qualifier_status()` returns a `QualifierStatus` (`QualifierPhase::Locked`, `Countdown`, `Running` or `Debrief` with ticks, plus score, streak, multiplier and finished runs), `qualifier_result()` the last `QualifierResult`, and `qualifier_best()` the personal best. `Grade::from_score` grades a score. These queries do not consume input or change game state.

Scene transforms use the checked scene hierarchy while retaining every authored object and original editor index. `edit_positions(&[(index, [x, y, z])])` validates the entire bounded batch before changing rendering, FPS collision or dynamic-world terrain. It returns `SceneEditError` for connected peers, fixed/missing objects, duplicate rows, nonfinite/out-of-range coordinates or missing geometry. The original `edit(...) -> bool` and browser ABI use the same validation. `object_by_path(&["courtyard", "object-1"])` resolves a checked literal path to the existing index. Browser scene-v1 saves keep their original rows; the browser still loads rows individually until the host adopts a batch API.

The browser uses additive `ferravex_training_init`, `ferravex_training_reset`, `ferravex_training_state`, `ferravex_training_progress` and `ferravex_training_release_input` exports. State codes are Ready=0, Playing=1, Paused=2, Won=3, Lost=4; progress bits 0–3 are movement, shot, reload and hit. Completing the drills does not finish the session; it stays Playing. Additive `ferravex_qualifier_phase` (0 locked, 1 countdown, 2 running, 3 results/free practice), `ferravex_qualifier_score`, `ferravex_qualifier_best` and `ferravex_qualifier_grade(which)` (0 last run, 1 best; 1–5 for S–D, 0 none) feed the screen-reader status and the menu's personal best. The host treats them as optional. The existing 11-float status buffer, scene rendering and shaders are unchanged; the mesh list gains three confetti colors at the end.

All existing ABI signatures are preserved. ABI state now lives in a thread-local `RefCell<Option<Lab>>`, matching the renderer, controls and audio runtime. Each calling thread initializes and resets its own scene. Returned pointers remain backed by that thread's Lab after the borrow ends; consume them on the same thread before mutation/reset/reinitialization or thread exit. Buffer-growing operations may reallocate, as before. `Lab` is thread-affine through `GameAudio`; do not transfer it between threads.

## Targeted validation

```sh
cargo test -p owned-lab --offline --locked -j 2
cargo clippy -p owned-lab --all-targets --offline --locked -j 2 -- -D warnings
node examples/owned-lab/tests/platform-session.mjs
node web/tests/journey200-owned-lab-host.mjs
```

Rust regressions cover paused world/replay/timers, reset, action buffering, training milestones, live authority/prediction and the existing software renderer. Qualifier tests drive real match events through drills, countdown, scoring, results, the bell restart and reset, check that identical input gives identical scores and replay bytes, and check that peers never score. Other tests cover streaks, grades, quip rotation and fit, reduced-motion juice and HUD layouts at desktop, narrow and compact sizes.

Debug Wasm builds (`debug_assertions`) add `ferravex_debug_aim`, `ferravex_debug_aim_bell` and `ferravex_debug_best` for scripted screenshot review. Release Wasm and native builds do not contain them. The browser test uses mocks to verify menu, capture failure, focus/visibility, settings and reset routing; it does not replace actual Wasm, WebGL or two-browser peer testing.

Journey200 coverage additionally exercises per-thread ABI/reset isolation, real controller taps across zero/multiple ticks, atomic failed scene batches, collision/render transform agreement, target hysteresis/cover, packet freshness and independent live presentation policies. Fresh Wasm export linkage, browser layout and playtesting remain parent integration checks. The accessible host's original static default-binding instructions remain alongside the live canvas hints.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
