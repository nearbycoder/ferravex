# Orbit Breaker

A playable original 2D paddle-and-ball game built exclusively with Ferravex's first-party crates and Rust std. Clear two fields of 35 destructible panels: sector one, **Low Orbit** ("mind the space junk"), and sector two, **The Armored Belt**, whose reinforced panels take two impacts. Three misses end the run. Paddle contact position controls the return angle.

## What makes it fun

- **Three capsules.** Panel clears drop capsules in a fixed, replayable rotation: turquoise **Wide** (nine seconds of 55% more paddle), violet **Multi-ball** (two free bonus balls burst from the paddle; losing them never costs a life, and if your main ball drains a bonus ball takes over) and green **Sticky** (the next three returns catch the ball; it rides the paddle with an aim preview until you press launch or 0.9 seconds pass).
- **Volleys.** Panel hits since any ball last touched the paddle form a volley. Milestones at 3, 5, 8, 12, 16, 20 and 25 hits get escalating callouts ("TRIPLE TAP!" through "PLEASE STOP. IT'S TOO POWERFUL."), the ball trail heats from cyan to gold, and in the browser each panel chime climbs in pitch.
- **Bonuses.** Clear a sector without losing a ball for a **+1000 flawless** bonus. The last panel of a sector gets a pulsing spotlight and a "LAST ONE!" callout. A small secret: a satellite drifts through the gap above the panels every 24 seconds of play (from 12 seconds in); punch a hole through the panels and bonk it for **+1000**. The ball flies straight through, so collisions are unchanged.
- **Juice.** Panels shatter into tumbling shards in their own color, scores pop and rise from the panel that earned them, the paddle squashes on contact, big moments add shockwave rings and a small camera kick, and a lost ball flashes a red edge vignette.
- **Personality.** Rotating, never-repeating quips for lost balls ("GRAVITY 1, PADDLE 0"), ball saves, retries and pause, plus sector taglines on the ready card.
- **Results card.** Win or lose, you get a silly title ("SPACE TOURIST", "ORBIT INTERN", "GRAVITY TAMER", "COMET WRANGLER", "THE ORBIT BREAKER", rarer feats like "UNTOUCHABLE" and "SPACE JUNK ARTIST"), the score, a stats grid (time, panels, best volley, best precise streak, ball saves, satellites, flawless sectors, balls lost), your personal best or a **NEW PERSONAL BEST!** banner, the points or panels still needed for the next title, and a teaser if you have not found the secret yet. The HUD says **NEW BEST PACE!** once a run passes your best.

The browser keeps the personal best on this device with the shared `ferravex_render::storage::Records` (owner `owned-orbit`, record `campaign`); storage failures leave the game fully playable. The native CLI is headless and session-only.

The ready card shows the sector or a lost-ball quip, serve charge, the current launch binding and an onboarding line: steering bindings until you have steered for about a second, then a full-charge tip until you have fired one, then a reminder about capsules and the secret. These learned hints survive restarts within a session.

- **A/D or arrow keys:** move
- **Mouse/touch drag:** steer in the browser
- **Space, Enter, or tap:** launch/retry
- **Escape:** pause in the browser
- **R:** restart

The legacy Wasm API, interactive browser adapter and native CLI all advance through `Orbit` and Ferravex's `GameSession` at 120 Hz, with at most twelve catch-up steps per frame. The session begins `Playing` while the game phase is `Ready`: steering and serve charging must run before launch. Pause freezes gameplay acknowledgements and the sector countdown; presentation envelopes can settle while paused. Win/loss finishes the session. Restart clears clock debt, pending launch input, score, presentation and world state, then readies a fresh ball. Retry and launch require separate presses, even during catch-up.

Every ball displacement uses `ferravex-dynamics::PhysicsWorld<2>::cast_ball` for continuous circle/box collision against the paddle, walls, and panels; the game applies its arcade reflection rule and bounds consecutive impacts. Particles use `ferravex-play::ParticlePool`, capsule pulses use `SpriteClip`, and press/release edges pass through `ferravex-engine::ActionMap`. The interactive adapter samples each edge once per frame; `Orbit` retains launch until a fixed tick consumes it. Held buttons cannot relaunch after a miss or repeatedly restart. Pause, focus loss and controls panels discard stale launch requests.

All game visuals—including the original bitmap lettering—are rasterized into an RGBA framebuffer in Rust. The browser presents this with `CanvasRenderingContext2D.putImageData`; it contains no game rules, rendering library, or JavaScript dependency. The CPU raster is a portable 2D reference path, not a GPU renderer. No external assets are included; the interactive adapter uses first-party synthesized audio cues.

