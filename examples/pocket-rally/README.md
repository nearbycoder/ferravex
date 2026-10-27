# Pocket Rally

A first-party Ferravex example with a shared WebGL2 browser host and native renderer entry point. See [platform support and verification](../../docs/PLATFORMS.md) for the OS libraries and device checks that apply.

A small complete 2D rally time trial built with Ferravex. Race three laps of Valle
Verde, a procedural rural circuit, drift through the bends for style points, then
try to beat the translucent best-lap ghost and the gold medal. A cheeky co-driver
comments on your grass cutting. There are no downloaded art assets, audio files,
textures, or fonts.

```sh
cargo run --release -p pocket-rally
cargo test -p pocket-rally --offline --locked -j 2
cargo clippy -p pocket-rally --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p pocket-rally -- --autoplay --smoke-frames 3000 --screenshot /tmp/rally-finish.png
```

WASD or arrow keys steer, accelerate, and brake/reverse. Space loosens rear grip
for handbrake turns. Escape pauses/resumes; F2 opens control settings. R restarts
with a fresh three-second countdown while preserving the best lap and ghost.
After the finish, Enter or clicking/tapping also starts another race. Touching the screen
reveals left/right steering and gas/brake controls along the bottom edge. The
four default touch regions shrink to fit narrow viewports without overlapping;
saved customization still addresses the same control indexes. On-screen start,
driving and retry hints show the first current-device binding from the live
Controls profile, including remaps and unbound actions.
Default touch hints use the visible button labels (for example, **Hold GAS for
gas**) instead of internal touch channel numbers. Remapped touch actions follow
their actual button; customized layouts and unknown channels keep the shared
binding description because the default button labels are not drawn there.
Touch results say **Tap to retry**; no invisible restart/confirm touch buttons
are advertised. Keyboard and gamepad retry hints still follow live bindings.
The editor retains its **Brake / slow** label for the handbrake action. The browser help lists defaults;
Tab leaves the canvas, while normal Tab inside Controls navigates that editor.
Shift+Tab can leave the canvas even with Controls open.

Every race begins with **3, 2, 1, GO**: three start lamps light while the camera
flies in from the circuit overview to the car, and a big **GO!** stamps the start.
The first countdown also shows the live gas/drift bindings; retries show a fresh
retry quip and your best race time. Hold gas to accelerate when the countdown
ends; neither the car nor the lap/race timer advances before GO. Pause freezes
countdown, race time and transient feedback, and shows a pause card with the race
state, a quip and the live resume/restart bindings. Interactive play also suspends
for control settings and lost focus; autoplay ignores those focus interruptions. A
resumed frame discards elapsed inactive time, and restart clears skid trails,
particles, pops, banners, style points, audio travel/voices, countdown and
messages.

## Style, splits and secrets

Hold the handbrake through a bend and keep the car sideways on the road to
**drift**. A live counter rides above the car; when the slide ends the points are
banked with a rating (**TIDY**, **SPICY**, **SIDEWAYS!**, **PURE STYLE!**,
**MAXIMUM ATTITUDE!**). Drifts banked within three seconds of each other chain
up to a x5 multiplier. Sliding onto the grass **botches** the drift: the unbanked
points are lost and the chain breaks. A lap without touching grass earns **CLEAN
LAP +250**, three clean laps add **PERFECT LINE +500**, and a full 360° handbrake
spin is a **DONUT +300**. There is also a small secret on the farm; the results
card hints at it once you have gold and a perfect line.

After a best lap exists, every gate shows a green (ahead) or red (behind) split
versus that lap, and the HUD keeps the latest delta under the lap timer. The
second lap's banner announces **FINAL LAP!**. Splits for a restored saved ghost are
derived from its 20 Hz poses, so the save format is unchanged.

The finish card ranks the race with a medal (gold under 43 s, silver under 47 s,
bronze under 55 s; the handbrake-free autopilot runs about 43.8 s), a silly title
(SILVER SLIDER, FENCE INSPECTOR, AGRICULTURAL DRIVER, VALLE VERDE LEGEND…), the
personal-best comparison, a next-goal hook ("0.81S FASTER FOR GOLD!") and stats:
best lap, style (and style record), best drift, clean laps, grass time and the
secret. Human races store the best race time and best style score in the shared
`pocket-rally/records` record book; autoplay is labelled a demo and never writes.

All style scoring lives in `style.rs`, observes only accepted 120 Hz ticks and is
a pure function of them; it never changes gates, laps, timers, the saved best lap
or its ghost. Quips (`quips.rs`) rotate deterministically from race counters and
never repeat back-to-back.

The road is deliberately forgiving, but grass sharply reduces speed. The HUD bar
shows the lap counter, a large lap timer with the latest split delta, gates cleared
this lap, speed with an amber speed bar, and race time; narrow screens use a
three-row bar. Times are stopwatch text from whole centiseconds. A minimap under
the bar shows the circuit, the car, the ghost, the next gate and the best lap
(**SESSION BEST** when the save failed). Completed laps show a banner with the lap
time and **FIRST BEST**, **TIED BEST**, **NEW BEST** or signed seconds versus the
previous best. Small pale arrows pulse at the next gate. Facing or moving back
along the road shows **WRONG WAY!** with a quip. Autoplay exposes the shared
driver's reversing recovery state. These observations cannot award gates.
After a quarter-second on grass, a co-driver quip ("SCENIC ROUTE SELECTED") asks
you to follow an amber arrow pointing back toward the nearest road segment;
returning shows **BACK ON TRACK**. Recovery guidance never teleports the car, skips
a checkpoint or resets lap time. Short onboarding tips (hold gas; hold the
handbrake to drift) disappear once each lesson is learned. Sixteen ordered gates
check forward line crossings inside the road corridor; repeatedly crossing the
finish, reversing, and driving outside a gate cannot award a lap. The three-lap
result freezes the race timer.

