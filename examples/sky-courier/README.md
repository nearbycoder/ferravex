# Sky Courier

A first-party Ferravex example with a shared WebGL2 browser host and native renderer entry point. See [platform support and verification](../../docs/PLATFORMS.md) for the OS libraries and device checks that apply.

A small 3D coastal flight game built with Ferravex. Pilot a low-poly floatplane through five ordered amber navigation rings, then slow down and fly low over the marked island landing pad to deliver the north coast mail before the 90-second window closes. Customers tip for style: thread rings dead centre, keep a clean streak going, and stick a gentle drop. All geometry is procedural and cached at startup; no external assets are required.

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

Start with **Take off**; the delivery window stays frozen on the briefing screen, which shows the route (five rings, then one parcel), the 1:30 window and three hull hearts as icons. Follow the amber ring. Cleared rings disappear; future rings are muted. The progress strip under the clock shows the five gates, then a parcel marker for the delivery. The heading widget's dial points at the current target (mint on course, coral when it is behind you, with a U-turn prompt), beside its distance and climb arrows (one to three heads by altitude gap, mint bars when level). A gold bracket marks the current target in view, and an edge arrow points to it when it is off-screen or behind you. Accepted gates replenish some boost.

## Tips, streaks and secrets

Every accepted gate pays a tip, scored from the line you were flying when you
entered it: within 2.5 m of the centre is a **bullseye** (+$10), within 5.5 m is
**clean** (+$5), anything else is a **scrape**. Boosting through a ring adds a
$5 "hot" bonus. Consecutive gates without an impact build a streak that
multiplies tips (×1 up to ×5); any impact resets it. The current ring turns
gold once the streak pays ×3 or more.

The delivery pass is graded too: a slow (≤ 15 m/s), centred pass is a
**bullseye drop**, then **smooth** and **bumpy**, multiplied by the streak. Every
whole second left on the window adds an early-arrival dollar.

Three golden rings hide well off the route (one skims the swell, one floats
above the clouds, one sits between the eastern islands). Each pays $50 and
tops up boost; finding all three pays a $100 scenic-route bonus. They never
affect gate order and are never marked on the HUD.

Gate passes, hidden rings and impacts get punchy rotating headlines of at most
three words ("Threaded it!", "Boat mode!", "Corner office!") over icon chips
(coin tip, bolt for a boosted pass, flame streak, clock penalty, shield ring)
that never repeat back to back. Juice comes only from accepted events: ring
sparks, rising coin and star pops, a small FOV kick, a gold edge glow on
bullseyes, a victory roll on streak milestones and hidden rings, speed lines
while fast, a recovery blink, a bursting hull heart, a flashing route segment,
a popping tip badge and confetti on delivery.

The shift card shows a silly rank (from **Pigeon in Training** to **Legend of
the Letterbox**, plus secret and failure titles), the drop grade, then icon stats
for tips, best streak, flight time, hull hearts and hidden rings, a one-line
customer reaction, your personal best (trophy) and the next rank (crown).
Undelivered flights show their tips struck out as unpaid.

On the delivery approach, hold Shift (or the right side on touch) and fly through the airspace above the coral cross at no more than 18 metres/second (about 35 knots), aiming for 16 metres altitude. While too fast, a coral prompt shows the current slow-flight key caps and **Slow**; once slow enough it becomes a mint check over the parcel, and the speed dial's mint band and the altimeter's mint tick show the targets. The final low pass releases the parcel automatically. The countdown rounds up until the window closes and turns amber for the final 15 seconds.

Ocean, island, ceiling, and map-boundary impacts cost one of three hull points and five seconds. Surviving impacts recover you at the previous beacon with temporary collision immunity. Three impacts or running out of time ends the flight. The same recovery rules apply to human and automated play.

