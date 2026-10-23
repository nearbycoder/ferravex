# Journey200 API migration cookbook

Use these recipes to adopt the APIs present in the Journey200 source while
preserving the [Journey50 baseline](JOURNEY_50_RESULTS.md). They supplement the
[original migration cookbook](MIGRATION_50.md); they do not require replacing an
existing renderer loop, game rules, save format or network protocol.

The before examples are small, executable demonstrations of an older calling
pattern, not quotations from historical game revisions. Every Rust fence is a
complete independent binary, marked `rust,check,crate-type=bin`. The checks at
the end compile **and run** their assertions against the current facade. All
examples use only `ferravex` and Rust std, with default facade features. No window,
wall-clock sleep, storage write, browser or audio device is needed.

Source/API checks establish the behavior shown here. They do not establish fresh
Wasm linkage, browser presentation, device accessibility, audio quality,
performance or whole-workspace integration. Those checks remain with the
integration owner; implementation receipts are not substitutes for them.

## 1. Validate session settings and account for inspector steps

Before: `GameSession::new` is appropriate for trusted constants, but panics on
invalid configuration. An inspector can update its world directly while paused,
leaving the session's issued-tick count behind that update:

```rust,check,crate-type=bin
use ferravex::GameSession;

fn main() {
    let mut session = GameSession::new(64.0, 4, true);
    session.advance(0.5 / 64.0);
    session.set_paused(true);
    let simulated_seconds = session.dt(); // Caller performs one sandbox update.
    assert_eq!(simulated_seconds, 1.0 / 64.0);
    assert_eq!(session.tick(), 0); // That update never passed through the clock.
}
```

After: use `try_new(hz, max_steps, autoplay)` at a configuration boundary, or
reuse a validated `SessionConfig`. `step_paused()` issues exactly one tick and
returns the same `FrameStep` shape as `advance`:

```rust,check,crate-type=bin
use ferravex::core::FixedClockError;
use ferravex::core::session::{GameSession, SessionConfig, SessionState, SessionStepError};

fn main() {
    assert!(matches!(GameSession::try_new(64.0, 0, true),
        Err(FixedClockError::ZeroCatchUpBudget)));
    assert!(SessionConfig::try_new(f64::NAN, 4, true).is_err());
    let config = SessionConfig::try_new(64.0, 4, true).unwrap();
    let mut session = GameSession::from_config(config);
    assert_eq!(session.advance(0.5 / 64.0).steps, 0);
    session.set_paused(true);
    assert_eq!(session.advance(20.0).steps, 0);

    let step = session.step_paused().unwrap();
    let mut simulated_seconds = 0.0;
    for _ in 0..step.steps {
        simulated_seconds += step.dt;
    }
    assert_eq!(simulated_seconds, 1.0 / 64.0);
    assert_eq!((step.steps, step.alpha, step.dropped_seconds), (1, 1.0, 0.0));
    assert_eq!(session.state(), SessionState::Paused);
    assert_eq!(session.tick(), 1);

    session.set_paused(false);
    assert_eq!(session.advance(0.5 / 64.0).steps, 1); // Prior fraction survived.
    assert_eq!(session.tick(), 2);
    session.finish(true);
    assert_eq!(session.step_paused().unwrap_err(),
        SessionStepError::NotPaused { state: SessionState::Won });
    session.restart();
    assert_eq!(session.tick(), 0);
}
```

[Session implementation](../crates/ferravex-core/src/session.rs) and
[regressions](../crates/ferravex-core/tests/journey200_001.rs) define the boundary:
rates must be finite in `1..=1000`, and the catch-up budget must be nonzero.
`step_paused() -> Result<FrameStep, SessionStepError>` rejects Ready, Playing,
Won and Lost without mutation. It preserves fractional elapsed time, stays
paused and returns `alpha = 1` for the inspected current pose. Its tick counter
wraps like the existing clock; reset/restart clear it.

The returned step does not execute physics or input. The caller still performs
one fixed update, consumes pending gameplay edges for that update, and aligns
previous/current poses after inspector edits. Do not unpause temporarily to
manufacture elapsed time. `tick()` counts **issued** work, including an entire
`advance` batch; stop subsequent updates when game rules reach a terminal state.
Do not use that batch total as every update's unique event ID.

Both [2D Physics Lab](../examples/physics-2d/src/lib.rs) and
[3D Physics Lab](../examples/physics-3d/src/lib.rs) now call `step_paused` in their
real inspector paths. This supersedes Journey50's manual-step workaround.

## 2. Report executed hooks instead of inferring them from the clock

Before: `App::frame` returns application control, but a session tick difference
can overstate the number of update hooks executed after an early stop:

```rust,check,crate-type=bin
use ferravex::app::{App, AppControl, Game};
use ferravex::GameSession;
use std::convert::Infallible;

struct StopAfterOne(u32);
impl Game for StopAfterOne {
    type Error = Infallible;
    fn fixed_update(&mut self, _: f32) -> Result<AppControl, Self::Error> {
        self.0 += 1;
        Ok(AppControl::Stop)
    }
}
fn main() {
    let mut app = App::new(StopAfterOne(0), GameSession::new(64.0, 4, true));
    let before = app.session().tick();
    assert_eq!(app.frame(3.5 / 64.0).unwrap(), AppControl::Stop);
    assert_eq!(app.session().tick() - before, 3);
    assert_eq!(app.game().0, 1);
}
```

