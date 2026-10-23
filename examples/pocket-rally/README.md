# Pocket Rally

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A small complete 2D rally time trial built with Ferravex. Race three laps of Valle
Verde, a procedural rural circuit, then try to beat the translucent best-lap ghost.
There are no downloaded art assets, audio files, textures, or fonts.

```sh
cargo run --release -p pocket-rally
cargo test -p pocket-rally --offline --locked -j 2
cargo clippy -p pocket-rally --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p pocket-rally -- --autoplay --smoke-frames 6000 --screenshot /tmp/rally-finish.png
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

Every race begins with **3, 2, 1, GO**. Hold gas to accelerate when the countdown
ends; neither the car nor the lap/race timer advances before GO. Pause freezes
countdown, race time and transient feedback. Interactive play also suspends for
control settings and lost focus; autoplay ignores those focus interruptions. A
resumed frame discards elapsed inactive time, and restart clears skid trails,
audio travel/voices, countdown and messages. The overview camera is reconstructed
from the viewport every frame and retains no smoothing or shake state.

The road is deliberately forgiving, but grass sharply reduces speed. The HUD
shows lap and race time separately, gates cleared this lap, the next gate and
completed lap times with **FIRST BEST**, **TIED BEST**, or signed seconds versus
the previous best. A negative delta is faster; a positive delta is slower. Lap
time and comparison use separate banner rows, including on portrait screens.
The finish panel retains the final lap comparison. Small pale arrows mark the
next gate. A bounded distance lookahead shows **LEFT BEND** / **RIGHT BEND**;
facing or moving back along the road shows **WRONG WAY**. Autoplay exposes the
shared driver's reversing recovery state. These observations cannot award gates.
After a quarter-second on grass, an amber arrow points back toward the nearest
road segment; returning shows **BACK ON TRACK**. Recovery guidance never teleports
the car, skips a checkpoint or resets lap time. Sixteen ordered gates check forward line crossings
inside the road corridor; repeatedly crossing the finish, reversing, and driving
outside a gate cannot award a lap. The three-lap result freezes the race timer.

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

## Engine exercise

Uses Ferravex's `GameSession`, rendering prelude/window configuration, and shared
`demo::Demo` native smoke harness. The recovered version uses only first-party runtime APIs.
The game uses a full-window camera projection so track aspect and pointer/touch
coordinates share the same presentation transform. Native Linux currently uses
physical window pixels; automatic desktop DPI scaling is not implemented. Custom vehicle dynamics live here as
game logic; they do not expand the common engine API prematurely.

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
Checked bitmap font metrics size centered labels and comparison banners. The
custom tire model, authored tuning and static overview camera are retained;
no perspective/follow camera or replacement vehicle simulation is introduced.

The shared renderer presentation policy is read before accepted feedback on
every frame, including paused menus and frames with zero simulation ticks.
Reduced motion immediately clears skid trails and suppresses new trails;
restoring motion only allows fresh trails. The car, gameplay ghost and guidance
continue normally. Reduced flash remains independent: this game has no screen
flashes, camera shake or hit-stop to suppress. No duplicate preference store,
Wasm export or unnecessary `FeedbackController` is added.

The shared Racer input preset also supplies gamepad bindings and remapping.
Procedural engine/skid audio and countdown/lap cues use `GameAudio`; the browser
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
