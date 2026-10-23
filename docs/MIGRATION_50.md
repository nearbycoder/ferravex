# Journey50 API migration cookbook

These recipes migrate existing Ferravex loops to the shared session, facade and
application APIs. They refer to source in this checkout, not a released-version
promise or a claim that all demos have passed integration. The
[Journey50 ledger](JOURNEY_50.md) owns completion status and the
[frozen session contract](JOURNEY_50.md#shared-session-contract-frozen-for-concurrent-implementation).
For a runnable starting point, use the [quickstart](QUICKSTART.md).

Rust blocks below are independent examples or complete helper functions. The
explicitly labeled **before** examples illustrate patterns to replace; compiling
them does not make their behavior correct. Assertions in the **after** examples
exercise the intended boundaries. Runtime hosts still need their own gameplay
and platform checks.

## 1. Move common imports to the facade

Before, a renderer-backed game can import the individual crates:

```rust
use ferravex_core::session::GameSession;
use ferravex_math::{Rect, Vec2};
use ferravex_render::Camera2D;

let session = GameSession::new(60.0, 8, true);
let position = Vec2::ZERO;
let camera = Camera2D::from_display_rect(Rect::new(0.0, 0.0, 640.0, 360.0));
assert!(session.is_playing());
```

After, use the facade's intentionally small prelude:

```rust
use ferravex::prelude::*;

let session = GameSession::new(60.0, 8, true);
let position = Vec2::ZERO;
let camera = Camera2D::from_display_rect(Rect::new(0.0, 0.0, 640.0, 360.0));
assert!(session.is_playing());
```

These are re-exports of the same types. Keep drawing and scheduling qualified as
`render::draw_circle`, `render::next_frame`, and so on. Specialized systems remain
explicit imports: `ferravex::app::{App, Game, AppControl}`,
`ferravex::input::{ActionHandle, ContextId}`, or
`ferravex::feedback::FeedbackController`. The facade prelude does not export
every module's types or the renderer's entire prelude.

In an existing workspace package, add `ferravex.workspace = true` to its
`[dependencies]` only when needed. Keep any direct crate dependency still named
by the source. A simulation-only crate can continue using `ferravex-core` and
`ferravex-play`; adopting the facade is optional. All dependency paths must remain
first-party under the [dependency policy](../DEPENDENCIES.md).

## 2. Replace duplicated local lifecycle flags with GameSession

Before, a host has to synchronize flags and its clock itself:

```rust
use ferravex_core::FixedClock;

struct LocalRun {
    clock: FixedClock,
    started: bool,
    paused: bool,
    finished: bool,
    elapsed: f32,
}

impl LocalRun {
    fn frame(&mut self, seconds: f64) {
        if !self.started || self.paused || self.finished { return; }
        let frame = self.clock.advance(seconds);
        for _ in 0..frame.steps {
            self.elapsed += frame.dt;
        }
    }
}
```

After, make the session the local host's start/pause/retry authority. This small
run finishes after one accepted update; it deliberately guards an already
scheduled catch-up batch:

```rust
use ferravex_core::session::{GameSession, SessionState};

struct LocalRun {
    session: GameSession,
    accepted_updates: u32,
    elapsed: f32,
}

impl LocalRun {
    fn new() -> Self {
        Self {
            session: GameSession::new(60.0, 8, false),
            accepted_updates: 0,
            elapsed: 0.0,
        }
    }

    fn frame(&mut self, seconds: f64) {
        let frame = self.session.advance(seconds);
        for _ in 0..frame.steps {
            if !self.session.is_playing() { break; }
            self.elapsed += frame.dt;
            self.accepted_updates += 1;
            self.session.finish(true);
        }
    }

    fn restart(&mut self) {
        self.accepted_updates = 0;
        self.elapsed = 0.0;
        self.session.restart();
    }
}

let mut run = LocalRun::new();
run.frame(30.0);
assert_eq!(run.accepted_updates, 0);
assert!(run.session.start());
run.session.set_paused(true);
run.frame(30.0);
assert_eq!(run.session.tick(), 0);
run.session.set_paused(false);
run.frame(2.0 / 60.0);
assert_eq!(run.accepted_updates, 1);
assert_eq!(run.session.state(), SessionState::Won);
assert_eq!(run.session.tick(), 2); // Issued ticks, including the skipped update.
assert!(!run.session.start());    // Starting does not retry a terminal run.
run.restart();
assert_eq!(run.session.tick(), 0);
assert_eq!(run.session.state(), SessionState::Playing);
```

The public session methods and their roles are:

| Signature | Behavior |
| --- | --- |
| `new(hz: f64, max_steps: u32, autoplay: bool) -> Self` | Ready unless autoplay; owns a fixed clock. Panics for invalid clock configuration. |
| `state(&self) -> SessionState` | Ready, Playing, Paused, Won or Lost. |
| `is_started(&self) -> bool` | True for everything except Ready, including terminal states. |
| `is_playing(&self) -> bool`, `is_paused(&self) -> bool` | Exact state predicates. |
| `start(&mut self) -> bool` | Ready to Playing only; returns whether it changed. |
| `set_paused(&mut self, paused: bool) -> bool`, `toggle_pause(&mut self) -> bool` | Transitions only between Playing and Paused. |
| `finish(&mut self, won: bool) -> bool` | Playing/Paused to Won/Lost once; cannot overwrite a result. |
| `restart(&mut self)` | Clears clock debt/ticks and enters Playing. |
| `reset(&mut self)` | Clears clock debt/ticks and enters Ready. |
| `advance(&mut self, elapsed_seconds: f64) -> FrameStep` | Bounded updates while Playing; otherwise zero steps, alpha and dropped time, with normal fixed dt. |
| `tick(&self) -> u64`, `dt(&self) -> f32` | Issued tick count and fixed duration. |

Keep game-specific phases that express countdowns, waves, serves, checkpoints or
round rules. Convert their accepted terminal transition to `session.finish(won)`.
Guard later updates in the same batch on that phase/session. The session's tick
is the count of **issued** steps, not proof that every rule update ran. Preserve
a simulation-owned tick for replay/network sequencing and early-exit accounting.

Measure elapsed time between host frames even while paused. If the host stops
delivering frames, rebase its timestamp before resuming; the session cannot infer
that a large active elapsed value was actually suspended time. Inactive time
does not accumulate, while a pre-pause fractional remainder is retained.

For rendering, shift current to previous before each actual update, and
interpolate once afterward. Inactive sessions return zero alpha: show current
state or snap both poses when pausing/finishing. Reset both snapshots on retry or
teleport. Track `FrameStep::dropped_seconds` separately from gameplay time.

## 3. Move manual hook ordering into App

Before, a small host wires initialization, fixed updates and cleanup itself:

```rust
use ferravex_core::session::GameSession;

#[derive(Default)]
struct Point { x: f32, active: bool }
impl Point {
    fn startup(&mut self) { self.active = true; }
    fn update(&mut self, dt: f32) {
        assert!(self.active);
        self.x += 64.0 * dt;
    }
    fn shutdown(&mut self) { self.active = false; }
}

let mut point = Point::default();
let mut session = GameSession::new(64.0, 4, true);
point.startup();
for seconds in [0.5 / 64.0, 2.0 / 64.0] {
    let frame = session.advance(seconds);
    for _ in 0..frame.steps { point.update(frame.dt); }
}
point.shutdown();
assert_eq!(point.x, 2.0);
assert!(!point.active);
```

After, `App<G>` owns a `Game` and its session. The host still supplies elapsed
time; no executor, window or wall clock is introduced:

```rust
use ferravex::app::{App, AppControl, AppError, Game};
use ferravex::prelude::GameSession;
use std::convert::Infallible;

#[derive(Default)]
struct Point {
    previous: f32,
    current: f32,
    visible: f32,
    startups: u32,
    shutdowns: u32,
}

impl Game for Point {
    type Error = Infallible;

    fn startup(&mut self) -> Result<AppControl, Self::Error> {
        self.startups += 1;
        Ok(AppControl::Continue)
    }

    fn fixed_update(&mut self, dt: f32) -> Result<AppControl, Self::Error> {
        self.previous = self.current;
        self.current += 64.0 * dt;
        Ok(AppControl::Continue)
    }

    fn render(&mut self, alpha: f32) -> Result<AppControl, Self::Error> {
        self.visible = self.previous + (self.current - self.previous) * alpha;
        Ok(AppControl::Continue)
    }

    fn shutdown(&mut self) -> Result<(), Self::Error> {
        self.shutdowns += 1;
        Ok(())
    }
}

fn main() -> Result<(), AppError<Infallible>> {
    let mut app = App::new(Point::default(), GameSession::new(64.0, 4, false));
    app.frame(10.0)?; // Starts the application, renders, but the session is Ready.
    assert_eq!(app.game().current, 0.0);
    app.session_mut().start();
    app.frame(2.5 / 64.0)?;
    assert_eq!(app.game().visible, 1.5);

    app.game_mut().previous = app.game().current; // Hold current pose while paused.
    app.session_mut().set_paused(true);
    app.frame(30.0)?;
    assert_eq!(app.game().visible, 2.0);
    app.stop()?;
    app.stop()?;
    assert_eq!((app.game().startups, app.game().shutdowns), (1, 1));
    assert_eq!(app.frame(1.0)?, AppControl::Stop);
    Ok(())
}
```

`Game::fixed_update(&mut self, dt: f32) -> Result<AppControl, Self::Error>` is
required. `startup`, `render(alpha: f32)` and `shutdown` have defaults. Construction
runs no hooks. `App::startup()` and `App::frame(elapsed_seconds)` return
`Result<AppControl, AppError<G::Error>>`; `App::stop()` returns
`Result<(), AppError<G::Error>>`. Access game/session data with
`game`/`game_mut` and `session`/`session_mut` between frames. Let `App::frame`
advance the session rather than calling `session_mut().advance` yourself.

An ordinary win/loss should keep the app alive to render its result screen.
Guard your game phase inside `fixed_update`, then call
`app.session_mut().finish(won)` between frames after observing that result.
`AppControl::Stop` instead skips remaining updates and rendering, shuts down,
and permanently stops the app. Session restart never repeats startup or revives
a stopped app; construct another app for another application lifecycle.

A hook error also stops the app and attempts shutdown. `AppError` preserves the
failing `hook`, primary `error`, and optional `shutdown_error`. Cleanup must
tolerate partial startup. Shutdown is attempted once after startup was attempted;
calling `stop()` on an unstarted app runs neither hook. Drop is a fallback that
discards cleanup errors, so explicitly stop when errors matter. No mutations are
rolled back and panics are not caught. Existing async `render::game!` loops can
stay as they are; `App` is an additive synchronous option.

## 4. Consume gameplay input only on actual ticks

Before, consuming an action before the update loop loses it when there are no
ticks and repeats it when there are several. This example exposes the repeat:

```rust
use ferravex::prelude::*;

let mut input = ActionMap::default();
let code = InputCode::Key(KeyCode::Space as u16);
let jump = input.action(vec![Binding::digital(code)]);
input.feed(code, 1.0);
let pressed = input.consume_tick(jump).pressed;
let mut session = GameSession::new(60.0, 8, true);
let frame = session.advance(2.0 / 60.0);
let mut requests = 0;
for _ in 0..frame.steps { requests += u32::from(pressed); }
assert_eq!(requests, 2); // One press was incorrectly applied twice.
```

After, resolve a typed handle once and consume inside each tick. A quick press
and release before a zero-tick frame still reaches the next simulation update:

```rust
use ferravex::input::{ContextId, InputError};
use ferravex::prelude::*;

fn main() -> Result<(), InputError> {
    let mut input = ActionMap::default();
    let context = input.create_context("gameplay")?;
    input.set_context(Some(context))?;
    let code = InputCode::Key(KeyCode::Space as u16);
    let jump = input.named_action_handle(
        "jump", ActionKind::Button, context, vec![Binding::digital(code)],
    )?;
    assert_ne!(context, ContextId::GLOBAL);
    assert_eq!(input.action_handle("jump"), Some(jump));

    input.feed(code, 1.0);
    input.feed(code, 0.0);
    let mut session = GameSession::new(60.0, 8, true);
    assert_eq!(session.advance(0.5 / 60.0).steps, 0);
    let frame = session.advance(2.0 / 60.0);
    let mut requests = 0;
    for _ in 0..frame.steps {
        let action = input.consume_tick_handle(jump)?;
        requests += u32::from(action.pressed);
    }
    assert_eq!(requests, 1);
    assert!(!input.peek_handle(jump)?.pressed);

    let other_map = ActionMap::default();
    assert_eq!(other_map.peek_handle(jump), Err(InputError::InvalidHandle));
    Ok(())
}
```

`named_action_handle(...) -> Result<ActionHandle, InputError>` registers and
returns the handle. `action_handle(name: &str) -> Option<ActionHandle>` resolves
an existing name; `handle_from_id(id: usize) -> Option<ActionHandle>` adapts an
index from the same map. `peek_handle` and `consume_tick_handle` return
`Result<Action, InputError>`. A handle survives rebinding and context changes,
but cannot be used with another/replaced map. It is process-local: save action
names, not handles. Copies share consumption; they are not subscriptions.

For a renderer host, `PlayerInput` already owns an action map and named profiles.
Use its tick snapshot rather than inventing a second input layer:

```rust
use ferravex::input::Action;
use ferravex::prelude::*;

fn fixed_inputs(
    input: &mut PlayerInput,
    frame: FrameStep,
    mut update: impl FnMut(f32, Vec2, Action, Action),
) {
    let movement = input.movement();
    for _ in 0..frame.steps {
        let [jump, fire] = input.take_tick(["jump", "fire"]);
        update(frame.dt, movement, jump, fire);
    }
}
```

The caller must register those action names and sample the host first. Its
`update` callback must guard any terminal game phase, as in recipe 2, so later
scheduled ticks cannot apply rules after a win/loss.
`take_tick<const N: usize>(&mut self, names: [&str; N]) -> [Action; N]` allocates
no per-tick storage. Share that array with physics/animation/rules; taking the
same action again consumes already-cleared edges. Duplicate names within one
snapshot share the result; unknown names are neutral. Held values persist, while
press/release flags and relative motion are consumed. Several taps before a tick
coalesce into edge flags, not a count.

Choose a pause owner:

- Existing local hosts can keep `sample_play()`, which consumes the global pause
  action, then reconcile `paused()`/`gameplay_active()` with their session.
- Session-owned hosts call `sample(enabled)`, consume registered global menu
  actions once per host frame, update session state, and reconcile gameplay
  enablement before ticking. `sample_play()` and manual pause handling must not
  both consume/toggle the same action.
- Keep manual pause intent separate from temporary focus/controls blockers.
  Closing a panel must not undo a user's manual pause. On a transition, clear
  stale actions through the adapter's disabling/context behavior before enabling
  gameplay again. Consume relative `look(dt)` once per host frame and use the
  resulting absolute aim in fixed/network commands.

Games with their own start, pause or result cards should finish their HUD with
`PlayerInput::draw_controls()`. It keeps the controls editor and customized touch
controls without drawing a second pause banner. `draw_settings()` retains its
standalone pause hint for hosts that rely on it.

Global actions expire on the next sampled render frame; gameplay edges wait for
a tick. Focus/context changes intentionally suppress stale held controls until
release. See [input details](INPUT.md) and the
[PlayerInput implementation/tests](../crates/ferravex-render/src/input/player.rs).

## 5. Route feedback from accepted gameplay events

Before, playing a fire cue on a button request can announce a shot that the rules
reject. This concrete counterexample has an empty magazine:

```rust
use ferravex::gameplay::magazine::Magazine;

let mut magazine = Magazine::new(1, 0, 0.5).unwrap();
assert!(magazine.fire());
let pressed = true;
let cue_count = u32::from(pressed); // Request-driven feedback is premature.
let accepted = pressed && magazine.fire();
assert!(!accepted);
assert_eq!(cue_count, 1);
```

After, reuse the magazine rules and emit only for acceptance. The same event ID
can be received again without replaying its feedback:

```rust
use ferravex::feedback::{FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackPolicy};
use ferravex::gameplay::magazine::Magazine;
use ferravex::prelude::Vec3;

let mut magazine = Magazine::new(1, 0, 0.5).unwrap();
let mut feedback = FeedbackController::<16>::new(FeedbackConfig::default(), 7).unwrap();
feedback.set_policy(FeedbackPolicy::new(true, true, 0.5).unwrap());
let event = FeedbackEvent::fire(Vec3::ZERO, 1.0).unwrap();
let mut cue_count = 0;

let accepted = magazine.fire();
if let Some(response) = feedback.emit_if_accepted(1, accepted, event).unwrap() {
    cue_count += 1; // The host sends response.cue/gain to its audio adapter here.
    assert_eq!(response.gain, 0.55);
    assert_eq!(response.particles, 0); // Reduced motion leaves audio unchanged.
}
assert!(feedback.emit(1, event).unwrap().is_none());
assert!(feedback.emit_if_accepted(2, magazine.fire(), event).unwrap().is_none());
assert_eq!((magazine.loaded(), cue_count), (0, 1));
let presentation = feedback.advance(1.0 / 60.0).unwrap();
assert_eq!(presentation.shake.translation, Vec3::ZERO);
assert_eq!(presentation.flash, 0.0);
assert_eq!(presentation.hitstop_seconds, 0.0);

feedback.clear(); // Restart: clears transient effects and remembered IDs.
assert!(feedback.emit(1, event).unwrap().is_some());
```

`emit(id: u64, event: FeedbackEvent)` and
`emit_if_accepted(id: u64, accepted: bool, event: FeedbackEvent)` return
`Result<Option<FeedbackResponse>, FeedbackError>`. Only `Some` plays effects;
`None` includes a duplicate or rejected action. Use unique, nonzero IDs for
accepted events, not a single ID per frame/tick when several events can occur.
The bounded recent-ID history supplements network acknowledgments; it does not
guarantee deduplication forever. Do not reapply gameplay damage when a duplicate
notification is received; deduplicate gameplay at its authority boundary too.

The example unwraps authored constants and a known bounded burst. A host must
handle `FeedbackError::Invalid`/`Capacity` without rolling back an accepted shot.
`FeedbackController<N>` supports `1..=128` recent IDs and at most N accepted events
between successful `advance(dt)` calls. `advance` requires finite seconds in
`0..=0.25`; select/bound presentation time separately from fixed simulation time.
`clear()` retains policy/configuration, the shake seed sequence and cumulative
`dropped` diagnostics. Reconstruct with the original seed if identical initial
shake playback is required.

`FeedbackPolicy::new(reduced_motion: bool, reduced_flash: bool, intensity: f32)`
validates visual intensity in `0..=1`. Reduced motion suppresses camera shake,
particle requests and presentation hit-stop; reduced flash suppresses flashes.
Authored sound and rumble are unchanged. Host-created effects must honor the
policy too. Never subtract presentation hit-stop from the authoritative clock.

## 6. Reuse camera and resource systems during reset/setup

Before, rebuilding a camera on retry silently discards authored settings:

```rust
use ferravex::camera::{Follow2, Follow2Settings};
use ferravex::prelude::Vec2;

let mut camera = Follow2::new(Follow2Settings {
    offset: Vec2::X * 4.0,
    ..Default::default()
}).unwrap();
assert_eq!(camera.settings().offset, Vec2::X * 4.0);
camera = Follow2::default();
assert_eq!(camera.settings().offset, Vec2::ZERO);
```

After, snap at a known spawn or reset tracking until the next valid sample:

```rust
use ferravex::camera::{Follow2, Follow2Settings};
use ferravex::prelude::*;

let mut camera = Follow2::new(Follow2Settings {
    offset: Vec2::X * 4.0,
    ..Default::default()
}).unwrap();
let mut session = GameSession::new(60.0, 8, true);
session.advance(1.5 / 60.0);
let spawn = vec2(100.0, 20.0);
session.restart();
let current = spawn;
let previous = spawn;
let view = camera.snap(spawn, Vec2::ZERO).unwrap();
assert!(view.cut);
assert_eq!(view.center, spawn + Vec2::X * 4.0);
assert_eq!(previous, current);
assert_eq!(session.tick(), 0);

camera.reset();
assert_eq!(camera.center(), None);
assert_eq!(camera.settings().offset, Vec2::X * 4.0);
assert!(camera.update(0.0, spawn, Vec2::ZERO).unwrap().cut);
```

`Follow2::snap(subject: Vec2, velocity: Vec2) -> Result<Frame2, CameraError>`
retains offsets/look-ahead/limits and cuts immediately. `reset()` forgets
tracking; `cut()` requests a cut on the next valid update. `update` accepts finite
presentation seconds in `0..=0.25`. Do not feed a suspension-sized delta to smooth
from a previous level. Game entities, audio voices, feedback and input debt still
need their own reset; `GameSession` and the camera do not own them.

Before, optional resources often cause repeated setup checks and eager work:

```rust
use ferravex::resources::Resources;

let mut resources = Resources::default();
if !resources.contains::<Vec<u32>>() {
    resources.insert(Vec::<u32>::new()).unwrap();
}
resources.get_mut::<Vec<u32>>().unwrap().push(7);
```

After, initialize once and use the checked required-resource boundary:

```rust
use ferravex::resources::Resources;

let mut resources = Resources::default();
resources.get_or_insert_with(Vec::<u32>::new).unwrap().push(7);
resources.get_or_insert_with(|| vec![99_u32]).unwrap();
assert_eq!(resources.require::<Vec<u32>>().unwrap(), &[7]);
assert!(resources.require::<u64>().is_err());

assert!(resources.try_init(|| "bad".parse::<u32>()).is_err());
assert!(!resources.contains::<u32>());
assert_eq!(*resources.try_init(|| "60".parse::<u32>()).unwrap(), 60);
assert_eq!(*resources.try_init(|| "bad".parse::<u32>()).unwrap(), 60);
```

`get_or_insert_with<T: Any>(initialize: impl FnOnce() -> T)` returns
`Result<&mut T, ResourceError>`; `try_init<T: Any, E>(initialize: impl FnOnce() ->
Result<T, E>)` returns `Result<&mut T, ResourceInitError<E>>`. Neither calls the
initializer for an existing resource, or for an absent type when capacity is
full. A failed initializer inserts nothing. `require<T: Any>()` and
`require_mut<T: Any>()` report the missing type. The store remains bounded to
256 distinct types; handle capacity and application initialization errors in a
real host. External side effects inside an initializer are not rolled back.

For other reusable rules, start with [gameplay recipes](GAMEPLAY_RECIPES.md):
`gameplay::buffer::CommandBuffer`, `gameplay::trial::Trial`,
`gameplay::checkpoint::Checkpoints`, `gameplay::waves::WaveSequence`, and
`gameplay::inventory::Inventory` already exist. Reuse [scene authoring](SCENES.md),
[persistence](SAVES.md) and [replay](REPLAYS.md) without changing formats just to
adopt a session. The APIs above supplement these systems rather than replacing
their domain rules.

## 7. Apply the migration to all fifteen demos

The paths below identify the inspected integration points and invariants to
preserve. They are a migration/review map, not a completion checklist. Per-demo
READMEs own controls; the [catalog](../examples/catalog.json) owns package/host
mapping. Keep each demo's existing fixed frequency, catch-up budget, autoplay
path and exported entry points unless a separate change explicitly requires it.

| Demo and integration source | Migration boundary and behavior to check |
| --- | --- |
| [Emberline](../examples/emberline/src/lib.rs) (`emberline`) | Coordinate start/pause/retry and goal completion with the session. Keep the character motor, jump forgiveness, checkpoints and trial rules. Consume jump once per tick; restart clears buffered input and presentation state. |
| [Neon Swarm](../examples/neon-swarm/src/run.rs) (`neon-swarm`) | Let the local run own session timing while `game::Game` owns waves/combat. Stop later catch-up updates on terminal phase; dash/fire feedback follows accepted actions. Reset actors, progression and transient effects together. |
| [Pocket Rally](../examples/pocket-rally/src/session.rs) (`pocket-rally`) | Keep countdown and race timing on fixed updates; countdown is not a second host clock. Pause preserves trial time. Reset lap/checkpoint progress and camera/skids/audio on retry; keep valid-lap and persistence rules. |
| [Salvage Pinball](../examples/salvage-pinball/src/game.rs) (`salvage-pinball`) | Session coordinates ready/play/pause/game-over. Preserve flipper holds, one-shot launch and three-ball rules. Zero-tick launch taps survive; a drained ball and its score/feedback must be applied once. |
| [Orchard Guard](../examples/orchard-guard/src/host.rs) (`orchard-guard`) | Keep pre-wave build planning and wave progression distinct from lifecycle. Preserve authored scene/navigation/save handling. Accept build/upgrade/wave actions once and preserve manual pause when closing settings. |
| [Marble Run](../examples/marble-run/src/trial.rs) (`marble-run`) | Coordinate course picker, countdown and trial start without accumulating paused time. Keep four courses, checkpoints and reload support; respawn/retry cuts camera history and cannot award completion twice. |
| [Dune Skimmer](../examples/dune-skimmer/src/session.rs) (`dune-skimmer`) | Retain fixed vehicle/race rules and custom shaders. Session pause freezes race timing; retry clears boost/control debt and camera/feedback history. Finish comes from accepted course progress. |
| [Crystal Cavern](../examples/crystal-cavern/src/lib.rs) (`crystal-cavern`) | Keep interaction/collection/gate rules authoritative. Queue interaction until a tick and consume once during catch-up; one press must not collect repeatedly. Preserve shader presentation and clear stale controls on retry/refocus. |
| [Siege Foundry](../examples/siege-foundry/src/lib.rs) (`siege-foundry`) | Keep rigid-body stepping and ammunition acceptance fixed. Session finish follows the range phase; rebuild resets bodies/ammo/timers/effects. Emit fire/destruction feedback only after accepted changes. |
| [Sky Courier](../examples/sky-courier/src/lib.rs) (`sky-courier`) | Session gates fixed flight, delivery progress and countdown. Preserve gate ordering and failure/win rules. Pausing cannot consume delivery time; retry resets flight/camera and pending input. |
| [Strike](../examples/strike/src/host.rs) (`strike`) | Use a session for offline practice. Keep network cadence and recorded replay ticks separate; menus neutralize local commands while network ticks continue. Explicit replay pause/seek remains replay-owned. Practice retry cannot reset network/replay state. |
| [2D Physics Lab](../examples/physics-2d/src/lib.rs) (`physics-2d`) | Pause regular updates; an explicit manual step calls physics with fixed `session.dt()` while staying paused. Preserve selected shape/pause on this lab's reset. Bound bodies and clear pending drop/cooldown state. |
| [3D Physics Lab](../examples/physics-3d/src/lib.rs) (`physics-3d`) | Keep fixed physics and explicit paused manual stepping. Reset world, orbit camera and feedback together; this lab's reset resumes its session. Keep body limits and deterministic autoplay inputs. |
| [Courtyard](../examples/owned-lab/src/lib.rs) (`owned-lab`, host `owned`) | Local interactive training waits for capture and can pause. Peer authority/prediction continues with neutral local commands. Preserve software renderer, headless/PNG paths, FPS tick sequencing, WebRTC formats and host exports. |
| [Orbit Breaker](../examples/owned-orbit/src/lib.rs) (`owned-orbit`) | Session wraps the original framebuffer host and continuous collision simulation. Keep serve/life-loss/sector rules in the game, retain launch taps through zero-tick frames, and consume restart once. Preserve CLI/Wasm exports and the platform-input regression. |

For manual-step labs, `GameSession` has no public single-step command. A deliberate
paused update with `session.dt()` advances the sandbox's own simulation counter;
it does not increment the session clock's issued ticks. Keep that distinction
visible in diagnostics and tests rather than temporarily unpausing for arbitrary
wall time.

For remote sessions, the rule is authority continuity: a local overlay never
freezes a live match. For replay, preserve recorded tick order and explicit
playback pause/seek semantics rather than replacing them with a local game clock.
Neutral local commands may preserve aim; they must clear movement/action debt.
See Strike and Courtyard's mode-specific hosts before copying a single-player
pause pattern into either.

## Compatibility and verification

This migration is additive. Existing `FixedClock`, individual crate imports,
renderer `game!` entry points, game-specific phases and host APIs remain usable.
Adopting `GameSession` or `App` does not authorize changing input bits, network
protocols, replay/save layouts or cooked assets. Format changes need explicit
versions and coordinated decoders, tests and documentation. Do not serialize a
new session enum or action handle into an old record implicitly.

Use focused checks for the boundary you change:

```sh
cargo test -p ferravex-core --test session --offline --locked -j 2
cargo test -p ferravex-engine --lib app::tests --offline --locked -j 2
cargo test -p ferravex-play --lib input --offline --locked -j 2
cargo test -p ferravex-render --lib input --offline --locked -j 2
cargo test -p ferravex-play --lib feedback --offline --locked -j 2
cargo test -p ferravex --doc --offline --locked -j 2
cargo test -p emberline --offline --locked -j 2
node examples/owned-orbit/tests/platform-input.mjs
```

Select the relevant commands, replacing the demo package as needed. Use
`--all-features` for the affected authored-content paths in Orchard Guard/Marble
Run. Assertions should cover zero/multiple tick input, pause without catch-up,
terminal updates, complete retry reset and the demo's actual success/failure
path. Network/replay hosts also need tests proving menu/focus events do not
alter authority or replay ordering. Compilation alone is not a playtest.

`cargo test --doc` checks crate Rustdoc, not standalone Markdown guides. To check
these guides' independent Rust blocks against the built facade and direct crate
imports, run this focused stdlib-only command from the repository root. It uses
Cargo's reported artifacts instead of guessing an rlib hash and writes only
normal build/test output:

```sh
python3 -B - <<'PY'
import json
import pathlib
import subprocess
import tempfile

build = subprocess.run([
    "cargo", "build", "-p", "ferravex", "--lib", "--offline", "--locked",
    "-j", "2", "--message-format=json",
], check=True, text=True, stdout=subprocess.PIPE)
needed = {"ferravex", "ferravex_core", "ferravex_math", "ferravex_render"}
libraries = {}
for line in build.stdout.splitlines():
    message = json.loads(line)
    if message.get("reason") != "compiler-artifact":
        continue
    name = message["target"]["name"]
    if name in needed:
        for filename in message["filenames"]:
            if filename.endswith(".rlib"):
                libraries[name] = pathlib.Path(filename)
if libraries.keys() != needed:
    raise SystemExit(f"Missing library artifacts: {needed - libraries.keys()}")
arguments = []
for directory in sorted({str(path.parent) for path in libraries.values()}):
    arguments += ["-L", f"dependency={directory}"]
for name, path in sorted(libraries.items()):
    arguments += ["--extern", f"{name}={path}"]
with tempfile.TemporaryDirectory(prefix="ferravex-doc-recipes-") as directory:
    for guide in ("docs/QUICKSTART.md", "docs/MIGRATION_50.md"):
        subprocess.run([
            "rustdoc", "--test", "--edition=2024", guide,
            "--test-run-directory", directory,
            "--test-args=--test-threads=2", *arguments,
        ], check=True)
PY
```

During Journey50, the parent owns final workspace/Wasm/visual validation. Report
the exact command, result and first cross-owner compilation failure if blocked;
do not edit another owner's files or infer a passed integration gate from source
presence. Module registrations for the documented session/prelude/app are present
in this checkout; the ledger and collected checks, not this cookbook, determine
whether the complete series is verified.