After: `frame_report` distinguishes scheduling, hook invocation and rendering;
`last_frame_report` also retains the partial result of an errored frame:

```rust,check,crate-type=bin
use ferravex::app::{App, AppControl, AppHook, Game};
use ferravex::GameSession;

#[derive(Default)]
struct Probe { updates: u32, cleanups: u32 }
impl Game for Probe {
    type Error = &'static str;
    fn fixed_update(&mut self, _: f32) -> Result<AppControl, Self::Error> {
        self.updates += 1;
        Err("update rejected")
    }
    fn shutdown(&mut self) -> Result<(), Self::Error> {
        self.cleanups += 1;
        Err("cleanup failed")
    }
}
fn main() {
    let mut app = App::new(Probe::default(), GameSession::new(64.0, 4, true));
    assert_eq!(app.last_frame_report(), None);
    let error = app.frame_report(6.5 / 64.0).unwrap_err();
    assert_eq!(error.hook, AppHook::FixedUpdate);
    assert_eq!(error.shutdown_error, Some("cleanup failed"));
    let report = app.last_frame_report().unwrap();
    assert_eq!((report.scheduled_updates, report.executed_updates), (4, 1));
    assert_eq!((report.dt, report.alpha), (1.0 / 64.0, 0.5));
    assert_eq!(report.dropped_seconds, 2.0 / 64.0);
    assert!(!report.rendered);
    assert_eq!(report.control, AppControl::Stop);
    assert_eq!(app.game().cleanups, 1);
    assert_eq!(app.frame_report(1.0).unwrap().executed_updates, 0);
    app.stop().unwrap();
    assert_eq!(app.game().cleanups, 1);
}
```

The [runner and its tests](../crates/ferravex-engine/src/app.rs) expose
`frame_report(f64) -> Result<AppFrameReport, AppError<G::Error>>` and
`last_frame_report() -> Option<AppFrameReport>`. `frame` remains supported and
updates the same retained report. An executed count includes a hook that returned
Stop or Err; it is not a count of successful game transactions. `rendered`
means the render hook ran, not that a GPU presented a frame. Dropped seconds
cover the clock's catch-up budget only, excluding early-stop skips and time
discarded by elapsed-time sanitization/capping. Cleanup stays exactly once;
the report does not roll back mutations or make a stopped app resumable.

## 3. Preserve repeating-timer phase and expose skipped expirations

Before: resetting a repeating countdown to its full interval on expiration loses
overshoot. These supplied deltas cross three boundaries, but only report two:

```rust,check,crate-type=bin
use std::time::Duration;

fn main() {
    let interval = Duration::from_millis(100);
    let mut remaining = interval;
    let mut events = 0;
    for milliseconds in [150, 150] {
        remaining = remaining.saturating_sub(Duration::from_millis(milliseconds));
        if remaining.is_zero() {
            events += 1;
            remaining = interval;
        }
    }
    assert_eq!(events, 2); // Reset discarded two 50 ms remainders.
}
```

After: `SimulationTimer` keeps the remainder and bounds reported work without
silently accumulating an unbounded backlog:

```rust,check,crate-type=bin
use ferravex::time::{SimulationTimer, TimerError};
use std::time::Duration;

fn main() {
    let interval = Duration::from_millis(100);
    let mut timer = SimulationTimer::repeating(interval, 2).unwrap();
    let mut events = 0;
    for _ in 0..2 {
        events += timer.advance(Duration::from_millis(150)).unwrap().expirations;
    }
    assert_eq!(events, 3);
    assert_eq!(timer.elapsed(), Duration::ZERO);
    let report = timer.advance(Duration::from_millis(550)).unwrap();
    assert_eq!((report.expirations, report.skipped), (2, 3));
    assert_eq!(timer.elapsed(), Duration::from_millis(50));
    assert_eq!(timer.advance(Duration::ZERO).unwrap().expirations, 0);
    timer.set_paused(true);
    timer.advance(Duration::from_secs(30)).unwrap();
    timer.set_paused(false);
    assert_eq!(timer.advance(Duration::from_millis(50)).unwrap().expirations, 1);
    assert_eq!(SimulationTimer::once(Duration::ZERO).unwrap_err(), TimerError::ZeroInterval);
}
```

The [time module](../crates/ferravex-engine/src/time.rs) provides
`once(Duration)` and `repeating(Duration, u32)`, returning
`Result<SimulationTimer, TimerError>`. `advance(Duration)` returns a
`TimerReport { expirations: u32, skipped: u128 }`; skipped expirations are
discarded, not deferred. Zero intervals/limits are errors. Overflow leaves state
unchanged. `reset` clears progress but preserves pause.

Feed durations from **executed simulation updates**, or deliberately select a
`SimulationTime` scaled/unscaled delta for a frame-driven system. Do not also
feed render elapsed time to a gameplay timer. Integer nanoseconds preserve the
supplied duration; conversion from a floating fixed `dt` may round. Local menus
must not pause authority-owned timers in network/replay modes.

## 4. Reuse scheduler output without changing task identity

Before: `drain_ready` collects a new vector. Cancelling and reinserting an older
task changes its identity and its FIFO position among equal deadlines:

