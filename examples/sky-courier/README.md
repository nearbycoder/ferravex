# Sky Courier

A first-party Ferravex example with a shared WebGL2 browser host and native renderer entry point. See [platform support and verification](../../docs/PLATFORMS.md) for the OS libraries and device checks that apply.

A small 3D coastal flight game built with Ferravex. Pilot a low-poly floatplane through five ordered amber navigation rings, then slow down and fly low over the marked island landing pad to deliver the north coast mail before the 90-second window closes. All geometry is procedural and cached at startup; no external assets are required.

```sh
cargo run -p sky-courier --release
```

| Input | Action |
| --- | --- |
| Enter / gamepad confirm / menu button | Start, resume, or fly again |
| A / D or left / right | Turn and bank |
| W / S or up / down | Climb / dive; release to level out |
| Space | Boost |
| Shift | Slow flight; required for delivery |
| Escape / gamepad Start | Pause / resume |
| R | Restart immediately, including from pause |
| Touch left stick | Turn and climb / dive |
| Hold touch on right side | Slow flight |

These are the default bindings. In-game hints follow the current keyboard or
controller mappings, including unbound actions; **Controls / F2** shows the full
mapping when it is too long for the HUD. Default touch hints describe only the
installed left stick and right-side slow region. The default touch layout has no
boost region. Customized layouts show their configured touch channels.

Start with **Start flight**; the delivery window stays frozen on the briefing screen. Follow the amber ring. Cleared rings disappear; future rings are muted. The six-part progress strip shows the five gates and final delivery. Navigation reports turn direction, climb/descent and distance to the current target, including a turn-back prompt when a gate is behind you. Accepted gates briefly confirm progress and replenish boost.

On the delivery approach, hold Shift (or the right side on touch) and fly through the airspace above the coral cross at no more than 18 metres/second (about 35 knots), aiming for 16 metres altitude. The HUD reports **SLOW DOWN** or **SPEED READY**; the final low pass releases the parcel automatically. The countdown rounds up until the window closes and turns amber for the final 15 seconds.

Ocean, island, ceiling, and map-boundary impacts cost one of three hull points and five seconds. Surviving impacts recover you at the previous beacon with temporary collision immunity. Three impacts or running out of time ends the flight. The same recovery rules apply to human and automated play.

The HUD distinguishes accepted boost, cruise, slow flight and boost recharge,
and reports vertical speed alongside airspeed and altitude. Recovery identifies
the start or last cleared gate, the five-second penalty and protection remaining.
The result screen distinguishes hull exhaustion from timeout and reports cleared
gates, actual simulated flight time and impact penalties separately. It offers a
retry button. Retry restores the full route, hull, timer and boost, clears
camera/trail/exhaust history, and resets the engine audio. Release held steering,
climb, boost and slow controls after starting or retrying before applying them
again. Keyboard/gamepad confirmation held across a result transition must also be
released before retrying.

## Shared APIs and timing

`ferravex_core::session::GameSession` owns Ready/Playing/Paused/Won/Lost and the 120 Hz clock, capped at 12 steps per frame. Simulation-specific `Outcome` still decides delivery and failure. The host reconciles `PlayerInput` pause, focus and controls-panel state before advancing the session. Pausing freezes flight, countdown, recovery protection, gate notices, trails and the propeller; resuming does not catch up inactive time. Autoplay starts immediately, uses the ordinary flight controls and remains independent of focus for smoke runs; explicit pause still applies.

The HUD and keyboard/gamepad/touch menu buttons use the shared bounded `ui::Ui`,
`Viewport`, `slots` and `ButtonOptions` APIs. Text is formatted into the retained
UI arena; current bindings use `PlayerInput::format_hint` and fixed byte buffers.
`FlightState` now owns the motor directly, including its checked recovery reset,
forward direction and telemetry. Authored handling, route geometry, capture and
delivery-speed rules are unchanged.

The private presentation owner consumes `preferences::sync_feedback` and
`preferences::feedback_policy` every frame, including ready/pause/results and
frames with no fixed tick. `FeedbackController<16>` deduplicates accepted gate
and damage cues; damage adds a restrained camera impulse and coral screen-edge
cue. Reduced motion clears current wing trails, exhaust and camera impulses and
stops the cosmetic propeller animation. Reduced flash independently clears the
edge cue. Re-enabling a channel does not revive cleared effects, and neither
choice alters flight, penalties, input or audio cues.

The existing `Rig3` chase camera retains its authored offsets and damping, but
now smooths on presentation time between simulation ticks. It remains frozen in
local inactive sessions and cuts on recovery. Every accepted recovery explicitly
breaks wing ribbons with `Trail::break_segment` and resets emitter motion history,
even for jumps below the automatic teleport threshold. Retry clears all history.
No external module registration, export, save/replay format or dependency changes
are required. This game still has no saved progress or replay format to migrate.

## Responsive flight panel

The HUD now uses logical pixel sizes instead of fitting a 560-pixel design onto
narrow screens. The briefing, pause and results panel replaces navigation and
telemetry while inactive, with a 48-pixel flight button and complete recovery,
delivery and retry guidance. Gameplay retains ordered progress, the current target and recovery notices.
Current bindings are available in **Game menu → Controls**; the flight HUD does
not repeat a permanent key list.

The fullscreen app uses the actual viewport and hides the website toolbar and
page scroll links. The HUD keeps a small time/gate/hull badge, short navigation
cues and a 58px telemetry strip. Delivery guidance appears on approach; full
bindings and instructions are available from the game menu. The inline page
retains its scrolling links. Menus are centered cards capped at 420px wide,
without repeated binding lists or paragraphs about every state transition.

The private `flight_ui` module is registered by this game's root. Its tests cover
menu containment, actual logical-pixel button hit regions, long/remapped/unbound
hints, bounded UI command construction and unchanged flight state while queuing
menus. Task 195's receipt records the exact rebuilt browser artifacts and visual
checks separately from these source tests.

## Validation

```sh
cargo test -p sky-courier --offline --locked -j 2
cargo clippy -p sky-courier --all-targets --offline --locked -j 2 -- -D warnings
cargo check -p sky-courier --target wasm32-unknown-unknown --offline --locked -j 2
cargo run -p sky-courier -- --autoplay --smoke-frames 3000 --screenshot /tmp/sky-courier.png
```

Autoplay steers through the normal turn/climb/slow inputs; it does not teleport,
grant immunity, or change the timer. Tests cover ordered progression, complete
delivery at 30/60/144 Hz and irregular frame intervals, ready/pause timing, retry
input quarantine and clock reset, immediate crash-penalty timeout, countdown
rounding and bearing guidance, slow-flight delivery, and swept beacon capture.
Additional headless regressions cover live independent motion/flash changes,
clearing existing effects while inactive without revival, zero-tick camera
smoothing, short-recovery trail breaks, remapping/unbinding, accepted boost
status, and identical autoplay outcomes/audio cues with reduced effects.

Task 140 runs the package tests, strict targeted Clippy and owned-file formatting.
Fresh Wasm compilation, browser/GPU rendering, real controls/audio and screenshot
playtests remain parent integration checks; commands above are not claims that
those runtime checks have been completed.

This is an arcade flight slice, not an aerodynamic simulator. Collision volumes conservatively approximate island terrain; decorative houses and trees do not have separate colliders. The delivery is a low flyover rather than a wheel-contact landing. Audio is synthesized by the shared engine; there is no networking or saved progress. The restored runtime implements Linux and WebAssembly hosts; rendering and browser input require separate validation from headless tests.

## Browser player update

Open **Game menu → Settings → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
