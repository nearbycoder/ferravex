# EMBERLINE

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

Carry a lantern through an abandoned woodland mill. A compact, asset-free 2D platformer built with Ferravex's fixed simulation clock and renderer. Layered forest silhouettes, geometric rocks, warm ember fissures, and a tiny scarf-wearing courier establish the visual style.

```sh
cargo run --release -p emberline
cargo run --release -p emberline -- --autoplay --smoke-frames 620 --screenshot /tmp/emberline-win.png
```

Move with A/D or left/right arrows; jump with Space, W, or Up. R restarts the whole run. Touch devices get left, right, and jump buttons. The game begins immediately. Collect optional floating embers, clear the orange spikes and fissures, light the midpoint checkpoint, and reach the green lift beyond the third section. Falling or touching spikes returns to the last checkpoint; collected embers remain collected. The completion card records time, embers, and falls, and offers a restart. Touch the completion card to replay.

The three sections are Treeline, Old Mill, and Furnace. Upper platforms offer optional collectible routes. Movement runs at 120 Hz with acceleration, axis-separated solid collision, a 100 ms coyote window, and a 120 ms buffered jump. The shared `Canvas2D` presentation adapts to window size while preserving the playfield's aspect ratio. Only title/section and ember count remain on the HUD; keyboard instructions disappear after seven seconds.

## Engine exercise

Uses `ferravex_core::FixedClock`, `ferravex_render::prelude`, and the shared `ferravex_render::demo::Demo` smoke/screenshot harness. Autoplay issues actual movement and jump commands; it does not teleport or bypass collision. A deterministic test proves it can complete all three sections without falling.

The example supplies a lightweight character controller because the engine does not yet provide a reusable platformer controller. A future engine-level controller could expose solid collision, coyote time, jump buffering, and one-way platforms. This game needs no rigid bodies or external physics dependency. Solid collision uses the shared `ferravex_core::collision::Aabb2` strict positive-area overlap check so touching floor edges do not become side collisions. The restored renderer draws text from its original built-in bitmap glyphs; no font atlas or downloaded font is required.

## Checks

`cargo test -p emberline` covers floor collision, coyote/buffered jumping, checkpoint hazard respawn, and complete autoplay progression. Also run `cargo clippy -p emberline --all-targets -- -D warnings` and `cargo check -p emberline --target wasm32-unknown-unknown`, then exercise controls and capture a fresh screenshot with the restored runtime. Historical backend results are not current recovery validation.
