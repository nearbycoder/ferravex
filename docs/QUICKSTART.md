# Ferravex quickstart

Start from this checkout with the Rust toolchain required by the workspace
(`rust-version = "1.88"`, edition 2024). All Rust dependencies are local,
first-party workspace crates. Read the [dependency policy](../DEPENDENCIES.md)
before adding a package. Commands below run from the repository root.

## Run an existing example

Run the facade example without a display first:

```sh
cargo run -p ferravex --example quickstart --offline --locked -j 2 -- --headless
cargo run -p ferravex --example quickstart --offline --locked -j 2 -- --help
```

The headless mode executes the same fixed-update path as the interactive loop.
It asserts 60 ticks of movement, inactive-time discard, one paused inspector
step and a reset of both clock and poses on restart. It prints a success line
and exits; it does not call the window/GPU host.

For a headless application with startup, fixed updates, interpolation and shutdown:

```sh
cargo run -p ferravex-engine --example app_lifecycle --offline --locked -j 2
```

For an interactive example that imports only the facade:

```sh
cargo run -p ferravex --example quickstart --offline --locked -j 2
```

Arrows move the ball, Space pauses/resumes, N advances one tick while paused,
and R restarts. Hold an arrow while pressing N to inspect a movement tick.
Read its complete
[source](../crates/ferravex/examples/quickstart.rs): `Ball::fixed_update` receives
fixed time and input; `render_frame` reads snapshots. A supported native window
and graphics environment is required. A successful headless run does not verify
that environment; see [platform prerequisites](PLATFORMS.md).

To inspect the command for a catalog game without starting it:

```sh
python3 -B scripts/ferravex.py run emberline --dry-run
```

Omit `--dry-run` to launch it on a supported native graphics host.
The [catalog](../examples/catalog.json) lists all fifteen packages and browser
hosts. Courtyard's package is `owned-lab`, while its browser host is `owned`.

With the `wasm32-unknown-unknown` standard-library target installed, compile the
facade example's browser entry point separately:

```sh
cargo build -p ferravex --example quickstart --target wasm32-unknown-unknown --offline --locked -j 2
```

With the default Cargo target directory the artifact is
`target/wasm32-unknown-unknown/debug/examples/quickstart.wasm`. This checks the
example and existing `ferravex_init`/renderer ABI linkage. It does not create a
browser page or run WebGL. Quickstart is a Cargo example, not a catalog game;
the shared web builder does not stage it. Use a generated project's own host
for a standalone browser game, as described below.

## Start with the facade and a fixed simulation

`ferravex::prelude::*` brings in session timing, ECS handles/world, common math,
renderer cameras/configuration, and input entry points. Rendering functions stay
qualified as `render::…`. More specialized systems have explicit paths, such as
`ferravex::app`, `ferravex::feedback` and `ferravex::gameplay`.

This complete headless example advances a point without a renderer or wall clock.
Use the same update function inside a host's fixed-tick loop:

```rust,check,crate-type=bin
use ferravex::prelude::*;

struct Point {
    previous: Vec2,
    position: Vec2,
}

impl Point {
    fn new() -> Self {
        Self { previous: Vec2::ZERO, position: Vec2::ZERO }
    }

    fn fixed_update(&mut self, dt: f32, direction: Vec2) {
        self.previous = self.position;
        self.position += direction.clamp_length_max(1.0) * 120.0 * dt;
    }

    fn visible(&self, alpha: f32, playing: bool) -> Vec2 {
        if playing { self.previous.lerp(self.position, alpha) } else { self.position }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SessionConfig::try_new(60.0, 8, false)?;
    let mut session = GameSession::from_config(config);
    let mut point = Point::new();
    assert_eq!(session.state(), SessionState::Ready);
    assert_eq!(session.advance(20.0).steps, 0);
    assert!(session.start());

    let frame = session.advance(1.5 / 60.0);
    for _ in 0..frame.steps {
        point.fixed_update(frame.dt, Vec2::X);
    }
    assert!((point.position.x - 2.0).abs() < 0.0001);
    assert!((point.visible(frame.alpha, session.is_playing()).x - 1.0).abs() < 0.0001);

    session.set_paused(true);
    let paused = session.advance(30.0);
    assert_eq!(paused.steps, 0);
    assert_eq!(point.visible(paused.alpha, session.is_playing()), point.position);

    // Inspector commands schedule real ticks without resuming the session.
    let stepped = session.step_paused()?;
    for _ in 0..stepped.steps {
        point.fixed_update(stepped.dt, Vec2::X);
    }
    assert_eq!(session.state(), SessionState::Paused);
    assert_eq!(session.tick(), 2);
    assert_eq!(stepped.alpha, 1.0);
    assert!((point.position.x - 4.0).abs() < 0.0001);
    assert_eq!(session.advance(30.0).steps, 0);

    session.restart();
    point = Point::new(); // Also reset simulation and interpolation history.
    assert_eq!(session.tick(), 0);
    assert_eq!(point.visible(0.0, true), Vec2::ZERO);
    Ok(())
}
```

