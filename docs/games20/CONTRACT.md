# Twenty complete games: shared contract

This is a new user-authorized collection, separate from the paused Journey400
numbered backlog. Preserve all earlier work. Parent owns shared integration.
Each of twenty distinct implementation agents owns one `examples/<id>/` tree
(except its parent-authored Cargo.toml) and `target/games20/receipts/<id>.md`.
No subdelegation, commits, branches, PRs, deployment, scheduling, or other paths.
Read AGENTS.md, CONTRIBUTING.md, DEPENDENCIES.md and docs/AGENT_GUIDE.md; inspect
existing APIs and `examples/shared/arcade/src/lib.rs` before implementation.

## Deliver a game

Build an original, cohesive game with a tutorial/onboarding, an understandable
objective, meaningful choices, escalating authored content, win AND loss (or
explicit puzzle retry), complete ending, immediate retry, and replay scoring.
Use at least six substantial levels/missions/rounds or a campaign of equivalent
length and strategic depth. Do not deliver a mechanics sandbox, reskin, empty
scaffold, or one-screen survival counter. Draw a distinct, legible visual world
with procedural art, purposeful palette, game-state feedback, readable controls,
and content-specific HUD. No external libraries/assets. Quality over API breadth.

Separate pure rules/content (`model.rs`) from presentation. Game rules use
fixed 60 Hz ticks and the supplied Input; never platform I/O, renderer polling,
wall clocks or nondeterministic randomness. Bounded vectors/arrays. Use shared
math and existing gameplay APIs when useful; do not extend engine crates.
Only accepted events return Cue. All feedback/input/restart lifecycle is hosted
by the shared runner. Each game's draw is read-only; score is monotonic if possible.

## Shared crate: ferravex-arcade (parent-owned)

Available dependencies in every game: ferravex-arcade (path ../shared/arcade),
ferravex-render, ferravex-core, ferravex-play, ferravex-math (workspace).
`ferravex_arcade` reexports ferravex_render::prelude::*.

Logical canvas is WIDTH=960, HEIGHT=640. Game-specific drawing occupies
x=16..944, y=112..568. Shared HUD occupies y=0..104 and footer y=584..640.
Shared runner fits the canvas without distortion and inverse-maps pointer input.

Implement this trait on `pub struct Game` (usually reexport from model):

    pub trait ArcadeGame: Sized {
        fn new() -> Self;
        fn update(&mut self, input: &Input) -> Cue;
        fn draw(&self);
        fn hud(&self) -> Hud;
        fn outcome(&self) -> Outcome;
        fn summary(&self) -> String;
        fn bot_input(&self, tick: u64) -> Input;
        fn music_notes(&self) -> [Option<u8>; 4] { [None; 4] }
    }

Input is Copy + Default with public fields:
`left,right,up,down: bool` (held),
`left_pressed,right_pressed,up_pressed,down_pressed: bool` (edges),
`primary,secondary,tertiary,click: bool` (edges),
`primary_down: bool`, `pointer: Vec2`, `slot: Option<usize>` (1..5).
Arrows/WASD = direction; Space/Enter = primary; X/E = secondary;
C/Q = tertiary; mouse left = click (not also primary); keys 1..5 = slot.
The input edges are retained over zero-tick frames and consumed once per tick.
Esc/P pause; R restarts the WHOLE run; H help; M sound are parent-owned.
Do not use these keys for game mechanics. Implement any per-level retry/undo
via secondary/tertiary as appropriate, clearly explain costs and effects.

`Cue` = None, Action, Good, Bad, Win; `Outcome` = Playing, Won, Lost.
`Hud { stage: String, objective: String, status: String, score: u32 }`.
`Meta { id: &'static str, title: &'static str, subtitle: &'static str,
  controls: &'static str, help: &'static [&'static str], accent: Color }`.
`run::<Game>(META).await` hosts menus, pause, help, retry, audio and best records.
`window(title)` returns Conf; renderer macro supplies public run().

The default font is a wide 5x7 pixel font: measure at the actual fractional size
with `measure_text(value, None, 8, size / 8.)`. Reserve vertical space for wrapped
paragraphs. Keep side-panel labels, cards and logs from overlapping.

Drawing helpers (or use ferravex_render directly):
`text(value:&str,x:f32,y:f32,size:f32,color:Color)` (y is baseline),
`panel(x:f32,y:f32,w:f32,h:f32,color:Color)`,
`meter(x:f32,y:f32,w:f32,h:f32,fraction:f32,color:Color)`,
`wrapped(value:&str,x:f32,y:f32,max_width:f32,size:f32,color:Color)->f32`
(returns next baseline).
Constants INK, PAPER, MUTED; TICK_DT=1.0/60.0.
`text` and `wrapped` use actual renderer text measurement. Use ASCII text in
canvas copy for the built-in font; browser README prose may use Unicode.

Example lib entry (adapt metadata):

    pub mod model;
    mod view;
    use ferravex_arcade::*;
    pub use model::Game;
    pub const META: Meta = Meta { /* all fields */ };
    fn conf() -> Conf { window(META.title) }
    async fn game() { ferravex_arcade::run::<Game>(META).await; }
    ferravex_render::game!(conf, game);

An impl ArcadeGame may live in view.rs delegating update/outcome/hud to pure
model methods, but avoid name recursion. Document public APIs and give a real
runnable rustdoc example using new/update/outcome without rendering.

`music_notes` is an optional default hook: return up to four MIDI note-on
events from the just-executed tick. The host calls it exactly once after update,
synthesizes cached tones, and clears owned voices on pause/restart. Values above
127 are ignored; ordinary games leave the default silence.

`bot_input` is mandatory: generate LEGAL player controls through the same rules,
without writing state, bypassing hazards or special autoplay invulnerability.
Its purpose is reproducible native visual smoke. It need not play optimally.
Default main calls <crate>::run(). Optionally accept --headless for genuine
simulation validation (intercept before renderer Demo parses options).
Native smoke flags are --autoplay --smoke-frames N --screenshot PATH.

## Acceptance and receipt

Run `cargo test -p <id> --offline --locked -j 2` and scoped rustfmt. Add meaningful
behavioral tests for mechanics, invalid actions, loss/retry, terminal invariance,
and a COMPLETE campaign win via legal player actions (no direct stage/win hacks).
For procedural games, test reproducible generation and solvability where relevant.
Prove resource consumption and objectives cannot be farmed by repeated invalid
input. At least six focused tests; tests may be inline in owned source. Include
README with objective, campaign/content count, controls, rules, strategy, run
commands and honest validation limits. Do not call compilation a playtest.

No broad builds/Wasm/shared staging. A cross-owner compile error is a handoff,
not permission to patch or repeatedly poll another file. Parent integrates.
Receipt lists paths, mechanics, exact APIs, content counts, controls, commands,
executed tests and limitations, and any parent-required registration.
