# Salvage Pinball

Built with first-party Ferravex APIs. Native play uses Linux X11/GLX; browser play uses the shared WebGL2 host. The physics and session can also run headlessly in tests.

A warm, geometric salvage-yard table: save three steel balls, hit powered pop bumpers, and recover the highest score you can. Every scoring hit comes from Ferravex's original physics contact queries. Each bumper awards 100 points on a new contact, with a brief debounce to avoid counting persistent contact repeatedly.

```sh
cargo run -p salvage-pinball
cargo run -p salvage-pinball -- --autoplay --smoke-frames 420 --screenshot /tmp/salvage-pinball.png
cargo test -p salvage-pinball --offline --locked -j 2
```

| Control | Action |
| --- | --- |
| A / Left arrow | Hold left flipper |
| D / Right arrow | Hold right flipper |
| Space | Start and launch; release before each subsequent serve |
| Enter | Start without launching; retry after game-over |
| Escape | Pause / resume |
| R / RESET | Start a fresh three-ball game |
| Touch / mouse | Tap to launch; hold lower left or right to flip |
| Controller | South button launches / retries; shoulders flip; Start pauses |

The controls above are defaults. In-game launch, flipper, pause and retry hints follow the current device and saved remaps, including unbound actions. Long binding lists direct you to Controls; hints never consume a gameplay press. On the browser page, Shift+Tab from the table moves to How to play without pausing; pause first when reading help.

The browser table appears before the detailed page instructions and keeps a minimum 540 CSS-pixel height so narrow layouts retain a usable playfield. Scroll the game region to read help on short screens. The score, life count and 68×44 Reset target use a separate header; save/drain and launch prompts sit below the table at 13 logical pixels instead of shrinking with the physics geometry. Ready, pause and result panels use the available canvas width. Long binding hints point to Controls at a readable size.

The three dots and ball count include the ball in play. A ball returning down the launch lane can be served again with a new press. The first launch of each life lights a five-second ball save that rescues one early drain; the HUD shows its countdown and when the save is spent. A saved ball preserves the life and needs a new launch. The **THIS BALL** total includes points earned before a save or lane return. An unsaved drain reports that life's recovered points and the remaining ball count. This is a breakdown of the score, not an additional bonus award. After the third unsaved drain, tap, press Enter or R to retry. Retrying gives three balls and zero points, with a separate press to launch.

Flippers have a stronger attack than return stroke, an active-color inset, and one shared audio click per accepted press. An accepted launch briefly shows **BALL IN PLAY**. Physical bumper hits update one combined `+N RECOVERED` notice alongside the impact sound; nearby hits share this notice instead of spawning labels around every bumper. Merely approaching a bumper never scores. Ball saves and unsaved drains have distinct prompts and cues. Controls remain remappable in the shared player settings.

Pause, loss of focus and the controls panel freeze the ball, ball-save timer and save/drain rule prompts. Short cosmetic flashes, rings and score notices settle on presentation time, including while paused. Gameplay sounds stop on pause and resume only for fresh accepted events. Resume has no time catch-up and discards buffered presses. Release controls held through pause or restart before pressing again. Autoplay starts immediately, supplies the same table controls on every physics tick, and retries two seconds after game-over; existing smoke flags are unchanged.

The shared **Game and player UI motion** and **Reduce game flashes** settings are independent. Reduced motion clears bumper rings and ball trails; reduced flashes clears bumper illumination. Preferences are consumed every presentation frame, including menus and frames with no physics tick. Re-enabling effects reveals only new contacts/trail samples. These settings leave flipper/ball physics, scoring, textual feedback and audio unchanged. There is no camera shake or added particle system.

## Engine exercise

- `ferravex_core::session::GameSession` owns the ready/playing/paused/lost lifecycle and a 120 Hz clock capped at 16 ticks per frame. The table remains authoritative for lives, saves and scoring.
- `PinballGame::advance(elapsed_seconds, controls)` buffers render-frame input until a real tick. Quick taps survive zero-tick frames; a held launch is consumed once across catch-up ticks and cannot automatically launch a replacement ball. `Table::step` remains available for callers already supplying fixed ticks and one-tick launch commands.
- The existing shared `PlayerInput` pinball preset handles remapping and pause/focus suppression, including a registered mouse action. `current_device` and bounded `format_hint` supply HUD labels; checked `Font::try_metrics` measures fractional-size text consistently with drawing and fits overlay prompts to the panel.
- `PinballFeedback` reports only accepted simulation events for `GameAudio`; `pause`, `resume` and `reset` control owned gameplay voices. Flipper clicks use the gameplay effects route so they stop with the table. No new dependency or audio system is required.
- The renderer's shared `preferences::feedback_policy` governs the existing hand-drawn effects. Cosmetic state is separate from the simulation's bumper debounce, with three contact serials and fixed-size envelopes. No `FeedbackController`, duplicate preference store or game-specific preference export is introduced.
- `Table::ball_points()` and `Table::last_ball_points()` expose the current and latest completed life's score without consuming events or awarding additional points. Saves preserve the current breakdown; restart clears both.
- Original `ferravex-physics` 2D world, fixed capsule rails, CCD dynamic ball, hinged motor-driven capsule flippers and actual contact queries.
- Powered bumpers apply an outward impulse only on physical contact. Flippers transfer momentum through Ferravex's contact solver; neither player nor bot can directly steer the ball.
- Matching geometric table art and collision geometry; no external assets, UI framework, textures, or per-frame physics object creation.
- Shared `Demo` flags support reproducible native smoke runs and screenshots. The browser build uses the same Rust game through the shared WebGL2 host.

The public controller can be used without a window:

```rust
use salvage_pinball::{Controls, PinballGame};

let mut game = PinballGame::new(false);
let events = game.advance(1.0 / 60.0, Controls {
    launch: true,
    ..Controls::default()
});
assert_eq!(events.launches, 1);
assert_eq!(game.table().lives, 3);
game.set_paused(true);
game.advance(10.0, Controls::default()); // no simulation or timer advancement
game.restart(); // fresh table, Playing session, cleared input and clock
```

Headless regression tests cover ready/pause/no catch-up, zero-tick quick taps, a held launch during a save inside a catch-up frame, three lives and terminal freezing, reset/retry input suppression, contact-only scoring/debounce and feedback, physical flipper momentum/attack/return, and 60-second autoplay with equivalent outcomes at 30 and 120 render frames per second. Additional tests exercise independent effect suppression/no revival, new-only trail samples, combined score notice expiry, per-ball score accounting across saves/drains, remapped/unbound/oversized hints, fractional text fitting and gameplay audio retirement. The controller and score-summary rustdoc examples compile with the targeted test command. Policy-reducer tests supply the flags directly; fresh Wasm publication, live browser preference switching, rendering and physical-device behavior remain parent integration checks.

This is a single-table arcade slice with procedural game audio. Scores last for the current game; there is no saved leaderboard, multiball, tilt/nudge or networking. Powered contacts are capped at 34 m/s. Touch controls are implemented but device hardware has not been tested. The original rotational solver handles ball contacts against motor-driven capsule flippers. See `crates/ferravex-physics/README.md` for unsupported shape pairs and bounded CCD behavior.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