`SessionConfig::try_new` accepts finite rates in `1..=1000` Hz and a nonzero
catch-up limit. It returns the existing `ferravex::core::FixedClockError` on
invalid settings. Copy a config to create independent sessions, or use
`GameSession::try_new` for a single checked construction. The original
`GameSession::new(hz, max_steps, autoplay)` begins Playing only when `autoplay`
is true. `start()` enters Playing from Ready; `set_paused(false)` resumes;
`restart()` clears time and enters Playing; `reset()` clears time and enters
Ready. `finish(won)` records a result from Playing or Paused. It does not decide
whether game rules have actually been won or lost.

The host supplies elapsed seconds for each frame. Keep measuring between frames
while paused; if the host suspends entirely, rebase its time measurement on
resume. Inactive elapsed time is discarded, but fractional time from earlier
playing frames is retained. Paused/terminal sessions return `alpha = 0`, so show
the current pose or synchronize both snapshots instead of stepping back to the
previous pose.

`step_paused()` schedules exactly one update and retains fractional clock debt;
the caller must execute that update. It rejects Ready, Playing, Won and Lost
with `ferravex::session::SessionStepError`. Its returned `alpha = 1` is for the
new current pose; later inactive `advance` calls still return zero. Neither
manual stepping nor restart resets your game entities, input or effects.

## Connect the host

The [interactive quickstart](../crates/ferravex/examples/quickstart.rs) uses the
existing `ferravex::render::game!(config, game)` macro. `config() -> Conf` supplies
window settings; `async fn game()` runs the frame loop and awaits
`render::next_frame()`. The macro supplies native `run()` and the existing Wasm
initialization export. In library-based demos, keep a native `main.rs` that calls
the library's `run()`.

For a host-driven synchronous lifecycle, implement
[`ferravex::app::Game`](../crates/ferravex-engine/src/app.rs), then construct
`App::new(game, session)` and call `app.frame(elapsed_seconds)`. It runs startup
once, fixed updates, and one presentation hook per running frame. Call
`app.stop()` to observe shutdown errors. Application startup and session start
are separate: a Ready session stays Ready until the host starts it.

Use `frame_report` when the host needs to distinguish scheduled ticks from
executed hooks. This complete headless example stops during a catch-up batch:

