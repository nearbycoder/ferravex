# Generated-project parity verification

The [parity verifier](../scripts/test_template_parity.py) generates fresh 2D and
3D games and tests their compatibility with the engine in the current checkout.
It checks the real generated code, native linking, Wasm compilation and isolated
browser staging. A pass is scoped build and headless evidence; it does not mark
Journey200 integrated or establish browser playability.

The [existing scaffolder suite](../scripts/test_new_game.py) already covers
destination safety, customization validation, escaping, lock seeding and mocked
staging. This verifier adds actual builds and public-API behavioral tests rather
than another generator or runtime. The
[2D rules template](../templates/game-2d/src/game.rs.in) and
[3D rules template](../templates/game-3d/src/game.rs.in) already share the same
fixed-step game; their renderers differ.

## Run the checks

From the engine checkout:

```sh
python3 -B scripts/test_template_parity.py
python3 -B scripts/test_template_parity.py --verify
```

The first command runs the verifier's Python regression tests only. Synthetic
Wasm files in those fixtures are explicitly test data, never build evidence.
The second command generates and builds both projects sequentially. It requires
Python 3.11+, Cargo/Rust, the native linker and platform libraries, and an already
installed `wasm32-unknown-unknown` standard library. It needs no display, Node,
network access or registry packages. See [platform prerequisites](PLATFORMS.md)
and the [dependency policy](../DEPENDENCIES.md).

```sh
python3 -B scripts/test_template_parity.py --help
python3 -B scripts/new-game.py --help
python3 -B scripts/check-docs.py docs/JOURNEY_200_TEMPLATES.md --json
```

`--verify` defaults to the generated helper's `dev` Wasm profile and a 600-second
timeout per child command. `--web-profile release` selects the helper's existing
release profile; `--timeout` accepts finite seconds from 1 through 1800. Nondefault
profile/timeout values require `--verify`. A missing prerequisite, failing test/build, malformed
artifact or missing host resource makes verification exit 1. Invalid CLI options
exit 2. Missing Wasm support is a failure, never a skipped pass.

## Isolation and evidence

Every verification creates a new
`target/journey-200/template-parity/run-*` directory. Its absolute path is printed
before work starts. Both generated project paths deliberately contain spaces and
quotes, exercising argument handling in real Cargo and Python calls. All child
commands use argument arrays without a shell.

- The 2D project uses default 60 Hz and package-derived title. The 3D project uses
  59.94 Hz and a title containing quotes, an ampersand and Unicode.
- Each project has its own `[workspace]`, seeded `Cargo.lock` and `target`.
  Generation is through the actual [new-game CLI](../scripts/new-game.py).
- Cargo resolves with `--offline --locked`. Compilation/test commands select
  only the generated package and use two jobs. Native commands explicitly select
  the host triple reported by `rustc -vV`, so an inherited default Wasm target
  cannot silently replace native verification.
- The generated [web helper](../templates/game-2d/scripts/build-web.py.in) runs
  with its real `--profile` option, builds `--lib --target
  wasm32-unknown-unknown`, and stages only that project's `target/web`.
- `report.json` records schema version 1, the toolchain, scope, profile, project
  paths, command argument arrays, working directories, return codes or errors,
  durations, per-command logs and artifact SHA-256 values. The report starts
  with `ok: false`; both complete projects must pass to change it to true.
- The first failed phase stops dependent work for that project; the other
  dimension can still provide independent evidence. Missing later phases mean
  unverified work. There are no automatic retries or reuse of earlier runs.

Projects, binaries, Wasm, logs and reports are retained for review. Remove only a
specific run directory when its evidence is no longer needed. The verifier never
rewrites checkout manifests, the checkout lock, templates, demo sources, the
catalog, or shared `dist`. No workspace-wide build or site generation runs.

## What executes

Before compiling, the verifier checks the generation manifest's exact file list,
package/dimension/engine identity, and standalone workspace marker. Locked Cargo
metadata must resolve only the generated game plus path packages under this
checkout's `crates`; registry/Git sources or foreign path packages fail.

