# Siege Foundry

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A low-poly 3D demolition range built with Ferravex's `GameSession`, original rotational 3D physics, cached meshes, and built-in lit material. Bring down all six rust-red blocks with eight iron cannonballs. Supporting sandstone blocks are physical bodies too: removing a low support can collapse an entire tower.

```sh
cargo run --release -p siege-foundry
cargo test -p siege-foundry --offline --locked -j 2
cargo run --release -p siege-foundry -- --autoplay --smoke-frames 480 --screenshot siege-foundry.png
```

- **Enter**, **Space**, or **Start**: enter the range. Starting does not spend a shot.
- **Left / Right** or **A / D**: change cannon heading.
- **Up / Down** or **W / S**: raise / lower the barrel.
- **Space** or **Fire**: shoot. The dotted line previews the gravity-driven arc.
- **Drag in the scene**: aim with a mouse or touch; use the Fire button to shoot.
- **Escape** or **Pause / Resume**: freeze / resume the range.
- **R** or **Rebuild**: restore all bodies, eight shots, the original aim, timers and feedback.
- After a result, **Enter**, **Space**, or **Again / Retry** rebuilds without firing. The next fresh Fire press launches a shot.

The HUD shows heading (**YAW**) and elevation (**ELEV**) in degrees, fixed launch power (**28 m/s**), an ammunition count and eight ammunition markers. Power is fixed; adjust the angle to change the trajectory. Fire shows **Loading** and a progress bar during its 0.8-second reload. A press during reload is discarded, so it cannot launch an unexpected shot later. **Shot away** confirms an accepted shot; target knockdowns show a count and turn muted green. Shot, destruction and result cues use the shared audio system; enable browser sound with the Sound button.

The keys above are defaults. **Controls** supports remapping; the Fire and start/resume/retry hints read the current `PlayerInput` bindings after sampling, including **Unbound** actions. Touch hints point to the visible buttons rather than unused preset touch channels. Long binding lists are shortened with an ellipsis at a fixed font size; open Controls for the full mapping. Narrow layouts retain separate aim, power, ammo and status rows and 44-pixel-high buttons. Starting, retrying or rebuilding still spends no ammunition.

The view keeps the cannon, standing towers and low support arcs between the HUD panels on narrow screens. Framing uses the same fixed perspective and eye, fitting only its screen projection; it does not move with aim or debris, alter fog/depth, or change input sensitivity or ballistic rules. High arcs and flying debris can still leave the framed range. The header uses a complete short objective at 320 pixels.

**Details → Player tools → Game and player UI motion** controls barrel recoil through the shared render presentation policy. Each presentation frame reads that policy, including ready, paused, Controls, results and zero-tick frames. Reduced motion or zero visual intensity clears active recoil; restoring motion cannot replay it, and shots accepted while it is disabled cannot revive it. Shot/target text, target colors, audio, aiming, trajectories and timers remain unchanged. This range has no cosmetic particles, camera shake or screen flashes; the independent reduced-flash setting does not hide its static feedback.

Rust-red blocks count when they fall, rotate far enough, or move out of their original stack. The final cannonball gets six seconds of simulation time to finish its work before a loss; the HUD displays the settling countdown. A win or loss freezes the result and offers a fresh range. Pausing, losing focus or opening Controls suspends physics, reload and settling timers. Closing Controls or restoring focus resumes only if the player has not manually paused.

`ferravex_core::session::{GameSession, SessionState}` owns the host lifecycle and fixed clock. Physics and held-key/stick aim run at 120 Hz with at most twelve catch-up steps per frame; transforms interpolate between ticks, and cannonballs use continuous collision detection. Relative mouse/touch look is sampled once per presentation frame. Gameplay Fire uses `PlayerInput::take_tick` inside actual ticks, so quick press/release edges survive zero-step frames and cannot repeat across catch-up ticks. Visible Fire-button presses use a single bounded pending flag. Pause and rebuild discard stale edges. Terminal states stop the remainder of a catch-up batch. Rebuild resets both simulation resources and session clock. A bounded eight-projectile allocation avoids unbounded debris growth; native smoke flags and browser `?autoplay` remain supported.

Regression tests exercise stable stacks, shot cooldown and ammunition exhaustion, CCD, the final-shot grace period, zero-tick/catch-up input through the real PlayerInput adapter, reload rejection, ready/pause/terminal states, rebuild resources, accepted feedback, recoil cancellation, remapped/unbound hints and bounded HUD metrics at 320/390/700/1280 pixels. Held aim produces the same result at 30/60/144 render frames per second. Autoplay runs through the same session host at 30 and 144 render frames per second and knocks down every target through normal ballistic contacts. It only aims and fires; it never moves targets or awards points directly. A compiling crate rustdoc example demonstrates session start, finish and restart. These native/headless checks do not establish fresh Wasm linkage, browser preference delivery, visual layout, physical controls or audible playback; parent integration owns those checks.

The range is a small single-player example. Blocks topple as rigid pieces rather than fracturing, and the arc preview stops at the first collision without predicting later deflections. No persistent high scores or network mode is included. The restored renderer has Linux and WebAssembly hosts. Browser pointer/touch forwarding uses original platform bindings; physical touch devices and the updated HUD still need visual verification. Headless tests do not establish cross-platform physics determinism.

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