```rust,check,crate-type=bin
use ferravex::scheduler::TickScheduler;

fn main() {
    let mut tasks = TickScheduler::new(2);
    let first = tasks.schedule(20, "first").unwrap();
    tasks.schedule(10, "second").unwrap();
    let payload = tasks.cancel(first).unwrap();
    let replacement = tasks.schedule(10, payload).unwrap();
    assert_ne!(first, replacement);
    assert_eq!(tasks.drain_ready(10, 2), ["second", "first"]);
}
```

After: reschedule the original ID and use caller-owned storage or lazy draining:

```rust,check,crate-type=bin
use ferravex::scheduler::TickScheduler;

fn main() {
    let mut tasks = TickScheduler::new(2);
    let first = tasks.schedule(20, "first").unwrap();
    tasks.schedule(10, "second").unwrap();
    assert_eq!(tasks.reschedule(first, 10), Some(20));
    let mut ready = Vec::with_capacity(2);
    assert_eq!(tasks.drain_ready_into(10, 1, &mut ready), 1);
    assert_eq!(ready, ["first"]);
    assert_eq!(tasks.len(), 1);
    let capacity = ready.capacity();
    ready.clear();
    assert_eq!(tasks.drain_ready_into(10, 2, &mut ready), 1);
    assert_eq!(ready, ["second"]);
    assert_eq!(ready.capacity(), capacity);
    assert_eq!(tasks.reschedule(first, 30), None); // Already drained.

    tasks.schedule(11, "a").unwrap();
    tasks.schedule(11, "b").unwrap();
    {
        let mut due = tasks.drain_ready_iter(11, 2);
        assert_eq!(due.next(), Some("a"));
    }
    assert_eq!(tasks.len(), 1); // Dropping the iterator did not discard "b".
}
```

[Scheduler implementation](../crates/ferravex-engine/src/scheduler.rs) and
[allocation regressions](../crates/ferravex-engine/tests/journey200_005.rs):
`reschedule(TaskId, u64) -> Option<u64>` returns the old deadline and retains
original tie order, even when full. `drain_ready_into(u64, usize, &mut Vec<T>)
-> usize` appends only into spare capacity; clear/consume that output before
reusing it. A zero-capacity output removes nothing. Iterator draining removes
only yielded items. These drain paths allocate no output storage, but scheduling
and rescheduling may allocate tree nodes. IDs are scheduler-local; tick inputs
are absolute, caller-owned deadlines, not wall-clock timestamps.

## 5. Derive control hints and text measurements from current bindings

Before: a fixed label survives a rebind and directs the player to the wrong key:

```rust,check,crate-type=bin
use ferravex::prelude::{ActionKind, Binding, InputCode, KeyCode, PlayerInput};

fn main() {
    let mut input = PlayerInput::transient("migration-hints-before");
    input.action("jump", "Jump", ActionKind::Button,
        vec![Binding::digital(InputCode::Key(KeyCode::Space as u16))]).unwrap();
    let prompt = "Space";
    input.rebind("jump",
        vec![Binding::digital(InputCode::Key(KeyCode::J as u16))]).unwrap();
    assert_eq!(prompt, "Space"); // Still displays the former binding.
    assert_eq!(input.bindings("jump").unwrap()[0].code, InputCode::Key(KeyCode::J as u16));
}
```

After: use the input adapter's bounded formatter and the same bitmap geometry
that will draw the label:

```rust,check,crate-type=bin
use ferravex::prelude::{ActionKind, Binding, InputCode, KeyCode, PlayerInput};
use ferravex::render::{self, input::{HintDevice, HintError}};

fn main() {
    let mut input = PlayerInput::transient("migration-hints-after");
    input.action("jump", "Jump", ActionKind::Button,
        vec![Binding::digital(InputCode::Key(KeyCode::Space as u16))]).unwrap();
    let mut short = [b'!'; 2];
    assert!(matches!(input.format_hint("jump", &mut short),
        Err(HintError::BufferTooSmall { required: 5 })));
    assert_eq!(&short, b"!!");
    input.rebind("jump", vec![
        Binding::digital(InputCode::Key(KeyCode::J as u16)),
        Binding::digital(InputCode::PadButton(0)),
    ]).unwrap();
    let mut buffer = [0; 128];
    assert_eq!(input.format_hint("jump", &mut buffer).unwrap(), "J");
    assert_eq!(input.format_hint_for("jump", Some(HintDevice::Gamepad), &mut buffer).unwrap(), "Pad 0");
    let text = input.format_hint_for("jump", None, &mut buffer).unwrap();
    assert_eq!(text, "J / Pad 0");
    let metrics = render::get_default_font().try_metrics(16, 1.0).unwrap();
    let measured = metrics.measure_line(text).unwrap();
    assert_eq!((measured.width, measured.unsupported_glyphs), (108.0, 0));
    let x = (320.0 - measured.width) * 0.5;
    let baseline = 20.0 + metrics.baseline();
    assert_eq!((x, baseline), (106.0, 34.0));
    input.rebind("jump", vec![]).unwrap();
    assert_eq!(input.format_hint("jump", &mut buffer).unwrap(), "Unbound");
}
```