It then writes an additional `tests/journey200_185.rs` into each new project.
The test source is the verifier's `RUST_TESTS` constant; existing generated code
and tests remain unchanged. A targeted `cargo test` runs the generated rule
tests, these four acceptance tests and the original `Game` rustdoc example:

- **Real game lifecycle:** generated `Game::new`, `start`, `advance`,
  `toggle_pause`, `restart`, `session`, `position`, `render_position` and
  `TICK_RATE_HZ` interoperate with the current facade's `GameSession`,
  `SessionState`, `FrameStep` and math types. Held movement reaches the actual
  beacon and wins without modifying private state. Ready, pause and win freeze
  updates; restart clears the clock and interpolation history.
- **Render cadence:** the same sixty simulation ticks, delivered over 30, 60
  and 144 render frames, produce identical positions. This compares frame
  partitioning on one platform; it does not test cross-platform determinism.
- **Runtime input:** `PlayerInput::preset` with `Preset::Explore` receives
  injected `PadState` samples through `gamepad_sample` and `host_frame`. Confirm,
  movement, pause and restart bindings work with the generated rules. An
  `interact` tap survives a zero-tick frame and appears once across catch-up
  ticks; duplicate names in `take_tick` share the same snapshot.
- **Controls ownership:** `PlayerInput::transient`, `set_controls_open`,
  `sample`, `gameplay_active` and `simulation_dt` gate local elapsed time and
  reject invalid deltas. A separately advanced `GameSession` continues while
  Controls is open. This tests the clock-ownership pattern, not a network or
  replay implementation in the scaffold.

The Explore preset includes `interact`, but the shipped beacon game does not
consume that action. The edge test exercises the public adapter for a future
gameplay extension; it does not claim the template has an interaction mechanic.
The actual async play loops in
[2D](../templates/game-2d/src/lib.rs.in) and
[3D](../templates/game-3d/src/lib.rs.in) are compiled. Headless acceptance tests
call public rules/input APIs, rather than driving those async loops. Native
`run()` is type-checked and the real native binary is linked, without launching
a window.

After a successful Wasm build, the verifier checks that the staged module matches
the freshly built module byte-for-byte by hash. A bounded standard-library
reader inspects the export table for `memory`, `ferravex_init`, `ferravex_frame`,
`ferravex_key`, `ferravex_focus` and `ferravex_feedback_preferences`, with the
expected memory/function kinds. This checks symbol presence, not function
signatures, ABI behavior or WebAssembly instantiation.

The canvas game ID and generated `games.json` entry must match the package and
Wasm filename. HTML resources and literal `./*.js` / `./*.json` references in
staged JavaScript must resolve inside the stage, including current dynamic
imports. AudioWorklet and extracted retained/effects shader files must exist.
This is a static check of current literal resource paths, not a complete
JavaScript dependency parser or an HTTP/browser test. Computed resource paths,
GPU shader compilation and audio output remain unverified.

## Results and remaining integration

Task 185's exact executed commands, report path and observed results are recorded
in its receipt at `target/journey-200/receipts/185.md`. Retained reports describe
the checkout at execution time; concurrent or later edits need fresh validation.
Source availability and another task's receipt are not integration evidence.

The [shared validator](../scripts/validate.py) automatically discovers the
Python self-test file and checks this guide through `DOC_CHECK_PATHS`.
Its full Wasm group includes `wasm.templates`, which runs
`python3 -B scripts/test_template_parity.py --verify --web-profile release`
with Cargo/native/Wasm prerequisites. The fast Python suite intentionally
does not build projects. No Rust module
registration, reexport, dependency or snippet `--extern` wiring is needed.

Parent retains combined workspace/feature validation, fresh browser/native
playtests and final status. This verifier does not exercise a native window,
browser input events, physical controllers, touch, narrow layouts, focus/fullscreen
behavior, screen readers, audio, GPU output, release performance, or network/replay
progression. The common presentation export is checked for linkage; the beacon
templates have no feedback effects whose reduced-motion/flash behavior could be
claimed. The checked host/build path does not establish demo feature parity.