The HUD keeps only what matters now: score (with a chain multiplier badge when it is above x1) and your best, sector name, a row of active-state chips (armor piercing, wide-paddle seconds, multi-ball count, sticky catches or the launch-to-fire binding), the live volley count, lives, panels left (highlighted at one), a draining ball-save bar, the precise-return streak, and pause/restart bindings when nothing else needs that corner. Callouts appear in the open space above the paddle with short fades. Clearing sector one shows a countdown card that names the next sector, explains reinforced panels and announces a flawless bonus. Pause also appears inside the software framebuffer with run status, a quip and resume/restart bindings. Browser pause retains the explicit Resume button and accessible status text, and refocusing the tab never resumes automatically.

Browser framebuffer hints use `PlayerInput::format_hint` after sampling, so remaps, unbound actions and device changes are reflected even in menus. Long binding lists point to Controls or fit within the panel using the shared checked bitmap metrics; authored bitmap glyphs and the CPU renderer are retained. The controls above are defaults. F2 opens Controls; Shift+Tab from the canvas reaches HTML help, as described by the existing host.

Every `Orbit::advance_input` and `Orbit::render` consumes the shared `render::preferences` policy, including paused, terminal and zero-tick frames. Reduced motion clears/suppresses cosmetic particles, ball trails and paddle hit emphasis, and stops decorative capsule pulses. Reduced flash independently clears/suppresses panel flashes; damaged-panel cracks, charge meters, falling pickups and the ball remain readable. Zero visual intensity retires both channels. Re-enabling cannot revive cleared effects. These settings do not change collisions, input, charge, scores, rewards or simulation time. The host publishes choices through the existing common `ferravex_feedback_preferences` ABI; this game adds no export or preference store.

Accepted game events feed the shared bounded `EventQueue` and `FeedbackController`, replacing the browser's frame-total audio inference. Orbit drains events after every fixed tick and coalesces repeated cue categories into at most five cues per presentation frame. Distinct catch-up events remain audible; render-only frames never replay them, and restart discards pending cues. Standalone `Game` use retains at most 32 unconsumed presentation events. Presentation-only detail (where a panel broke, volley milestones, pickups, drains) rides the same queue but never adds cues.

All juice lives in `src/juice.rs`, is created only from those accepted events and is aged by fixed simulation ticks, so pause and terminal screens freeze it (only the victory confetti runs on presentation time once a run is won); nothing in it can change collisions, input, scoring or time. The camera kick moves only the playfield layer, never the HUD, and there is no hit-stop. Reduced motion clears and blocks camera kick, paddle squash, shockwave rings, shards and victory confetti, and keeps score pops and callouts still; reduced flash clears and blocks the drain vignette and bonus glow. Text stays because it is information. Quips, titles and next goals in `src/quips.rs` are pure functions of accepted counters, so the same run always reads the same way and consecutive quips never repeat.

```sh
cargo test -p owned-orbit --offline --locked -j 2
cargo clippy -p owned-orbit --all-targets --offline --locked -j 2 -- -D warnings
node examples/owned-orbit/tests/platform-input.mjs
cargo run --release -p owned-orbit --offline --locked -j 2 -- --autoplay --frames 1800 --screenshot /tmp/orbit.png
# Debug builds only: review hooks for specific screens (see src/main.rs).
ORBIT_UNTIL=multi ORBIT_FAKE_BEST=9000 cargo run -p owned-orbit --offline --locked -j 2 -- --autoplay --frames 72000 --screenshot /tmp/orbit-multi.png
cargo build --release --target wasm32-unknown-unknown -p owned-orbit --lib --offline --locked -j 2
```

The native CLI is a deterministic headless runner and PNG exporter. Frame counts request 120 Hz simulation steps; simulation stops advancing at victory or loss. Interactive play is in the browser host at `/owned-orbit/`.

To stage the browser build, copy `target/wasm32-unknown-unknown/release/owned_orbit.wasm` alongside `web/owned-orbit/index.html` and `platform.js`, preserving the filename, and serve that directory over HTTP. The workspace build script can perform this copy.

The WebAssembly ABI exports `init()`, `step(dt: f64, axis: f32, buttons: u32)`, `pixels()`, `width()`, `height()`, `status()`, `score()`, `lives()`, `sector()`, and `paddle()`. Button bit 0 is launch/retry; bit 1 is reset. RGBA bytes remain owned by the module and are read immediately after `step`. Status codes: 0 ready, 1 playing, 2 sector transition, 3 victory, 4 loss.

The browser retains `orbit_controls_frame(dt: f32, width: u32, height: u32)`, `orbit_pause(value: u32)`, `orbit_paused() -> u32` and `orbit_restart()` alongside the shared input/audio exports. Both restart paths resume local play and leave the ball unlaunched.

Narrow playfields also show native DOM status at 16 CSS pixels below the original
640×720 canvas. Scale-1 bitmap glyphs are only about 3.3 CSS pixels high when the
canvas is 301 pixels wide; the DOM readout keeps panel counts, precision/best
streak, ball saves, charge, active bonuses and current launch/retry bindings
readable without changing the 8:9 world or desktop bitmap design. It wraps and
scrolls in normal page flow. The stable phase/score/current-action line remains
the polite live region; frequently changing charge/bonus details use
`aria-live="off"`. Bindings come from the same sampled `PlayerInput` hints as the
canvas, including remaps, unbound actions and the Controls fallback.

