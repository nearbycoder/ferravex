# Salvage Pinball

Built with first-party Ferravex APIs. Native play uses Linux X11/GLX; browser play uses the shared WebGL2 host. The physics and session can also run headlessly in tests.

A warm, geometric salvage-yard table run by a grumpy (but supportive) foreman. Save three steel balls, light the bumpers, work the magnet crane and chase your personal best. Every scoring hit comes from Ferravex's original physics contact queries; a brief debounce stops a persistent contact from scoring repeatedly.

```sh
cargo run -p salvage-pinball
cargo run -p salvage-pinball -- --autoplay --smoke-frames 600 --screenshot /tmp/salvage-pinball.png
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

## How the table plays

- **Bumpers** score 100 points times the yard **multiplier** (shown on the bumper caps, the header and the 2X–5X inserts above the flippers). Hits less than 1.2 seconds apart build a **chain**; from the third hit on, each one adds an escalating bonus (+50, +100 … capped at +500) and the dot-matrix display shouts about it.
- **Magnet crane.** Each bumper you hit lights the lamp beneath it. Light all three and the crane lowers its magnet over the top lane. Send the ball under it: the crane grabs the ball, pays a **jackpot** of 2,500 times the multiplier, raises the multiplier (up to 5X) and drops the ball back toward the centre bumper. The lamps then reset for the next round.
- **Skill shot.** While a ball waits in the lane, a SKILL lamp hops between the bumpers. Launch so the lit bumper is the first one your ball touches for +1,000. The launch arc always takes the same route, so this is a timing skill you can learn.
- **Ball save.** The first launch of each life lights a five-second save (the SAVE insert above the drain) that rescues one early drain. A saved ball keeps its multiplier and needs a new launch. An unsaved drain costs a ball and resets the multiplier, lamps and crane.
- **Secret.** There is one hidden one-per-game award for an unusually patient ball. The foreman will tell you when you find it.

The armed crane ignores a ball still on its launch arc, so a re-served ball cannot collect a free jackpot. A ball returning down the launch lane can be served again with a new press. Retrying gives three balls and zero points, with a separate press to launch.

## Screens and messages

- The **header** shows only the rolling score, your stored best (or "NEW BEST PACE!" once you pass it), ball lamps, the multiplier and the 68×44 Reset target.
- One **dot-matrix display** under the table shows one message at a time: a fresh event callout (jackpot, skill shot, crane armed, chain, ball saved, drain) or, otherwise, the next objective (skill shot, ball-save countdown, bumpers lit, crane armed with its jackpot value). Callouts are prioritized, so banter never hides a jackpot. Drain, save, jackpot, pause and ready lines come from the foreman and never repeat back-to-back.
- One **hint line** below it carries the launch prompt while a ball waits and a flipper reminder that fades and retires after a few flips (it stays retired across retries).
- **Ready**, **Paused** and **Shift complete** cards sit over the table with one highlighted next action. The results card shows the score, a rank title with a quip (from GUTTER INSPECTOR to SALVAGE BARON), jackpots, best chain and skill shots, your personal-best comparison, and how many points you need for the next rank.
- Personal bests are saved locally through the shared `Records` store (one score per table rule version). Autoplay games are labelled as demos and never recorded. A storage failure leaves the game fully playable.

Layouts are checked at 1280×800, 1600×1000 and narrow portrait canvases down to 320×540. The browser table keeps a minimum 540 CSS-pixel height so narrow layouts retain a usable playfield. Card and dot-matrix copy is written to stay readable at the narrowest width; long remapped bindings fall back to "SEE CONTROLS" guidance.

## Feel and feedback

Bumper hits flash the cap, punch it outward, throw sparks, ring the rim and float a "+N" score pop; jackpots add a big pop, a spark shower, a table flash and a camera kick; drains, saves, skill shots and crane drops each have their own flash, shake and sound. Every effect is created from an accepted simulation event or contact serial. Flipper clicks use one shared audio click per accepted press; the crane arming, jackpot, skill shot, secret, crane drop, save and drain have distinct cues. Mashing the flippers earns a rate-limited remark from the foreman instead of a real tilt.

The shared **Game and player UI motion** and **Reduce game flashes** settings are independent and are read every presentation frame, including menus. Reduced motion clears and blocks bumper rings and punch, ball trails, sparks, camera shake, the crane's travel animation and the score roll (it snaps); score pops stay but no longer rise. Reduced flashes clears bumper illumination and table flashes and turns blinking lamps steady. Re-enabling an effect reveals only new contacts and trail samples. These settings leave physics, scoring, text and audio unchanged.

Pause, loss of focus and the controls panel freeze the ball, the crane hold, ball-save and chain timers, and all rule prompts. Short cosmetic effects, pops and messages settle on presentation time, including while paused. Gameplay sounds stop on pause and resume only for fresh accepted events. Resume has no time catch-up and discards buffered presses. Release controls held through pause or restart before pressing again. Autoplay starts immediately, supplies the same table controls on every physics tick, and retries 2.5 seconds after game-over; existing smoke flags are unchanged.

Debug native builds (never release or Wasm) read capture-only environment hooks for screenshots: `SALVAGE_WINDOW=WxH`, `SALVAGE_START=1`, `SALVAGE_PAUSE_FROM=N`, `SALVAGE_LAZY_BOT=1` (autoplay launches but never flips) and `SALVAGE_FAKE_BEST=SCORE`.

## Engine exercise

- `ferravex_core::session::GameSession` owns the ready/playing/paused/lost lifecycle and a 120 Hz clock capped at 16 ticks per frame. The table remains authoritative for lives, saves and scoring.
- `PinballGame::advance(elapsed_seconds, controls)` buffers render-frame input until a real tick. Quick taps survive zero-tick frames; a held launch is consumed once across catch-up ticks and cannot automatically launch a replacement ball. `Table::step` remains available for callers already supplying fixed ticks and one-tick launch commands.
- The existing shared `PlayerInput` pinball preset handles remapping and pause/focus suppression, including a registered mouse action. `current_device` and bounded `format_hint` supply HUD labels; checked `Font::try_metrics` measures fractional-size text consistently with drawing and fits overlay prompts to the panel.
- `PinballFeedback` reports only accepted simulation events (points, hits, launches, drains, saves, flipper presses, jackpots, skill shots, crane arms, crane drops, secrets and the chain count reached) for `GameAudio` and presentation; `pause`, `resume` and `reset` control owned gameplay voices. Flipper clicks use the gameplay effects route so they stop with the table. No new dependency or audio system is required.
- The renderer's shared `preferences::feedback_policy` governs the hand-drawn effects. Cosmetic state is separate from the simulation's bumper debounce, with three contact serials, a fixed six-slot pop list, a fixed 72-spark pool and fixed-size envelopes. No `FeedbackController`, duplicate preference store or game-specific preference export is introduced.
- Table rules live in `Table::step`: multiplier, bumper lamps, crane arm/catch/hold/drop, chain timer and bonus, skill-shot lamp and launch arc, ball time and the overtime secret. All of them advance only on fixed ticks, so autoplay and replays of the same controls produce the same score at 30, 60, 120 and 144 render frames per second. `Table::bumper_value` and `Table::chain_bonus` expose the current awards.
- `ferravex_render::storage::Records` stores one personal best per table rule version (`RECORD_RULES`); the rules version changed with this table's scoring, so old pre-multiplier scores are not compared.
- `Table::ball_points()` and `Table::last_ball_points()` expose the current and latest completed life's score without consuming events or awarding additional points. Saves preserve the current breakdown; restart clears both.
- Original `ferravex-physics` 2D world, fixed capsule rails, CCD dynamic ball, hinged motor-driven capsule flippers and actual contact queries.
- Powered bumpers apply an outward impulse only on physical contact. Flippers transfer momentum through Ferravex's contact solver; neither player nor bot can directly steer the ball. The only table-driven motion is the crane: after a catch it pins the ball under the magnet for 1.2 seconds, then releases it with a small alternating sideways velocity.
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

Headless regression tests cover ready/pause/no catch-up, zero-tick quick taps, a held launch during a save inside a catch-up frame, three lives and terminal freezing, reset/retry input suppression, contact-only scoring/debounce and feedback, physical flipper momentum/attack/return, and 60-second autoplay with equivalent outcomes at 30 and 120 render frames per second. Rule tests cover lamps arming the crane, jackpot value and multiplier step, the magnet hold and drop (including pause freezing the hold), the launch arc ignoring an armed crane, the skill shot only on the lit bumper, chain bonus growth/cap/lapse, save versus drain resets and the once-per-game secret. Presentation tests cover independent effect suppression/no revival (including sparks, punch and shake), new-only trail samples, bounded pop expiry, callout priority, the rate-limited mashing remark, learned hints surviving retry, non-repeating quips, rank ladder and next goals, record comparison, and copy that stays readable at a 320-pixel canvas. Policy-reducer tests supply the flags directly; fresh Wasm publication, live browser preference switching, rendering and physical-device behavior remain parent integration checks.

This is a single-table arcade slice with procedural game audio. Personal bests are stored locally; there is no online leaderboard, multiball, real tilt/nudge or networking. Powered contacts are capped at 34 m/s. Touch controls are implemented but device hardware has not been tested. The original rotational solver handles ball contacts against motor-driven capsule flippers. See `crates/ferravex-physics/README.md` for unsupported shape pairs and bounded CCD behavior.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