[Hint formatting](../crates/ferravex-render/src/input/player/editor.rs) returns a
string borrowing the supplied byte slice. `format_hint` uses `current_device()`;
`format_hint_for` takes `Option<HintDevice>`, with `None` selecting all families.
Unknown actions and small buffers return `HintError`; an error preserves the
buffer. Formatting neither polls input nor consumes edges. Real persistent
profiles load on first `sample`; generate their hints after sampling. Pad/touch
numbers identify channels, not hardware glyphs or guaranteed device support.

[Font metrics](../crates/ferravex-render/src/font.rs) validate size/scale and
measured extent. `measure_line` is for the immediate bitmap renderer, including
blank advance for unsupported characters; it does not wrap text or implement
the separate UI renderer's fallback. The example calculates layout but does not
draw it. See [Emberline's presentation adapter](../examples/emberline/src/presentation.rs)
and [Marble Run's HUD](../examples/marble-run/src/hud.rs) for actual consumers.

Hints do not authorize gameplay or consume it. Preserve the existing
`PlayerInput::take_tick` pattern: sample held state once, consume gameplay edges
inside actual fixed updates, and share the resulting snapshot with rules and
animation. Global menu actions stay in the host frame. Keep one pause owner;
do not combine automatic `sample_play` toggling with another toggle of the same
edge. A controls/focus blocker must not erase manual pause intent.

## 6. Synchronize live visual policy before accepted-event feedback

Before: copying presentation settings only at initialization leaves a consumer
stale when the host publishes a new policy:

```rust,check,crate-type=bin
use ferravex::feedback::{FeedbackController, FeedbackPolicy};
use ferravex::render::preferences;

fn main() {
    preferences::set_feedback_policy(FeedbackPolicy::default());
    let mut feedback = FeedbackController::<8>::new(Default::default(), 1).unwrap();
    feedback.set_policy(preferences::feedback_policy());
    preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.0).unwrap());
    assert_ne!(feedback.policy(), preferences::feedback_policy());
}
```

After: sync on every presentation frame, including menus and zero-tick frames.
Use `advance_with_impact` **instead of** `advance` to obtain paired samples:

```rust,check,crate-type=bin
use ferravex::feedback::{FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackPolicy};
use ferravex::math::Vec3;
use ferravex::render::preferences;

fn main() {
    let config = FeedbackConfig { networked: true, ..Default::default() };
    let mut feedback = FeedbackController::<8>::new(config, 1).unwrap();
    preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.0).unwrap());
    assert!(preferences::sync_feedback(&mut feedback));
    let accepted_hit = FeedbackEvent::hurt(Vec3::ZERO, 1.0).unwrap();
    let response = feedback.emit(1, accepted_hit).unwrap().unwrap();
    assert!(response.gain > 0.0); // The host may submit this accepted cue.
    assert_eq!(response.particles, 0);
    assert!(feedback.emit(1, accepted_hit).unwrap().is_none());
    let (frame, impact) = feedback.advance_with_impact(1.0 / 60.0).unwrap();
    assert_eq!(impact.emphasis, 0.0); // Reduced motion.
    assert!(impact.flash > 0.0); // Flash has its own preference.
    assert_eq!(impact.flash, frame.flash);
    assert_eq!(frame.hitstop_seconds, 0.0); // Network authority is untouched.

    preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.0).unwrap());
    preferences::sync_feedback(&mut feedback); // Also works with zero gameplay ticks.
    assert_eq!(feedback.impact().flash, 0.0);
    preferences::set_feedback_policy(FeedbackPolicy::default());
    preferences::sync_feedback(&mut feedback);
    assert_eq!(feedback.impact().flash, 0.0); // Cleared effects cannot revive.
}
```

[Preferences](../crates/ferravex-render/src/preferences.rs) expose
`set_feedback_policy(FeedbackPolicy) -> bool`, `feedback_policy() -> FeedbackPolicy`
and `sync_feedback<const N: usize>(&mut FeedbackController<N>) -> bool`.
These are thread-local, session-local operations, independent of persistence
and simulation. `try_apply_with_feedback(Preferences, FeedbackPolicy)` applies
validated audio/graphics settings and policy together, returning
`Result<ApplyOutcome, StorageError>`; it does not save or poll consumers.

[Feedback](../crates/ferravex-play/src/feedback.rs) remains bounded: nonzero event
IDs, `1..=128` recent IDs, at most N accepted events between successful advances,
finite presentation deltas in `0..=0.25`. Handle Invalid/Capacity errors without
undoing accepted game rules. IDs are only deduplicated within retained history.
`advance_with_impact(f32) -> Result<(FeedbackFrame, ImpactSample), FeedbackError>`
samples flash/emphasis before decay. Render its flash once; the two returned
flash values describe the same channel. Do not apply presentation hit-stop to
authoritative ticks.

The sync operation clears disabled controller channels. Separately owned dust,
trails, shader pulses and camera impulses must inspect the shared policy too,
including clearing already emitted cosmetics. Reduced motion and reduced flash
are independent; audio/rumble and gameplay damage remain independent of both.
See [Neon Swarm](../examples/neon-swarm/src/presentation.rs),
[Sky Courier](../examples/sky-courier/src/presentation.rs) and
[Strike](../examples/strike/src/effects.rs).

## 7. Distinguish invalid physics queries, misses and truncated results

Before: the compatible nearest-hit convenience method returns `None` for both
invalid input and a valid miss:

```rust,check,crate-type=bin
use ferravex::physics::d2::{PhysicsWorld, QueryOptions};

fn main() {
    let world = PhysicsWorld::default();
    let options = QueryOptions::new(10.0);
    assert!(world.raycast_with_options([0.0; 2], [0.0; 2], options).is_none());
    assert!(world.raycast_with_options([0.0; 2], [1.0, 0.0], options).is_none());
}
```

After: query into a fixed caller buffer and preserve the validation error:

```rust,check,crate-type=bin
use ferravex::physics::d2::{PhysicsWorld, QueryError, QueryOptions};

fn main() {
    let mut world = PhysicsWorld::default();
    world.add_ball([6.0, 0.0], 0.5, false);
    let near = world.add_ball([2.0, 0.0], 0.5, false);
    let options = QueryOptions::new(10.0);
    let mut hits = [None; 1];
    let report = world.raycast_into([0.0; 2], [1.0, 0.0], options, &mut hits).unwrap();
    assert_eq!((report.written, report.total), (1, 2));
    assert!(report.is_truncated());
    assert_eq!(hits[0].unwrap().body, Some(near));
    assert_eq!(world.raycast_into([0.0; 2], [0.0; 2], options, &mut hits),
        Err(QueryError::ZeroDirection));
    assert_eq!(hits[0].unwrap().body, Some(near)); // Error preserves output.
    let miss = world.raycast_into([0.0; 2], [0.0, 1.0], options, &mut hits).unwrap();
    assert_eq!((miss.written, miss.total), (0, 0));
    assert!(hits[0].is_none()); // Successful miss clears old hits.
}
```

The [physics API](../crates/ferravex-physics/src/api.rs) offers
`raycast_into(origin, direction, QueryOptions, &mut [Option<RayHit>]) ->
Result<QueryReport, QueryError>` for both `d2` and `d3`; array lengths match the
dimension. `spherecast_into` adds a radius before options. Only the nearest hits
fit, sorted by distance; `total` counts accepted **colliders**, including separate
compound children. A full slice is not automatically truncated. A zero-length
slice still validates and counts. Queries inspect current body/collider poses,
not interpolated render poses, and do not step or wake the world. Convex sweeps
retain their finite-iteration/grazing-hit limitation.

Keep `QueryOptions` layer/body/sensor filters; handles must belong to this world.
The facade build used here exposes both dimensions. Direct physics users must
enable the relevant `dim2`/`dim3` feature. `ferravex::dynamics::PhysicsWorld` is a
separate translational solver, not the rotating-body API in this recipe. See
[3D Physics Lab's bounded inspector](../examples/physics-3d/src/lib.rs).

## 8. Keep absolute ticks integer when interpolating snapshots

Before: converting an entire tick to `f64` loses sub-tick precision at large
tick values, even though the bounded snapshot history contains both endpoints:

```rust,check,crate-type=bin
use ferravex::snapshots::SnapshotRing;

fn main() {
    let base = 1_u64 << 60;
    let mut history = SnapshotRing::new(2);
    history.push(base, 0.0_f64).unwrap();
    history.push(base + 2, 8.0).unwrap();
    let (before, after) = history.bracket(base + 1);
    assert!(before.is_some() && after.is_some());
    let absolute = (base + 1) as f64 + 0.5;
    assert_eq!(absolute, base as f64); // Query collapsed onto the older tick.
}
```

After: pass an integer tick and a checked fractional tick separately:

```rust,check,crate-type=bin
use ferravex::snapshots::{SnapshotBracket, SnapshotQueryError, SnapshotRing};

fn main() {
    let base = 1_u64 << 60;
    let mut history = SnapshotRing::new(2);
    history.push(base, 0.0_f64).unwrap();
    history.push(base + 2, 8.0).unwrap();
    let position = match history.bracket_checked(base + 1, 0.5).unwrap() {
        SnapshotBracket::Between { before: (_, a), after: (_, b), alpha } =>
            a + (b - a) * alpha,
        other => panic!("expected interpolation, got {other:?}"),
    };
    assert_eq!(position, 6.0);
    assert!(matches!(history.bracket_checked(base + 2, 0.5).unwrap(),
        SnapshotBracket::AfterNewest { .. }));
    assert_eq!(history.bracket_checked(base, f64::NAN), Err(SnapshotQueryError::InvalidFraction));
    assert_eq!(history.bracket_checked(u64::MAX, 1.0), Err(SnapshotQueryError::TickOverflow));
    assert_eq!(history.len(), 2);
}
```

[SnapshotRing](../crates/ferravex-engine/src/snapshots.rs) adds
`bracket_checked(u64, f64) -> Result<SnapshotBracket<'_, T>, SnapshotQueryError>`.
The fraction is finite in `0..=1`; exactly one carries to the next tick.
Empty, BeforeOldest, Exact, Between and AfterNewest are distinct outcomes. Only
Between supplies an interpolation factor. Decide explicitly whether out-of-range
presentation waits, holds an endpoint or extrapolates; the query does none of
these automatically. Storage still requires strictly increasing nonwrapping
ticks. A reset/rollback needs the existing clear-history/rebranch policy, not a
floating conversion of a wrapping session tick. These helpers do not change
replay packets, simulation ordering or determinism guarantees.

For recorded pose ghosts, inspect the separate
[GhostTrack checked sampler](../crates/ferravex-play/src/replay/ghost.rs) and
[Pocket Rally's integer tick/alpha use](../examples/pocket-rally/src/sim.rs).
Snapshot and ghost result types are not interchangeable.

## 9. Compare results against the previous accepted best

Before: reading the best after saving an attempt loses the prior value and can
make a new best appear to tie itself:

```rust,check,crate-type=bin
use ferravex::persistence::RecordBook;

fn main() {
    let mut book = RecordBook::default();
    book.record("course", 7, 100, Some(2_000_000)).unwrap();
    let attempt = 1_500_000;
    book.record("course", 7, 90, Some(attempt)).unwrap();
    let comparison_after_save = book.get("course", 7).unwrap().fastest_us.unwrap();
    assert_eq!(attempt - comparison_after_save, 0); // Prior 500 ms improvement is gone.
}
```

After: preview without mutation when needed, then use the summary from actual
acceptance for result text/feedback:

```rust,check,crate-type=bin
use ferravex::persistence::{RecordBook, RecordComparison, SaveData};

fn main() {
    let mut book = RecordBook::default();
    book.record("course", 7, 100, Some(2_000_000)).unwrap();
    let preview = book.compare("course", 7, 90, Some(1_500_000)).unwrap();
    assert_eq!(book.get("course", 7).unwrap().runs, 1);
    let accepted = book.record_with_summary("course", 7, 90, Some(1_500_000)).unwrap();
    assert_eq!(preview, accepted);
    assert_eq!(accepted.score.comparison, RecordComparison::Worse);
    let time = accepted.time_us.unwrap();
    assert_eq!(time.comparison, RecordComparison::NewBest);
    assert_eq!((time.previous, time.delta), (Some(2_000_000), Some(-500_000)));
    assert_eq!(accepted.runs, 2);
    let before_error = book.clone();
    assert!(book.record_with_summary("course", 7, 999, Some(0)).is_err());
    assert_eq!(book, before_error);
    assert_eq!(<RecordBook as SaveData>::VERSION, 1);
}
```

[RecordBook](../crates/ferravex-play/src/persistence/records.rs) provides
`compare(&self, id: &str, context: u64, score: u64, time_us: Option<u64>)`
and `record_with_summary(&mut self, ...)`, both returning
`Result<RecordSummary, SaveError>`. Scores prefer larger values; successful times
prefer smaller values. Signed deltas are exact `i128` differences. `None` supplies
no successful time, not a tie or zero-duration win. Previews reserve nothing;
another accepted attempt may change the next summary.

The book still holds at most 24 identities, preserves insertion-order encoding
and version 1, and counts accepted worse/tied attempts as runs. Validation errors
preserve the book. In-memory acceptance does not establish durable storage:
handle the existing store's save error separately. See
[Marble Run timing](../examples/marble-run/src/timing.rs) and
[Pocket Rally presentation](../examples/pocket-rally/src/presentation.rs).

## All fifteen demo migration entry points

Each row links the actual game and the source that consumes relevant APIs.
These are implementation references, not browser/playtest completion claims.
The [catalog](../examples/catalog.json) remains the package/host mapping source;
Courtyard is package `owned-lab`, host `owned`. Preserve each game's own fixed
frequency, catch-up limit, authored content, result rules and smoke/export paths.

| Demo | Concrete Journey200 adoption to inspect | Compatibility boundary |
| --- | --- | --- |
| [Emberline](../examples/emberline/README.md) | [Presentation](../examples/emberline/src/presentation.rs) uses live hints/checked font metrics and independent cosmetic policy; [game loop](../examples/emberline/src/lib.rs) configures camera look-ahead dead zone. | Checkpoint/respawn acceptance controls resets; keep the character motor's jump forgiveness and consume jump edges on ticks. |
| [Neon Swarm](../examples/neon-swarm/README.md) | [Presentation owner](../examples/neon-swarm/src/presentation.rs) syncs feedback and samples impact; [combat/HUD](../examples/neon-swarm/src/game.rs) exposes encounter admission and ability availability. | Cosmetic suppression must not alter combat invulnerability, waves or accepted shots. |
| [Pocket Rally](../examples/pocket-rally/README.md) | [Simulation](../examples/pocket-rally/src/sim.rs) uses `Route3::project` and checked integer-tick ghost sampling; [presentation](../examples/pocket-rally/src/presentation.rs) compares lap records. | Gate crossings still award laps; preserve car handling, countdown and ghost/save bytes. |
| [Salvage Pinball](../examples/salvage-pinball/README.md) | [Presentation](../examples/salvage-pinball/src/app/presentation.rs) reads shared policy/live hints; [host](../examples/salvage-pinball/src/app.rs) measures labels and pauses/resumes `GameAudio`. | Contact acceptance owns score; clearing sparks/popups cannot change bumper debounce, ball drains or three-ball rules. |
| [Orchard Guard](../examples/orchard-guard/README.md) | [Game](../examples/orchard-guard/src/game.rs) uses `TargetSelector` and `EffectSet::status`; [host loop](../examples/orchard-guard/src/lib.rs) takes action snapshots on ticks. | Preserve authored content/hot reload, integer build affordability and wave/save semantics; menus must retain manual pause intent. |
| [Marble Run](../examples/marble-run/README.md) | [Timing](../examples/marble-run/src/timing.rs) uses accepted record summaries; [presentation](../examples/marble-run/src/presentation.rs) frames course bounds; [HUD](../examples/marble-run/src/hud.rs) measures live hints. | Checkpoints earn splits; recovery is consumed on a tick. Keep all four courses, picker/countdown lifecycle and existing record format. |
| [Dune Skimmer](../examples/dune-skimmer/README.md) | [Race](../examples/dune-skimmer/src/race.rs) reuses route projection and planar telemetry; [presentation](../examples/dune-skimmer/src/presentation.rs) applies independent dust/pulse policy. | Preserve accepted gates, boost rearm/depletion and recovery penalties; shaders remain presentation consumers. |
| [Crystal Cavern](../examples/crystal-cavern/README.md) | [Game/host](../examples/crystal-cavern/src/lib.rs) uses stable visible-target selection, objective inspection and remapping-aware prompts. | Input does not complete an objective by itself; collect/altar/exit acceptance still owns the six-part result. |
| [Siege Foundry](../examples/siege-foundry/README.md) | [Game/host](../examples/siege-foundry/src/lib.rs) consumes fire with `take_tick`, inspects aim and applies live recoil/pulse policy. | Fire feedback follows ammunition/shot acceptance; inspector previews must not step physics or spend ammo. |
| [Sky Courier](../examples/sky-courier/README.md) | [Flight](../examples/sky-courier/src/lib.rs) uses shared motor telemetry; [presentation](../examples/sky-courier/src/presentation.rs) syncs accepted feedback and camera recovery. | Ordered gates and delivery conditions remain authoritative; cosmetic time cannot spend the flight deadline. |
| [Strike](../examples/strike/README.md) | [Controls](../examples/strike/src/controls.rs) consume tick snapshots; [effects](../examples/strike/src/effects.rs) sync feedback; [replay](../examples/strike/src/replay.rs) exposes actual completion. | [Mode-specific host](../examples/strike/src/host.rs) preserves network cadence under local menus and replay-owned pause/seek; practice retry cannot reset those modes. |
| [2D Physics Lab](../examples/physics-2d/README.md) | [Lab](../examples/physics-2d/src/lib.rs) calls `step_paused`, checked queries, current-body inspection and contact-start feedback. | Keep selected shape/pause reset behavior, body bounds and current-pose query semantics; selection does not wake bodies. |
| [3D Physics Lab](../examples/physics-3d/README.md) | [Lab](../examples/physics-3d/src/lib.rs) uses checked session setup/stepping, bounded ray results and sleep/contact diagnostics. | Keep the 120 Hz simulation, body bound, pose synchronization and reset-resumes behavior. |
| [Courtyard](../examples/owned-lab/README.md) | [Runtime](../examples/owned-lab/src/lib.rs) owns audio state in TLS; [training](../examples/owned-lab/src/training.rs) reuses objectives; [HUD](../examples/owned-lab/src/hud.rs) uses current hints. | Peer authority/prediction continues with neutral local commands. Keep software/headless hosts and packet ABI; Rust batch scene edits do not make browser row-by-row imports atomic. |
| [Orbit Breaker](../examples/owned-orbit/README.md) | [Presentation](../examples/owned-orbit/src/presentation.rs) consumes accepted events/impact policy; [runtime](../examples/owned-orbit/src/lib.rs) syncs before update/render. | Preserve continuous collision, serve/life/sector rules, framebuffer/native CLI/Wasm ABI and zero-tick launch retention. |

## Compatibility boundaries beyond the recipes

- **Imports and lifecycle:** direct crates, `FixedClock`, `GameSession::new`,
  `App::frame` and renderer `game!` entry points remain supported. These recipes
  use registered facade paths; they require no new root registration or Cargo
  dependency. Optional content features still require their own validation.
  Restart clears timing, not entities, pending input, camera history, audio,
  accepted-event serials or feedback owned by the game.
- **Audio ownership:** [GameAudio](../crates/ferravex-render/src/audio/game.rs)
  is thread-affine (`!Send`/`!Sync`) and tied to the current renderer audio
  instance. Keep it on its owner thread; Courtyard uses `thread_local!` with
  `RefCell<Option<Lab>>`, not a process-global `Mutex<Option<Lab>>` requiring
  `Send`. Do not add an unsafe Send workaround. Pause/resume/reset govern
  gameplay voices and cadence, not the authority clock or user preferences.
- **Browser visual ABI:** the shared
  [renderer export](../crates/ferravex-render/src/lib.rs)
  `ferravex_feedback_preferences(motion: u32, flash: u32) -> u32` accepts only
  independent 0/1 flags. It returns 1 for valid flags, including unchanged
  values; invalid flags return 0 without changing policy. Intensity is preserved.
  Use the common export for custom hosts too. Actual linkage and visible policy
  adoption require a fresh game build and browser check, not just the export's
  source or a store-only test.
- **Web player settings:** [player.js](../web/player.js) exposes a frozen
  `presentationPreferences` snapshot with `reducedMotion: 'system' | boolean`
  and `reducedFlash: boolean`. Current backup export is **version 2**; strict
  imports accept versions 1 and 2. Version 1 permits only the original five
  fields, while version 2 additionally permits `flash`. Sparse imports preserve
  omitted choices, so an old backup does not clear flash preference. The storage
  key stays `ferravex.player.v1.<game>`; backup version and local-storage key are
  different contracts. Preview/Cancel do not apply settings. Effective URL-masked
  quality must not overwrite the saved choice. See the
  [bridge source test](../web/tests/journey200-visual-bridge.mjs).
- **Replay completion:** Strike adds the Wasm function
  `ferravex_replay_finished() -> u32` from actual playback state. A stationary
  cursor is not evidence of completion: playback may be paused or faulted.
  Preserve the existing state bits, explicit replay pause/seek and format;
  source export presence does not establish that a staged panel consumes it.
- **Formats, bounds and dependency policy:** snapshots, action handles, frame
  reports and inspection summaries are not permission to append bytes to old
  saves, replays, cooked content or network packets. Preserve explicit versions,
  decoders, errors and storage bounds; coordinate any actual format change with
  tests/docs. Rust remains first-party/std-only; Python tools use stdlib and
  browser hosts use standard APIs. See [DEPENDENCIES](../DEPENDENCIES.md).

## Reproduce the documentation checks

Run commands from the repository root. For a read-only link/fence check:

```sh
python3 -B scripts/check-docs.py docs/MIGRATION_200.md --json
```

`check-docs.py --check-rust` checks marked crates using `rustc --emit=metadata`;
it **does not run assertions**. The command below builds only the facade library
and its dependencies with two jobs, selects Cargo's actual facade/core artifacts,
checks links and types, then links and runs every marked binary. It uses a unique
temporary target/extraction directory and removes it afterward. It neither
changes manifests/lockfiles nor stages `dist`. Native linker prerequisites
still apply, even though no example opens a window. A cross-compilation target
override is unsuitable here because the resulting binaries must run locally.

```sh
python3 -B - <<'PY'
import json
from pathlib import Path
import subprocess
import tempfile

with tempfile.TemporaryDirectory(prefix="ferravex-migration-200-") as directory:
    work = Path(directory)
    build = subprocess.run([
        "cargo", "build", "-p", "ferravex", "--lib", "--offline", "--locked",
        "-j", "2", "--message-format=json", "--target-dir", str(work / "target"),
    ], check=True, text=True, stdout=subprocess.PIPE)
    needed = {"ferravex", "ferravex_core"}
    libraries = {}
    for line in build.stdout.splitlines():
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact"
                and message["target"]["name"] in needed):
            for filename in message["filenames"]:
                if filename.endswith(".rlib"):
                    libraries[message["target"]["name"]] = Path(filename)
    if libraries.keys() != needed:
        raise SystemExit(f"Missing library artifacts: {needed - libraries.keys()}")
    # Core's Cargo artifact locates the dependency directory as well as the
    # facade's top-level rlib directory. No guessed hash or stale glob is used.
    check_arguments = []
    rustc_arguments = []
    for name, library in sorted(libraries.items()):
        check_arguments += ["--extern", f"{name}={library}"]
        rustc_arguments += ["--extern", f"{name}={library}"]
    for parent in sorted({str(path.parent) for path in libraries.values()}):
        rustc_arguments += ["-L", f"dependency={parent}"]
    sources = work / "examples"
    subprocess.run([
        "python3", "-B", "scripts/check-docs.py", "docs/MIGRATION_200.md",
        "--check-rust", "--extract", str(sources), *check_arguments,
    ], check=True)
    manifest = json.loads((sources / "manifest.json").read_text())
    if not manifest["examples"]:
        raise SystemExit("No marked examples were discovered")
    for example in manifest["examples"]:
        source = sources / example["file"]
        binary = work / source.stem
        subprocess.run([
            "rustc", "--edition=2024", "--crate-type=bin", str(source),
            *rustc_arguments, "-o", str(binary),
        ], check=True, timeout=60)
        subprocess.run([str(binary)], cwd=work, check=True, timeout=30)
    print(f"Compiled and ran {len(manifest['examples'])} independent Rust examples")
PY
```

For the developer CLI, replace ad hoc stdout parsing with the implemented
`--result-json` envelope. These **dry-run** commands were checked against actual
CLI help/behavior; they print plans and do not test or launch a game:

```sh
python3 -B scripts/ferravex.py test -p ferravex-core --lib session --dry-run --result-json
python3 -B scripts/ferravex.py run owned-orbit --dry-run --result-json -- --autoplay --frames 120
```

The result has `schema_version: 1`, `status`, `exit_code`, `child_returncode`,
`plan`, `data` and `error`. A `dry_run` status is not successful execution;
without dry-run, child logs stream to stderr and stdout holds one outcome
object. Always narrow `test`/`check` with `-p`: their default is the workspace
with all features. For a focused session regression run, the direct command is:

```sh
cargo test -p ferravex-core --test journey200_001 --offline --locked -j 2
```

The cookbook's validation covers its examples, local links and command syntax,
not every demo feature. Parent owns shared validator/site registration, combined
workspace/Wasm checks and fresh visual verification under the active
[Journey200 contract](JOURNEY_200.md).