The additive `Orbit::status_text_v1(&mut self) -> &str` creates this presentation
without advancing simulation, sampling input or consuming feedback. The custom
browser reads `orbit_status_text_v1() -> *const u8`, then
`orbit_status_text_v1_len() -> u32`, and immediately copies the module-owned UTF-8
bytes before any other mutating game call. Version 1 is at most 2048 bytes with
no NUL terminator: the first newline separates the stable live summary from
visual detail. Before init the pointer is null and length zero. Oversized copy
becomes a complete error hint rather than partial instructions. The host checks
bounds and UTF-8, exposes a decode error in `data-orbit-status-text-error`, and
uses a generic Controls instruction if this optional ABI is unavailable; it
never guesses a remapped key. All earlier exports retain their signatures.

For Rust hosts, `advance(dt, axis, buttons)` takes held button levels. The additive `advance_input(dt: f64, input: game::Input)` takes already sampled press edges, with launch buffered across zero-tick frames and consumed once. `session_state() -> SessionState`, `set_paused(bool) -> bool` and `restart()` expose the local lifecycle. `game::Game` remains usable independently through `tick`; `feedback() -> Option<Feedback>` and `transition_remaining() -> f32` support presentation without putting rules in the host.

Hosts that keep records call `set_personal_best(Option<u32>)` before or during a run, read `personal_best()`, and record each finished run once from `take_finished_run() -> Option<FinishedRun>` (`won`, `score`, `seconds`, `new_best`). `Game::satellite()` reports where the bonus satellite is passing, if anywhere.

```rust
use owned_orbit::{Orbit, game::Input};

let mut orbit = Orbit::new();
orbit.advance_input(0.0, Input { launch: true, ..Input::default() });
orbit.advance_input(1.0 / 120.0, Input::default()); // consumes the buffered tap
orbit.set_paused(true);
orbit.advance_input(5.0, Input::default()); // no accumulated pause time
orbit.restart(); // fresh ready ball, cleared timing and input
```

Regression tests also cover the capsule rotation and replay, multi-ball hand-over without life loss, sticky catch/ride/release, satellite bonks once per pass without deflection, flawless bonuses, volley milestones, last-panel announcement, results/personal-best comparison recorded once, juice freezing on pause, learned hints surviving restart, reduced-motion camera kick, never-repeating quips, rank tiers and readable prose.

Regression tests cover high-speed CCD (12,000 pixels/second), full two-sector victory through both simulation and session autoplay, loss/restart, pickup expiry, opaque RGBA output, 30/60/144 Hz ready-state timing, quick taps, catch-up, terminal freezing and pause/retry input cleanup. Migration regressions compare the full autoplay run with effects enabled/disabled, exercise live independent policy changes while paused, effect retirement and re-enabling, accepted cue batching, remapped/unbound/oversized hints, label bounds and static reduced-motion capsule rendering. The browser adapter is also compiled by native unit tests; that is not a Wasm export/linkage test. The Node host test covers input routing, accessible phase feedback, blur/visibility pause and elapsed-time rebasing on resume/restart. Fresh Wasm and browser visual/audio validation remain parent integration checks; host mocks and framebuffer assertions do not establish them.

Successive panel hits within four seconds build a volley multiplier every three hits, capped at x4. Paddle returns and misses break the chain. The input harness now supplies the performance clock required by the host’s existing diagnostics.

Panel clears roll a seeded 25% capsule reward with a guarantee by the sixth eligible clear; capsule kinds then follow the fixed Wide, Multi-ball, Sticky, Multi-ball, Wide, Sticky rotation. The shared reward table and pity counter bound unlucky droughts while preserving replayable runs; drop storage remains capped at eight.

Capsules within 100 pixels bend toward the paddle using the shared magnet rule. This rewards steering toward a drop without requiring a pixel-perfect catch; distant or already-missed drops keep falling.

Each real launch has one 2.5-second ball save, shown in the HUD. An early drain returns the ball above the paddle without spending a life; the rescue consumes the window immediately. Later misses retain the normal three-life loss rule.

While ready, a serve charges automatically over 1.5 seconds. Launch immediately for normal pace or wait for up to 20% more speed. A full charge breaks reinforced panels in one hit until the first paddle return. The ready overlay shows charge; the same tap-to-launch controls work on every host.

Center returns within 18% of the paddle center earn 50/100/150/200 bonus points for consecutive precise volleys. A confirmed panel hit must occur between rewarded returns, preventing empty-bounce farming. Misses reset the streak; larger particles and the HUD show successful precision. The reusable rule is `gameplay::skill::PrecisionReturns`.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