```rust,check,crate-type=bin
use ferravex::app::{App, AppControl, AppError, Game};
use ferravex::prelude::*;
use std::convert::Infallible;

#[derive(Default)]
struct Counter { updates: u32, shutdowns: u32 }

impl Game for Counter {
    type Error = Infallible;

    fn fixed_update(&mut self, _dt: f32) -> Result<AppControl, Self::Error> {
        self.updates += 1;
        Ok(if self.updates == 2 { AppControl::Stop } else { AppControl::Continue })
    }

    fn shutdown(&mut self) -> Result<(), Self::Error> {
        self.shutdowns += 1;
        Ok(())
    }
}

fn main() -> Result<(), AppError<Infallible>> {
    let session = GameSession::try_new(64.0, 4, true).expect("valid constant settings");
    let mut app = App::new(Counter::default(), session);
    let report = app.frame_report(3.5 / 64.0)?;
    assert_eq!(report.scheduled_updates, 3);
    assert_eq!(report.executed_updates, 2);
    assert_eq!(report.control, AppControl::Stop);
    assert!(!report.rendered);
    assert_eq!(app.session().tick(), 3); // The clock counts the scheduled batch.
    assert_eq!(app.game().updates, 2);
    assert_eq!(app.last_frame_report(), Some(report));
    app.stop()?; // Cleanup was already attempted exactly once.
    assert_eq!(app.game().shutdowns, 1);
    Ok(())
}
```

Counts describe hook invocations, including a hook that returns an error, rather
than successful simulation commits. On a frame error, inspect
`last_frame_report()` for the partial result. `dropped_seconds` records the
clock's catch-up budget loss; it does not include skipped hooks or paused time.
Stopping the application is permanent; pausing its session keeps presentation
running. The host must still show the current pose when inactive.

Choose either direct session advancement or `App::frame`/`frame_report` as the clock owner.
Do not advance the same session twice. Both approaches preserve the existing
async renderer games; neither requires changing a demo's save or network format.

For a new local project, inspect the stdlib scaffolder and preview its invocation:

```sh
python3 -B scripts/new-game.py --help
python3 -B scripts/ferravex.py new moon-garden ../moon-garden --template 2d --dry-run
```

Remove `--dry-run` to create it. The destination must not exist and its parent
must exist. Use `--template 3d`
for the other template. The generated project points to this checkout; follow
its README for native and browser entry points. Do not add it to the shared
workspace or regenerate the workspace lockfile as part of a narrowly owned task.

## Add input and feedback at the right boundary

Sample `PlayerInput` every host frame, including frames with no simulation ticks.
Read held movement once, but consume gameplay edges with `pressed`, `take`, or
`take_tick` **inside** each actual tick. Share the returned action snapshot
between systems. Consume global menu actions in the host frame. This retains a
quick tap through a zero-tick frame and prevents replaying it on every catch-up
tick. Relative look motion is consumed once per host frame and converted to an
absolute aim for simulation commands.

For a local session, choose one pause owner and reconcile controls/focus/menu
state before advancing. For a remote match, a local overlay suppresses commands
while authority continues. Recorded tick order and explicit replay controls
remain replay-owned.

Apply game rules first, then route accepted shots, hits, landings or pickups
through `FeedbackController`. Only a returned `Some(FeedbackResponse)` should
play the cue or spawn effects. Clear input debt, transient feedback, camera
history and game timers alongside session restart. The [migration cookbook](MIGRATION_50.md)
provides compiling before/after recipes for these integrations and all fifteen
demos.

Synchronize `render::preferences::sync_feedback(&mut feedback)` on every
presentation frame before emitting/sampling effects, including paused and
zero-tick frames. Motion and flash preferences are independent. Separately owned
particles or camera impulses also need to obey `feedback_policy()`; a controller
cannot recall effects that another system already spawned. These presentation
choices must not change game rules, replay completion or network progression.

## Find specialized APIs

`SessionConfig` is the only new wildcard export in this refresh. Existing root
and prelude exports remain compatible; use qualified paths for the other systems.
All paths below start at `ferravex::` and refer to the existing implementation,
without a second wrapper or wire format:

| Need | Qualified path and source |
| --- | --- |
| Checked settings, paused inspector ticks | `SessionConfig`, [`session::SessionStepError`](../crates/ferravex-core/src/session.rs); setup errors at `core::FixedClockError` |
| Host lifecycle and partial frame reports | [`app::{App, Game, AppFrameReport}`](../crates/ferravex-engine/src/app.rs) |
| Drift-free timers and scheduled tasks | [`time::SimulationTimer`](../crates/ferravex-engine/src/time.rs), [`scheduler::TickScheduler`](../crates/ferravex-engine/src/scheduler.rs) |
| Bounded work, interpolation and metrics | [`events`](../crates/ferravex-engine/src/events.rs), [`snapshots`](../crates/ferravex-engine/src/snapshots.rs), [`diagnostics`](../crates/ferravex-engine/src/diagnostics.rs) |
| Action maps and binding conflicts | [`input`](../crates/ferravex-play/src/input/mod.rs); host sampling/hints at [`render::input::PlayerInput`](../crates/ferravex-render/src/input/player.rs) |
| Follow/orbit/framing rigs | [`camera`](../crates/ferravex-play/src/camera/mod.rs); drawing cameras remain `render::{Camera2D, Camera3D}` |
| Gameplay, routes and authored scenes | [`gameplay`](../crates/ferravex-play/src/gameplay/mod.rs), [`play::navigation`](../crates/ferravex-play/src/navigation.rs), [`play::scene`](../crates/ferravex-play/src/scene.rs), [`authoring`](../crates/ferravex-play/src/authoring/mod.rs) |
| Accepted-event effects and accessibility | [`feedback`](../crates/ferravex-play/src/feedback.rs), [`render::preferences`](../crates/ferravex-render/src/preferences.rs) |
| Portable mixing and runtime voices | [`play::audio`](../crates/ferravex-play/src/audio/mod.rs), [`audio::GameAudio`](../crates/ferravex-render/src/audio/game.rs) (runtime owner stays on its creating thread) |
| Generic tick recording/playback | [`play::replay::{Recorder, Recording, Playback}`](../crates/ferravex-play/src/replay/mod.rs); [`replay::Replay`](../crates/ferravex-engine/src/replay.rs) is the distinct legacy network-input recorder |
| Portable saves and host I/O | [`persistence`](../crates/ferravex-play/src/persistence/mod.rs), [`render::storage`](../crates/ferravex-render/src/storage/mod.rs) |
| Rotating rigid bodies or translational physics | [`physics::{d2, d3}`](../crates/ferravex-physics/src/lib.rs), [`dynamics`](../crates/ferravex-dynamics/src/lib.rs) (different world types) |
| Retained rendering, materials and UI | [`render::retained`](../crates/ferravex-render/src/retained.rs), [`render::material`](../crates/ferravex-render/src/material.rs), [`render::ui`](../crates/ferravex-render/src/ui.rs) |

The facade's optional `content` feature forwards to the engine/renderer authoring
bridge (`render::content`). Renderer-independent `authoring` parsing is available
without it. The quickstart works with default features and with `content`; neither
configuration adds third-party dependencies. Keep optional authoring exports out
of production games unless the game needs that bridge.

## Check the part you changed

```sh
cargo test -p ferravex --lib --offline --locked -j 2
cargo test -p ferravex --example quickstart --offline --locked -j 2
cargo test -p ferravex --doc --offline --locked -j 2
python3 -B scripts/check-docs.py docs/QUICKSTART.md
```

The Rust commands check facade exports, example behavior and Rustdoc examples.
The documentation checker checks local links and finds the two complete
`rust,check,crate-type=bin` snippets above. By default it does not compile them;
`--check-rust --extern ferravex=PATH_TO_MATCHING_RLIB` type-checks them using a
prebuilt facade artifact and `--extract NEW_DIRECTORY` writes their sources and
manifest for separate compilation/execution. Type-checking does not run assertions.
Select the matching hashed `libferravex-….rlib` in Cargo's `debug/deps` directory:
the checker searches that artifact's directory for its first-party dependencies.
The top-level `debug/libferravex.rlib` copy does not supply that dependency path.

These commands do not launch a window or validate browser play. Replace the
package/filter with the code you changed. During [Journey200](JOURNEY_200.md),
the parent owns combined workspace, staged Wasm and visual checks after concurrent
changes settle. The [Journey50 baseline](JOURNEY_50_RESULTS.md) remains intact.
See the [agent guide](AGENT_GUIDE.md) for ownership and exact validation reporting.
