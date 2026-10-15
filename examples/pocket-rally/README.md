# Pocket Rally

A small complete 2D rally time trial built with Ferravex. Race three laps of Valle
Verde, a procedural rural circuit, then try to beat the translucent best-lap ghost.
There are no downloaded art assets, audio files, textures, or fonts.

```sh
cargo run --release -p pocket-rally
cargo test -p pocket-rally
cargo clippy -p pocket-rally --all-targets -- -D warnings
cargo run -p pocket-rally -- --autoplay --smoke-frames 6000 --screenshot /tmp/rally-finish.png
```

WASD or arrow keys steer, accelerate, and brake/reverse. Space loosens rear grip
for handbrake turns. R restarts while preserving the session's best lap and ghost.
After the finish, clicking/tapping also starts another race. Touching the screen
reveals left/right steering and gas/brake controls along the bottom edge.

The road is deliberately forgiving, but grass sharply reduces speed. Small pale
arrows mark the next gate. Sixteen ordered gates check forward line crossings
inside the road corridor; repeatedly crossing the finish, reversing, and driving
outside a gate cannot award a lap. The three-lap result freezes the race timer.

The 120 Hz `ferravex_core::FixedClock` drives a compact longitudinal/lateral tire model:
acceleration, braking, speed-dependent steering, rolling resistance, lateral grip,
handbrake slides, and off-road drag. These are arcade vehicle dynamics, not a claim
of realistic suspension or full rigid-body car simulation. The autoplay driver
uses pure pursuit to produce normal steering/throttle controls, never teleports,
and completes the same gates and finish condition as a human.

Best-lap poses are sampled at 20 Hz and retained across retries; only the first
five minutes of a very slow lap are recorded. Track sampling, trees, farm buildings,
fields, and the car are generated from simple geometry. The road has 192 segments;
skid marks are bounded to 500. Race tests are headless and cover checkpoint order,
reverse/shortcut rejection, off-road drag, bounded motion, complete autoplay races,
finish state, and best-ghost preservation.

## Engine exercise

Uses Ferravex's fixed-step clock, rendering prelude/window configuration, and shared
`demo::Demo` native smoke harness. No extra runtime dependency or missing shared
API was needed. This exposed a practical DPI trap: raw `Camera2D.viewport` values
are framebuffer coordinates, while `screen_width()` is logical screen units.
The game uses a full-window camera projection instead, so track aspect and touch
controls stay aligned on scaled displays. Custom vehicle dynamics live here as
game logic; they do not expand the common engine API prematurely.

Best laps are session-only; there is no disk storage, multiplayer, gamepad, or
sound in this example. Browser builds share identical Rust simulation; the native
smoke flags are intentionally unavailable in browser builds.