The 120 Hz `ferravex_core::session::GameSession` drives a compact longitudinal/lateral tire model:
acceleration, braking, speed-dependent steering, rolling resistance, lateral grip,
handbrake slides, and off-road drag. These are arcade vehicle dynamics, not a claim
of realistic suspension or full rigid-body car simulation. The autoplay driver
uses pure pursuit to produce normal steering/throttle controls, never teleports,
and completes the same gates and finish condition as a human.

Best-lap poses are sampled at 20 Hz and retained across retries and saved runs; only the first
five minutes of a very slow lap are recorded. Track sampling, trees, farm buildings,
fields, and the car are generated from simple geometry. The road has 192 segments;
each rear-wheel skid trail is bounded to 256 points. Race tests are headless and cover checkpoint order,
reverse/shortcut rejection, off-road drag, bounded motion, complete autoplay races,
finish state, best-ghost preservation, countdown timing at 30/60/144 FPS,
pause/resume, recovery guidance, checkpoint HUD progress and restart feedback clearing.
The checked ghost sampler uses integer 120 Hz ticks divided into six-tick keys,
so floating-point lap-time accumulation cannot skew the 20 Hz playback cadence.
It hides missing history after the last recorded key instead of parking a ghost
at that pose; saved pose bytes and record-time authority are unchanged.
Additional headless tests cover style determinism and read-only observation,
drift chains/botches, clean laps, splits, the pond secret, donuts, quip rotation,
medals/ranks/hooks, the stopwatch format, the follow-camera bounds and reduced
motion/flash handling for juice.

## Engine exercise

Uses Ferravex's `GameSession`, rendering prelude/window configuration, shared
`demo::Demo` native smoke harness, particle pool/renderer, `ScoreChain`, the
record book and `GameAudio`. The recovered version uses only first-party runtime
APIs. The camera follows the car with velocity lookahead, stays inside the drawn
farmland and zooms further on portrait screens; the countdown flies in from the
whole-circuit overview, and the finish frames the car left of the results card on
wide screens. Pointer/touch input stays in screen space. Native Linux currently
uses physical window pixels; automatic desktop DPI scaling is not implemented.
Custom vehicle dynamics live here as game logic; they do not expand the common
engine API prematurely.

Human best laps and their ghosts use the existing typed `pocket-rally/best-lap`
save slot (native storage or browser local storage). A saved ghost is restored
only for the matching track/tuning/rules fingerprint. Restart preserves the
record; autoplay can improve its in-memory ghost but never writes the save slot.
Save failure leaves the race playable and labels the record **SESSION BEST**.
The schema, version, fingerprint and 20 Hz ghost format are unchanged.

Shared `Route3` closed-route projection replaces duplicate nearest-segment and
recovery projection code. Gate crossing remains the original ordered rule.
`RecordBook::compare` supplies presentation summaries only after an accepted lap;
the existing floating-point best-lap rule and typed save remain authoritative.
Checked bitmap font metrics size centered labels, banners and cards. The custom
tire model and authored tuning are unchanged, so saved ghosts stay valid.

Juice (`juice.rs`) is presentation only: tire smoke while drifting, grass and dirt
spray off-road, sparks and a squash when the car bumps the farm fence, a water
splash, gold bursts for a new best lap, finish confetti, rising score pops, a lap
zoom punch and camera shake. Effects are created from accepted ticks and age with
accepted simulation time (the results celebration uses render time while the race
is frozen). The shared presentation policy is read every frame, including paused
menus and zero-tick frames. Reduced motion clears and blocks skid trails,
particles, shake, squash, zoom punches and the countdown fly-in; pops and banners
stay readable but stop rising or bouncing. Reduced flash independently clears and
blocks the gold new-best edge glow. Restoring a preference never revives old
effects. No duplicate preference store or Wasm export is added.

Debug native builds read a few capture-only environment variables (window size,
pause frame, forced drifts/off-road/pond visit, fake previous best) documented in
`src/capture.rs`; release and Wasm builds do not compile them.

The shared Racer input preset also supplies gamepad bindings and remapping.
Procedural engine/skid audio plus countdown, lap, drift, split, fence-bump and
splash cues use `GameAudio`; the browser
Sound button enables audio. `GameAudio::pause` clears owned gameplay voices on
local pause/settings/focus loss; resume accepts fresh cues without replaying old
ones. Restart uses the instance's reset without stopping unrelated audio.
There is no multiplayer. Browser builds share
identical Rust simulation; native smoke flags remain unavailable in browser builds.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.

Task 133 validation is limited to native headless package tests, the existing
compiling rustdoc example, targeted strict Clippy/rustfmt, and source-only host
checks. Fresh Wasm, native/browser playtesting, portrait screenshots, physical
touch/gamepad and accessibility verification remain parent integration work.
