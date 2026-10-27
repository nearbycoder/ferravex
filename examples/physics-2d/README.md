# 2D Physics Lab

A workshop of questionable engineering: drop balls and crates, shove them with
pulses, throw in a CHAOS burst, set up a domino run, and chase four optional
goals with a silly lab rank and personal bests. Under the toys it is still an
interactive laboratory for Ferravex's original rotational 2D solver. A procedural stack of boxes and balls sits inside fixed platforms and boundaries, alongside four jointed mechanisms. The simulation uses `ferravex_core::session::GameSession` at 120 Hz, interpolated body poses, restitution and bounded ball CCD, with a maximum of 240 dynamic bodies.

Native platform support and verification limits are documented in [Platforms](../../docs/PLATFORMS.md). Browser play uses the shared first-party WebGL2 host.

```sh
cargo run --release -p physics-2d
cargo run --release -p physics-2d -- --autoplay --smoke-frames 360 --screenshot /tmp/physics-2d.png
cargo test -p physics-2d --offline --locked -j 2
cargo check -p physics-2d --target wasm32-unknown-unknown
```

The sandbox starts running immediately. Autoplay lets the original scene settle without input; native smoke runs use `Demo::dt()` for reproducible render timing. There is no random spawning or wall-clock-driven physics.

| Action | Keyboard / pointer | Gamepad / on-screen control |
| --- | --- | --- |
| Select ball / crate | 1 / 2 | West / north face buttons; BALL / CRATE |
| Place selected shape | Click or tap the scene | South face button at the virtual cursor |
| Drop selected shape | Enter at the cursor, or above center when outside the scene | Right trigger; DROP above center |
| Place a crate without changing selection | Right click | East face button |
| Pause / resume | Space | Start; PAUSE / RESUME |
| Advance one tick while paused | N | Right shoulder; STEP |
| Radial impulse pulse | F at the cursor | Left shoulder; PULSE at center |
| Inspect a body at the cursor | I | Left trigger |
| CHAOS: throw in fourteen mixed shapes | 3 | Left stick press; CHAOS |
| Swap workshop / domino run | 4 | Right stick press; DOMINO |
| Show engine stats | G | — |
| Rebuild the current scene | R | Back / Select; RESET |

The movement keys or left stick move the virtual cursor. The south face button
also activates on-screen buttons. Current bindings and remapping are in
**Game menu → Controls** (or F2); the table above lists defaults. Buttons are
grouped make / toys / time (BALL CRATE DROP · PULSE CHAOS DOMINO · PAUSE STEP
RESET): one row on wide screens, five-and-four at medium widths and a 3×3 grid
on phones, never below a 14 px label. Hovering a button (or the virtual cursor)
shows what it does and its current binding. STEP is disabled while running,
PULSE while paused or recharging, and DROP/CHAOS at capacity.
Drop previews use the same projection and clamped placement as the scene.
Fullscreen uses the actual viewport and hides page navigation; the inline page
retains its native scrolling links.

## Toys, goals and bests

- **CHAOS** throws fourteen mixed balls and crates in from above along a
  deterministic arc. Like a drop it waits for the next running tick (a paused
  press places them frozen); the key edge is consumed only on an actual tick.
- **DOMINO** rebuilds the scene as eleven domino planks with a kicker ball that
  rolls off the left platform; RESET rebuilds whichever scene is active, and
  DOMINO again returns to the workshop. Goals and bests survive scene swaps.
- **Goals** (one at a time in the top bar, with a progress underline and an
  in-scene target line for heights): LIFTOFF (pulse something past 11 m),
  SKYSCRAPER (rest something with its top 7.5 m up), DEMOLITION (50 solid bonks
  inside one simulated second) and DOMINO RUN (topple all eleven). Each grants
  a rank, from LAB INTERN to PROFESSOR OF CHAOS, and all four bring up a LAB
  CERTIFIED card with session stats, stored bests and a next-goal hook.
- **Secret:** launch something past 20 m (hint: chain pulses).
- **Personal bests** (tallest tower, highest flight, bonks per second and the
  fastest domino run) persist through the shared records store. Autoplay never
  writes a best, and a first visit never claims to have beaten one.
- Bodies that leave the lab below −30 m are returned above the floor.

Toys, goals and callouts are pure functions of accepted fixed ticks. Sparks,
bonks and collapse detection read the solver's observation-only impact log
(`set_impact_settings`), which does not change physics; the authored scene's
first 2.5 seconds of settling do not count as demolition unless CHAOS is used.

## Feel and feedback

Accepted impacts throw sparks and briefly flash the bodies involved, new bodies
pop in with a puff, pulses burst and kick the camera, and a big pile-up (a
weighted rumble of impacts) gets a "TIMBER!"-style callout with a short zoom
toward the collapse and cinematic letterbox bars. That beat is a camera effect
only; the simulation keeps its fixed 120 Hz tick. Short rotating quips never
repeat back to back. Onboarding hints fade once you have dropped and pulsed
(or after 30 seconds); on tall narrow screens a HOW TO LAB card uses the sky
above the lab, which also shows a faint grid for launched bodies.