The gauge footer distinguishes accepted boost (a glowing gold bolt), cruise,
slow flight (a mint halo on the speed dial) and boost recharge (a coral bolt
meter), and shows airspeed, altitude and vertical speed as a speed dial, an
altimeter and a climb arrow with their numbers. Recovery shows the five-second
penalty and a shield ring for the protection remaining. The result card
distinguishes hull exhaustion (empty hearts) from timeout (an expired clock) and
shows cleared gates, actual simulated flight time and impact penalties as icon
stats. It offers a retry button. Retry restores the full route, hull, timer and boost, clears
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
Reduced motion also clears and blocks ring sparks, pop rise, FOV kicks, speed
lines, ring breathing/spin, the victory roll and the results orbit; tip pops
stay readable. Reduced flash also clears the bullseye glow and replaces the
recovery blink with a steady aircraft. None of these change tips or cues.

Personal bests use the shared `storage::Records` slot (owner `sky-courier`,
record `north-coast-mail`, rules context `0x534b_5943_0001`). Delivered shifts
record their tips and flight time; failed shifts count as a run with zero tips
and no time. Autoplay never records. Storage failure leaves a playable game.
There is no replay format, and no external module registration, export or
dependency change is required.

## Responsive flight panel

The HUD now uses logical pixel sizes instead of fitting a 560-pixel design onto
narrow screens. The briefing, pause and results panel replaces navigation and
telemetry while inactive, with a 48-pixel flight button and complete recovery,
delivery and retry guidance. Gameplay retains ordered progress, the current target and recovery notices.
Current bindings are available in **Game menu → Controls**; the flight HUD does
not repeat a permanent key list.

The fullscreen app uses the actual viewport and hides the website toolbar and
page scroll links. The HUD keeps a small clock/hull/route badge (top left), a
coin and streak badge (top right), one heading widget, one transient callout, a
single fading coach prompt and a 58px gauge footer. Coach prompts (a ring icon
with "Fly through", the boost key cap with a bolt and "Hold", a ring and coin
for "Thread centre") disappear once you have done each thing and stay learned
across retries. The slow-flight prompt appears on the delivery approach; full
bindings and instructions are available from the game menu. The
inline page retains its scrolling links. Menus are cards capped at 420px wide,
placed slightly high so the aircraft stays visible, with three short rows below
the button for the customer reaction, personal best and next goal.

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

Debug native builds also read review-only capture variables (never compiled
into release or Wasm): `SKY_WINDOW=390x700`, `SKY_PAUSE_AT=N`, `SKY_CRASH_AT=N`
(boosted dive from frame N), `SKY_TOUR=1` (fly the hidden-ring route with
ordinary steering), `SKY_BOOST=1` and `SKY_FAKE_BEST=TIPS`.

Autoplay steers through the normal turn/climb/slow inputs; it does not teleport,
grant immunity, or change the timer. Tests cover ordered progression, complete
delivery at 30/60/144 Hz and irregular frame intervals, ready/pause timing, retry
input quarantine and clock reset, immediate crash-penalty timeout, countdown
rounding and bearing guidance, slow-flight delivery, and swept beacon capture.
Additional headless regressions cover live independent motion/flash changes,
clearing existing effects while inactive without revival, zero-tick camera
smoothing, short-recovery trail breaks, remapping/unbinding, accepted boost
status, and identical autoplay outcomes/audio cues with reduced effects.
Courier regressions cover tip/streak/drop arithmetic, hidden rings reached with
ordinary inputs (and never by the plain autopilot), identical ledgers across
render rates, impact causes, juice freezing in pause and obeying each policy,
non-repeating quips that fit a 320-pixel card, coach hints, and badge fit at
the narrowest supported width.

Task 140 runs the package tests, strict targeted Clippy and owned-file formatting.
Fresh Wasm compilation, browser/GPU rendering, real controls/audio and screenshot
playtests remain parent integration checks; commands above are not claims that
those runtime checks have been completed.

This is an arcade flight slice, not an aerodynamic simulator. Collision volumes conservatively approximate island terrain; decorative houses and trees do not have separate colliders. The delivery is a low flyover rather than a wheel-contact landing. Audio is synthesized by the shared engine; there is no networking, and only personal-best records are saved. The restored runtime implements Linux and WebAssembly hosts; rendering and browser input require separate validation from headless tests.

## Browser player update

Open **Game menu → Settings → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
