# 3D Physics Lab

Loading-dock bowling: orbit the yard, roll a heavy ball at the big crate stack,
throw in a CHAOS pile, and chase four optional goals with a yard rank, strike
calls and personal bests. Underneath it is still a low-poly loading yard for testing Ferravex's original rotational 3D solver. Boxes stack inside a fixed floor and curbs while spheres drop into the scene. `ferravex_core::session::GameSession` runs physics at 120 Hz with at most eight catch-up ticks per frame. Interpolated positions/rotations drive cached flat-normal meshes, directional vertex light and distance fog. The sandbox starts running immediately and is bounded to 180 dynamic bodies.

Native platform support and verification limits are documented in [Platforms](../../docs/PLATFORMS.md). Browser play uses the shared first-party WebGL2 host.

```sh
cargo run -p physics-3d --release --offline --locked --jobs 2
cargo run -p physics-3d --release --offline --locked --jobs 2 -- --autoplay --smoke-frames 360 --screenshot /tmp/physics-3d.png
cargo test -p physics-3d --offline --locked --jobs 2
cargo check -p physics-3d --offline --locked --jobs 2 --target wasm32-unknown-unknown
```

Drag with the left mouse button or touch the scene to orbit; arrows/WASD and gamepad sticks also orbit. Use the wheel or gamepad triggers to zoom. The camera remains usable while paused.

| Action | Keyboard | Gamepad | Footer |
| --- | --- | --- | --- |
| Drop crate / ball and select it for repeat drops | 1 / 2 | South / East | CRATE / BALL |
| Drop the selected shape again | Enter | West | DROP |
| Radial impulse | F | Left shoulder | PULSE |
| Pause / resume | Space | Start | PAUSE / RESUME |
| Advance one tick while paused | N | Right shoulder | STEP |
| Restore the default orbit without changing physics | C | North | VIEW |
| Rebuild yard, restore view, resume simulation | R | Select / Back | RESET |
| Bowl a heavy ball at the stack from the camera's side | B | Left stick press | BOWL |
| CHAOS: throw in sixteen mixed shapes | 3 | Right stick press | CHAOS |
| Show engine stats | G | — | — |

These are default bindings. Change them in **Game menu → Controls** (or F2).
The footer shows state, body count and ten grouped buttons (CRATE BALL DROP ·
PULSE BOWL CHAOS · PAUSE STEP VIEW RESET): one row on wide screens, two at
medium widths and three or four on phones, never below a 14 px label. Hovering
a button explains it with its current binding. Shape buttons highlight the
selected repeat-drop shape. STEP is disabled while running, PULSE while paused
or recharging, and BOWL/CHAOS at capacity. Rejected actions show short
feedback. Per-frame query, timing and sleep counters live in the optional
**G** engine stats panel (tick, awake/asleep, contacts, the camera ray probe's
hits and nearest distance, pulse readiness, bonks per second and toppled stack
crates), so the playfield stays clean.

## Toys, goals and bests

- **BOWL** rolls a dense ball (radius 0.58 m, density 10) from the camera's
  side of the yard on a shallow lob that arrives around the stack's second
  layer. Like drops it is an immediate edit, so a paused bowl waits frozen.
  Five simulated seconds later it gets a call: GUTTER BALL, A LITTLE NUDGE,
  SPARE-ISH, STRIKE! (8+ crates) or TURKEY! MEGA STRIKE! (20+).
- **CHAOS** drops a deterministic swirling ring of sixteen crates and balls.
- **Goals**, one at a time in the top bar: STRIKE (topple 10 stack crates with a
  bowl), AIRTIME (pulse something 7 m up), AVALANCHE (50 solid bonks in one
  simulated second) and CLEAN SWEEP (topple all 30 stack crates; orbit and bowl
  from new angles). Ranks run from YARD TRAINEE to YARDMASTER GENERAL, and all
  four bring up a YARD CERTIFIED card with session stats, stored bests and a
  next-goal hook.
- **Secret:** rest something on top of a corner pedestal.
- **Personal bests**: fewest bowls for a clean sweep, highest airtime and bonks
  per second, in the shared records store. Autoplay never writes a best and a
  first visit never claims to have beaten one. RESET rebuilds the stack and its
  bowl count, so "sweep it in fewer bowls" is always one more try away.
- Bodies that fall below −20 m are returned above the yard.

Goals, calls and callouts are pure functions of accepted fixed ticks; sparks,
bonks and collapse detection read the solver's observation-only impact log.
The untouched yard's settling never counts, and the first 2.5 simulated
seconds only count after a bowl or CHAOS.

## Feel and feedback

Accepted impacts throw sparks and flash crate frames, new bodies arrive with a
puff, pulses burst and shake the camera, and a big pile-up gets a short camera
push-in toward the collapse with cinematic letterbox bars plus a rotating
callout. These are camera and overlay effects only: the simulation keeps its
fixed 120 Hz tick and the ray probe samples the steady orbit. Onboarding text
in the footer fades after you have bowled and pulsed, or after 30 seconds.

