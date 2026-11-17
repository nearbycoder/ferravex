# Ferravex Arcade host

This first-party crate hosts the twenty independent campaigns listed in
[The Twenty](../../../docs/GAMES_20.md). It reuses Ferravex's renderer,
`GameSession`, `PlayerInput`, `GameAudio`, `Records` and `StatusBridge`. It does
not contain the games' rules or replace those existing systems.

Each `ArcadeGame` receives one plain `Input` per executed 60 Hz tick and returns
feedback only for accepted events. The host owns the title, paginated guide,
pause, restart, results, audio, controls editor and local personal records.

The host speaks the [visual language](../../../docs/VISUAL_LANGUAGE.md): a live
bot demonstration plays behind the title, the header shows progress pips and a
star score whose gains rise as `+N`, the footer offers icon buttons for pause,
help, controls and sound (with key tooltips), the guide's first page shows the
controls as key caps, and the results card counts the score up beside a trophy.
Accepted cues are felt as well as heard: sparkles on good moves, shake and a
red vignette on bad ones, confetti on victory. Optional `ArcadeGame` hooks let
each game add `progress()` pips, location-based `effects()` through the
engine's `Juice`, a visual `draw_objective()` and an icon `draw_result()`.
`clock()` and `pointer()` give `draw` presentation time and hover position.
Menus and lost focus suspend these offline games. Terminal states stop the
remaining catch-up batch. Restart replaces the model and clock debt and
suppresses stale held controls. Autoplay calls the same update path with a
read-only game-authored command generator; it uses transient records/settings.

Drawing uses a 960 by 640 logical canvas. A centered, aspect-preserving camera
and inverse pointer transform keep rendering and hit tests aligned at different
window sizes. Games own the area between y=112 and y=568; the host owns the top
HUD and the bottom icon bar. Keyboard, pointer and touch use the same fixed-tick game
commands. A first touch reveals fourteen controls: movement, Act, X/C, choices
1–5, Pause and Guide. Touching the playfield also activates its buttons and
tiles. Act supplies Space/Enter, including held actions. Wide phones use side
margins for touch rails; narrower screens reserve a bottom dock. Targets are at
least 44 logical screen pixels, and saved touch-layout edits use the same
geometry for drawing and hit testing. Rotation, cancellation and menus release
held contacts. Physical mobile and gamepad verification remain separate checks.

The read-only browser status bridge publishes the current lifecycle, objective,
stage, status, score, executed tick count and ending through the framework's
existing versioned ABI. Game-specific stories and tactics are not automatically
screen-reader accessible merely because that bridge is available.

```sh
cargo test -p ferravex-arcade --offline --locked -j 2
cargo clippy -p ferravex-arcade --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p mosslight --offline --locked -j 2 -- --autoplay --smoke-frames 420 --screenshot target/mosslight.png
```

The native runner also supports an explicit `FERRAVEX_ARCADE_CAPTURE` output
directory during autoplay: it exports a PNG every six presentation frames.
For screenshots, `FERRAVEX_ARCADE_VIEW=help`, `help:N` or `paused` opens a menu
after 90 frames, `FERRAVEX_ARCADE_FAST=N` (1–240) runs autoplay N times
faster to reach an ending, and `FERRAVEX_ARCADE_POINTER=x,y` pins the pointer
to capture hover previews. These knobs are native-only.
Normal interactive play never writes frame files. The existing
`scripts/capture-media.mjs` captures browser gameplay for the website's posters,
stills and loops. Neither capture path establishes campaign balance or audible
sound-device quality.

Pure driver regressions cover zero-tick input retention, one-shot catch-up
consumption, paused-time discard, terminal batch gating, restart and render-rate
independent tick counts; viewport tests check aspect fitting and pointer
round-trips. Each game owns its separate rules/content regression suite.

After a browser build, `node --test web/tests/wasm-arcade.mjs` checks the actual
twenty release artifacts. It exercises start, help, pause, control editing and
restart at three canvas sizes, then runs every complete campaign through legal
autoplay with catch-up ticks. Victory must publish an ending, freeze further
simulation and leave personal records unchanged. This ABI check complements
the rendered browser captures; it does not simulate a human playthrough.
