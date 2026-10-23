# 2D Physics Lab

An interactive laboratory for Ferravex's original rotational 2D solver. A procedural stack of boxes and balls sits inside fixed platforms and boundaries, alongside four jointed mechanisms. The simulation uses `ferravex_core::session::GameSession` at 120 Hz, interpolated body poses, restitution and bounded ball CCD, with a maximum of 240 dynamic bodies.

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
| Restore the original scene | R | Back / Select; RESET |

The movement keys or left stick move the virtual cursor. The south face button also activates on-screen buttons. All bindings, including drop, step and inspect, appear in the shared Controls panel and can be remapped. Canvas hints read the current bindings and device through `PlayerInput::format_hint`; the table above lists defaults. Buttons wrap into two rows on narrow screens and three rows below 360px so RESUME remains readable. Narrow diagnostics use fixed 16px text in reserved wrapped rows; long binding hints point to Controls instead of shrinking. Unbound hints remain explicit. The browser provides a taller scrollable canvas and a sticky Scene / Lab controls / Guide link bar, which remains a native page scrolling surface above the canvas. STEP is disabled while running, PULSE while paused or recharging, and DROP at capacity. Drop previews match the clamped placement inside the boundaries.

The footer shows the selected shape, running/paused/suspended state, body/awake/asleep counts, fixed frequency, tick and pulse readiness. Wide layouts also show mean and p95 frame times. Last-tick diagnostics show solid contact pairs and contact starts/stops. Cursor queries count collider hits, retaining at most eight and explicitly reporting truncation; compound children count separately. Queries use actual current collider poses, which may lead the interpolated drawing by part of a tick while running. Pausing shows the current pose directly.

Press **I** over a body to pin its inspection. The steady white outline and footer identify its scene number, shape, awake/asleep/pending-wake state, quiet time, speed and last-tick contact count. Inspection is read-only and does not wake bodies. A miss clears selection; successful drops select the new body. Reset clears body selection while preserving the shape chosen for future drops. Contact diagnostics remain the latest completed tick while paused; they are not new events every render frame.

Pausing freezes body motion, simulation ticks and physics/audio cooldowns. You can still select and place shapes while paused; they remain still until you resume or step. Each **N / STEP** press advances exactly one 1/120-second tick and leaves the session paused; holding a button does not continuously step. STEP while running adds no extra tick. Running drop/pulse edges are consumed on actual fixed ticks, so a zero-tick render frame retains a quick tap and catch-up ticks do not repeat it. UI drop/pulse requests each have one pending slot; repeated requests before a tick coalesce. Paused drops remain immediate editor operations. Pulse is available only while running and has a 0.65-second simulation-time cooldown; pulses wake bodies and pass through the normal solver.

Reset restores the initial 34 dynamic bodies and joints, clears accumulated time, cooldowns, pending commands, body selection, pulse visuals and frame statistics, and preserves the selected shape and pause state. Reset takes priority over other inputs in the same frame, so a pending drop cannot leak into the rebuilt scene. The body cap rejects further drops with a visible explanation. Closing Controls/Details or regaining focus preserves a manually paused session; inactive time is not accumulated. Focus/control suspension and pause transitions discard stale queued commands.

The renderer and built-in glyphs are first-party, with no external assets. The existing native `physics_2d::run()` entry point and browser exports are unchanged. The private lab controller uses checked session construction and `GameSession::step_paused()`, retaining the fractional clock remainder without toggling lifecycle state. Rendering uses the current body pose while paused, so single steps are visible immediately. HUD sizing uses checked bitmap measurements and buttons use the shared enabled/selected styling.

Impact audio now requires an accepted solid contact start and a sufficient velocity change; gravity, joint correction alone and sensors cannot manufacture a collision cue. The pulse cue follows an accepted impulse. Audio remains on the game's creating thread through `GameAudio`.

Every presentation frame reads the shared `render::preferences` policy, including paused, Controls/Details, focus-suspended and zero-tick frames. Either reduced motion or reduced flashes clears the expanding pulse ring immediately and suppresses new rings; returning to standard settings cannot revive an old ring. Intensity scales the ring's opacity. Its cosmetic lifetime is separate from the physics cooldown and can finish while paused. Body motion, impulses, audio, inspection outlines and fixed-tick progression are unchanged by these preferences. The lab has no separate flash, camera shake, particles or `FeedbackController` to synchronize, and adds no preference store or export.

This sandbox has no win condition. Validate contacts, visible rotations, stable resting bodies, spawn limits and pause/reset. It does not establish general physics accuracy: supported shapes, joints and CCD limits are documented in the physics crate README. See the `ferravex-physics` README for the solver's precise scope.

Regression tests cover pause/resume without catch-up, single stepping and resumed fractional time, reset precedence and restored state, zero/catch-up tick command consumption, paused drops and shape overrides, body bounds/capacity, pulse gating/cooldowns, real-contact feedback, compound queries and truncation, read-only/stale selection, remapped hints, preference transitions without revived effects, and equal settling results at 30/60/144 Hz render rates on the tested native target. Existing solver/constraint tests remain. These checks do not claim cross-platform bitwise determinism or replace fresh Wasm/browser/native visual verification, which remains with the integration owner.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