Press **I** over a body to pin its inspection. The steady white outline and footer identify its scene number, shape, awake/asleep/pending-wake state, speed and last-tick contact count. Inspection is read-only and does not wake bodies. A miss clears selection; successful drops select the new body. Reset clears body selection while preserving the shape chosen for future drops. The scene and inspection remain frozen until STEP or resume.

Pausing freezes body motion, simulation ticks and physics/audio cooldowns. You can still select and place shapes while paused; they remain still until you resume or step. Each **N / STEP** press advances exactly one 1/120-second tick and leaves the session paused; holding a button does not continuously step. STEP while running adds no extra tick. Running drop/pulse edges are consumed on actual fixed ticks, so a zero-tick render frame retains a quick tap and catch-up ticks do not repeat it. UI drop/pulse requests each have one pending slot; repeated requests before a tick coalesce. Paused drops remain immediate editor operations. Pulse is available only while running and has a 0.65-second simulation-time cooldown; pulses wake bodies and pass through the normal solver.

In the workshop, reset restores the initial 34 dynamic bodies and joints (the domino run rebuilds its eleven planks, kicker ball and joints), clears accumulated time, cooldowns, pending commands, body selection, pulse visuals and frame statistics, and preserves the selected shape and pause state. Reset (and a scene swap, which is a rebuild) takes priority over other inputs in the same frame, so a pending drop or CHAOS cannot leak into the rebuilt scene. The body cap rejects further drops with a visible explanation. Closing Controls/Details or regaining focus preserves a manually paused session; inactive time is not accumulated. Focus/control suspension and pause transitions discard stale queued commands.

The renderer and built-in glyphs are first-party, with no external assets. The existing native `physics_2d::run()` entry point and browser exports are unchanged. The private lab controller uses checked session construction and `GameSession::step_paused()`, retaining the fractional clock remainder without toggling lifecycle state. Rendering uses the current body pose while paused, so single steps are visible immediately. HUD sizing uses checked bitmap measurements and buttons use the shared enabled/selected styling.

Impact audio now requires an accepted solid contact start and a sufficient velocity change; gravity, joint correction alone and sensors cannot manufacture a collision cue. The pulse cue follows an accepted impulse. Audio remains on the game's creating thread through `GameAudio`.

Every presentation frame reads the shared `render::preferences` policy, including paused, Controls/Details, focus-suspended and zero-tick frames. Either reduced motion or reduced flashes clears the expanding pulse ring immediately and suppresses new rings; returning to standard settings cannot revive an old ring. Intensity scales the ring's opacity. Its cosmetic lifetime is separate from the physics cooldown and can finish while paused. Reduced motion also clears and blocks sparks, shake, spawn pop-ins and the collapse zoom/letterbox; reduced flashes clears and blocks hit flashes. Callout text stays readable but no longer rises. Sparks and flashes age with simulation time, so pausing freezes them for inspection; text and camera effects age with bounded render time. Body motion, impulses, audio, inspection outlines and fixed-tick progression are unchanged by these preferences. The lab adds no preference store or export.

This sandbox has no win condition. Validate contacts, visible rotations, stable resting bodies, spawn limits and pause/reset. It does not establish general physics accuracy: supported shapes, joints and CCD limits are documented in the physics crate README. See the `ferravex-physics` README for the solver's precise scope.

Debug builds (never release or Wasm) accept review hooks for screenshots:
`PHYSICS_LAB_WINDOW=390x720`, `PHYSICS_LAB_FOCUS=1`, `PHYSICS_LAB_GOALS=N` and
`PHYSICS_LAB_SCRIPT="100:chaos,200:pulse@0;1,300:domino"`; see `src/capture.rs`.

Toy tests cover goal calibration (no goal, collapse or flight from the untouched
settle; pulses reach LIFTOFF, two stacked crates reach SKYSCRAPER, CHAOS reaches
DEMOLITION, the domino run topples all eleven), deterministic and bounded CHAOS,
tick-only CHAOS edges, scene toggles, the lost-and-found net, the secret,
policy clearing without revival, pause-frozen effects, quip rotation, records
that never come from autoplay, onboarding fade and grouped button readability.

Regression tests cover pause/resume without catch-up, single stepping and resumed fractional time, reset precedence and restored state, zero/catch-up tick command consumption, paused drops and shape overrides, body bounds/capacity, pulse gating/cooldowns, real-contact feedback, compound queries and truncation, read-only/stale selection, remapped hints, preference transitions without revived effects, and equal settling results at 30/60/144 Hz render rates on the tested native target. Existing solver/constraint tests remain. These checks do not claim cross-platform bitwise determinism or replace fresh Wasm/browser/native visual verification, which remains with the integration owner.

## Browser player update

Open **Game menu → Settings → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