Pausing discards elapsed inactive time. Drops while paused appear immediately but remain frozen until a manual step or resume. Each press of N/STEP calls `GameSession::step_paused`, advances exactly 1/120 second, increments the shared session tick and remains paused. Fractional time from before pausing is preserved for resume. Pausing and manual stepping synchronize interpolation history, so resuming before the next tick does not rewind the inspected pose. The keyboard step action while running gives a pause prompt. RESET restores the original 39 bodies, selected crate, camera, clock, impact cooldown and pulse state, then resumes. Reset wins over other actions in the same frame. VIEW restores only the camera, with an immediate camera cut instead of smoothing from the previous view.

The original automatic falling/settling scene and `--autoplay` launch remain available. Smoke runs now use `Demo::dt()` for both camera and fixed simulation timing, making their physics independent of rendering speed. No additional periodic input is injected. Opening Controls or losing focus gates simulation and input without resuming a manually paused session.

All geometry and text are generated by first-party code. The immediate renderer has no shadow-map pass.

This sandbox has no win condition. Check stacking, angular response, ball contacts, body limits, reset/pause and camera behavior. Its solver is a bounded original implementation with documented shape, joint and CCD limits; see the `ferravex-physics` README rather than assuming production physics coverage.

Press F, the gamepad left shoulder or PULSE for a mass-normalized radial kick, with a 0.65-second simulation cooldown. Pulses wake bodies and pass through the normal physics solver. Pulse taps wait for an actual running tick, survive zero-tick frames and fire once through catch-up; reset, pause and input-context transitions discard pending requests. Shape drops remain immediate inspector edits, including while paused. Impact sounds now require an accepted solid contact-start event and sufficient normal relative speed; free fall and joint corrections alone do not trigger them.

The shared `render::preferences` policy is read every presentation frame, including Controls, paused and zero-tick frames, before feedback and camera sampling. Either reduced motion or reduced flashes suppresses and clears the expanding pulse ring; re-enabling cannot revive an old ring. Its presentation lifetime expires independently of the physics cooldown. Reduced motion also cuts camera smoothing, and clears and blocks sparks, shake and the collapse push-in/letterbox, while retaining deliberate orbit/zoom input. Reduced flashes clears and blocks crate hit flashes. Callout text stays readable without rising. Sparks and flashes age with simulation time (pausing freezes them); text and camera effects age with bounded render time. Motion and flash choices remain independent, and neither changes physics. Camera inputs reject nonfinite values, bound pitch/distance and wrap yaw; presentation deltas are bounded separately from fixed simulation time.

Debug builds (never release or Wasm) accept review hooks for screenshots:
`PHYSICS_LAB_WINDOW=390x720`, `PHYSICS_LAB_FOCUS=1`, `PHYSICS_LAB_GOALS=N` and
`PHYSICS_LAB_SCRIPT="200:bowl,520:yaw@2.25,521:bowl"`; see `src/capture.rs`.

Toy tests cover goal calibration (an untouched yard completes nothing; one
bowl topples 10+ crates; a pulse reaches 7 m; CHAOS reaches 50 bonks and a
collapse; three orbiting bowls with pulses clean-sweep the stack), deterministic
bounded bowl/CHAOS edits, the pedestal secret, lost-and-found, policy clearing
without revival, single bowl calls, quip rotation, records never written by
autoplay, onboarding fade and readable ten-button layouts.

Targeted tests exercise pause/no catch-up, shared manual tick accounting and fractional resume, real PlayerInput controller taps over zero/catch-up/menu frames, shape selection and capacity rejection, reset precedence, contact-only sound, bounded ray queries and immediate query edits, live sleep/contact counts, independent preference suppression without effect revival, camera bounds/rate behavior, remapped hints, fixed physics at supplied render rates, responsive button bounds and the existing solver/terrain/joint checks. Native graphics, browser rendering and Wasm behavior require separate platform validation; headless unit tests do not establish those results.

## Browser player update

Open **Game menu → Settings → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.

Browser focus follows the host guidance: focus the canvas before game keys; Tab/Shift+Tab navigate the page while Controls is closed. Inside Controls, those keys navigate the editor; Escape cancels capture or closes it. The host descriptions list defaults, while renderer hints reflect remapping. Journey200 migration checks are native/headless; fresh Wasm linkage, GPU/browser layout and physical-input playtesting remain parent integration work.

## Narrow browser layout

Below 540 canvas pixels, the camera fits the authored mechanisms into the
same scene band used for orbit and zoom input. The band begins below the compact
44px header and ends at the action dock. Eye, target, depth, fog, simulation and
relative input sensitivity are preserved. The browser's fullscreen app fills the
viewport and hides scrolling strips. Inline page navigation remains available.
