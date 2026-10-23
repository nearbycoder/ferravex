# Marble Run

Built with first-party Ferravex APIs. Native play uses the platform host; browser
play uses the shared WebGL2 host.

A 3D rolling-ball campaign with four hand-authored courses. Choose any course
immediately, collect all its crystals and reach the finish arch. Falling restores
the latest checkpoint and preserves crystals; the timer keeps running.
Each fresh trial begins with **3, 2, 1, GO**. The marble and sweepers wait for GO,
and only time after GO counts toward medals and records.

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
cargo test -p marble-run --all-features --offline --locked -j 2
cargo run --release -p marble-run -- --content examples/marble-run/content/clockwork.fxscene
```

Default bindings (the in-game hints follow Controls remaps and the active device):

- WASD / arrows: camera-relative roll.
- Space: brake.
- Hold Tab: see the entire course.
- R: restart the time trial.
- Escape / controller Start: pause or resume (remappable in Controls).
- C / controller X: open or close the course picker without losing the run.
- V / controller left bumper: recover to the latest checkpoint (counts as a fall).
- Touch: use the stick/brake; tap Courses, Recover and the finish-panel buttons.
- Menus: mouse/touch, arrows/Tab + Enter, or controller D-pad + A.
- F2: Controls; Escape cancels capture first, then closes the editor.
- Browser Shift+Tab: leave the canvas; use Up for backwards Controls navigation.
  Pause before leaving canvas focus if desired.

The current course's **Resume** card preserves an unfinished run, including its
countdown; R or Retry for Gold starts it over. Choosing another course starts a
new trial. The picker pauses the countdown, timer and moving obstacles. Closing
it preserves a separate manual pause. Its four-card layout also fits a 320 × 152
logical-pixel play area, with course labels sized to their cards.

The HUD shows the next checkpoint (gold marker), remaining-crystal/finish guidance,
checkpoint progress and falls, and confirms that recovery kept your crystals.
Distance is straight-line distance to the named target, not route completion.
Checkpoint splits are cumulative time since GO, captured on the first accepted
visit. Skipped checkpoints have no invented split. After completing a course,
retries compare splits against that course/content's fastest completed run in
this session; the label explicitly says **SESSION**. These references are bounded
to one per course and are not persisted. A different content version cannot use
the previous version's splits.

The finish panel compares the result with the **prior** saved/session best:
first time, tie, new best or signed time delta (negative is faster). Medal score
and fastest time stay independent. Saving failures retain the session record and
show Save Unavailable on the full-height result panel. Record schema, owner/slot
and content fingerprints are unchanged.

Recovery counts as a fall and is available after GO while playing. Keyboard/pad
edges and the touch Recover button wait for an actual 120 Hz tick; a quick tap
survives a zero-tick frame and is applied once during catch-up. Pausing or retrying
clears pending recovery. It cannot change a paused or completed run.

The marble uses an original `ferravex-physics` sphere with bounded CCD, friction,
impulses and torque. Ice uses per-platform friction; sweepers use kinematic rigid
bodies that transfer motion through real contacts. Simulation and hazard motion
pause together in the course picker or Details panel. Physics runs at 120 Hz;
render poses and sweepers interpolate between ticks. Meshes are built on course
load, including the combined striped sweeper beams. There are no downloaded
assets, textures or invisible collision rails.

`trial.rs` coordinates `ferravex_core::session::GameSession`: Ready for the initial
picker, Playing for countdown/physics, Paused for menus/focus/manual pause and Won
at the finish. Countdown ticks do not enter `Run::step`; physics still receives
the same fixed 1/120-second timestep. Inactive time never builds up into catch-up
ticks. Pausing retains the exact interpolated marble/hazard pose. Recovery clears
old look-ahead, immediately relocates the camera near the safe anchor, then uses
the shared `Rig3` damping to settle from a bounded 1.44-unit offset; it never flies
through the intervening course. Retry and content reload reset camera history.
The overview uses `FramingSettings` and transformed platform bounds to fit both
portrait and landscape views, with matching far-plane/fog distances.

Every presentation frame reads the shared renderer preference policy, including
paused, course-picker and zero-tick frames. Reduced motion removes decorative gem
bobbing/spin, camera anticipation and recovery settling, and clears old cosmetic
state so re-enabling cannot revive it. Essential marble tracking and physical
sweepers still follow the simulation. Checkpoint markers and notices are steady;
there are no flashes, particles, hit-stop or `FeedbackController` effects in this
game. The independent flash setting therefore has no additional visual channel
to suppress. Audio remains tied to accepted gameplay changes.

`sim.rs` keeps gameplay independent of rendering. Tests complete all four courses
using ordinary rolling inputs, exercise sweeper contact transfer, verify every
checkpoint is safe, and check pickups, recovery and braking. Session regressions
cover countdown pause, independent picker/manual pause, frozen feedback and
render interpolation, terminal time, recovery/retry and all-course autoplay.
Migration regressions additionally cover real `PlayerInput` recovery taps across
zero/catch-up frames, remapped hints/profile compatibility, split/record deltas,
reload rejection, recovery settling, overview geometry and independent live
motion/flash choices. These are native headless checks, not a visual playtest.
The existing release-Wasm contract covers desktop/phone course picking, settings
blocking, all four finishes and campaign progression; fresh Wasm/browser validation
of this migration remains parent integration work. Autoplay steers through the
same player physics path; it does not teleport, collect remotely or bypass collisions.

The shared content editor follows the selected course. Retry retains live edits;
switching to another course loads its embedded scene. Native `--content` watches
the supplied file until switching courses. Accepted reloads begin a fresh
countdown and reset camera/audio/input state; invalid edits keep the last valid
course; a persistent HUD message identifies a rejected edit while the last-good
course continues. Record keys and authored course formats are unchanged.

The overhead camera preserves its follow distance beside every pole instead of
treating the open course's poles as walls that should shorten its boom. Narrow
viewports use separate title/status rows and scale the touch controls to fit.
The browser's shared Details panel adjusts resolution independently of lighting;
device resolution preserves high-density phone sharpness by default. These
choices save per game. See [visual settings](../../docs/VISUALS.md).

Native Linux and browser build paths are implemented. Touch input is provided
but has not been tested on a physical mobile device.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
