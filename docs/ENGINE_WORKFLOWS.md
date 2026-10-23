# Build with the current engine

Start with a small loop, then add the systems that give your game its identity.
Ferravex shares its Rust simulation, input and presentation helpers across fifteen
playable examples. The facade is `ferravex`; use its qualified modules when you
need a specific system. The [quickstart](QUICKSTART.md) and
[project generator](../website/content/getting-started.md) get you to running code.

The current release includes 83 Journey400 implementations. These APIs are
available in source; adoption varies by demo. The other 117 planned tasks remain
[deferred](STOPPING_POINT.md). A checked API reports invalid input or exhausted
capacity; it does not remove the caller's responsibility to choose a policy.

## Run rules on fixed ticks

Use `GameSession` for Ready, Playing, Paused and terminal states. A render frame
can issue zero, one or several updates. Consume gameplay edges only when an
update actually executes. Keep rendering separate from that loop.

```rust,check,crate-type=bin
use ferravex_core::session::GameSession;
use ferravex_engine::interpolation::InterpolationPair;
use ferravex_math::Vec2;

fn main() {
    let mut session = GameSession::new(60.0, 8, true);
    let mut position = Vec2::ZERO;
    let mut history = InterpolationPair::new(position);

    let batch = session.advance_batch(2.5 / 60.0);
    for tick in batch.ticks() {
        if !session.is_playing() { break; }
        position.x += 120.0 * tick.dt;
        history.commit(position);
    }
    let visible = history.sample_with(batch.frame().alpha,
        |a, b, t| a.lerp(*b, t)).unwrap();
    assert_eq!(position.x, 4.0);
    assert!((visible.x - 3.0).abs() < 0.001);

    history.snap(Vec2::new(100.0, 0.0));
    assert_eq!(*history.current(), Vec2::new(100.0, 0.0));
}
```

Tick IDs describe scheduled work. Breaking out of a batch does not undo issued
ticks or prove they executed. Record accepted updates in the runner. On an
accepted teleport or restart, snap presentation history; on a presentation pause,
use its hold/resume policy. The [session guide](MIGRATION_200.md) explains local
menus, focus and authoritative clocks.

## Compose entities and resources

ECS queries support optional references and presence filters (`With`, `Without`,
`All`, `Any`, `Not`). Bundles and fetched tuples support eight entries. Prepare
entity/component capacity before bounded batch spawning; use an
`EcsCommandBuffer` when structural edits must wait until query guards are dropped.
Selected-component duplication returns an explicit old-to-new entity map.

For singleton state, use typed `Resources`. Disjoint callbacks validate duplicate
and missing types before executing. Separate roles with newtypes rather than
using two aliases of the same Rust type.

```rust,check,crate-type=bin
use ferravex_engine::resources::Resources;

struct Score(u32);
struct Reward(u32);

fn main() {
    let mut resources = Resources::default();
    resources.insert(Score(10));
    resources.insert(Reward(5));
    resources.with2_mut_ref::<Score, Reward, _>(|score, reward| {
        score.0 += reward.0;
    }).unwrap();
    assert_eq!(resources.require::<Score>().unwrap().0, 15);
}
```

`SystemSchedule` prepares sequential ordering, predicates and flush boundaries.
Fallible command buffers expose accepted/rejected work and stop/continue policies.
Neither callback errors nor panics imply automatic rollback. Scope delayed tick
jobs to the scene or round that owns them, and cancel that scope before reset.
See the [agent API map](AGENT_GUIDE.md#task-and-api-map) for exact entry points.

## Author physics changes explicitly

Use the rotating-body `ferravex_physics::d2` or `d3` world for rigid bodies. It is
separate from the translational `ferravex_dynamics::PhysicsWorld`.

- `try_teleport_body` makes motion-reset policy explicit and preserves handles.
- `replace_collider_shape` validates mass before replacing geometry.
- `WorldJointFrame` converts authored world frames to existing joint descriptors.
- `Assembly` preflights grouped body/collider/joint construction.
- Overlap queries report separation and bounded result truncation.
- `SensorOccupancyTracker` aggregates child contacts into actor occupancy.
- `set_impact_settings` opts into bounded accepted-impact observation.
- `apply_radial_impulse` preflights a complete bounded candidate set.
- `Character::try_set_height` distinguishes blocked clearance from invalid input
  and exhausted query budgets.

Commit game effects only after an accepted result. Rebuild or snap presentation
from the returned pose; avoid changing visual geometry before a physics edit
succeeds. Explore the [rigid-body guide](../crates/ferravex-physics/README.md),
[joints](JOINTS_AND_COMPOUNDS.md) and [character controllers](CHARACTERS.md), then
try the [2D](../examples/physics-2d/README.md) and
[3D labs](../examples/physics-3d/README.md).

## Keep the HUD readable

Fit the world to available space, but lay out essential UI in logical screen
pixels. `Viewport::responsive` keeps text size independent of world scale.
`HudRegions` reserves strips; `ui::flow` wraps actions into complete rows.

```rust,check,crate-type=bin
use ferravex_play::ui::{Rect, Viewport};
use ferravex_play::ui::flow::{layout, FlowItem, FlowOptions};

fn main() {
    let viewport = Viewport::responsive([320.0, 568.0], [16.0; 4]).unwrap();
    let area = Rect { min: [0.0, 0.0], size: [viewport.bounds().size[0], 104.0] };
    let items = [FlowItem::fixed(140.0); 3];
    let mut buttons = [area; 3];
    let report = layout(area, &items, FlowOptions::default(), &mut buttons).unwrap();
    assert_eq!((report.written, report.rows, report.overflow), (3, 2, 0));
    assert_eq!(buttons[0].size[1], 44.0);
}
```

Use the returned rectangles for both drawing and hit testing. Inspect overflow
instead of silently shrinking labels. Wrapped text, stat lists, meters and
content-driven cards compose into `Ui`; `Ui::compose` can roll back an incomplete
widget group. Opt-in semantic inspection exposes current geometry without
consuming input. Themes and UI preferences require consumer adoption; a stored
text preference cannot restyle arbitrary custom drawing automatically.

Read [Design for different screens](RESPONSIVE_GAMES.md) and
[HUD and menus](UI.md) before choosing fixed, wrapped or scrollable content.

## Separate authority from transport

`fps::server_state::ServerState` and `fps::client_state::ClientState` are the
supplied-time production drivers behind the network adapters. Feed complete
datagrams, poll through local overlays, and tick only executed updates.
`local_session` and `ferravex_net::local_link` support bounded in-process sessions.

Baseline stores authorize deltas against exact retained ticks. Reliable sends
accept packet/byte budgets; borrowed encode/drain paths support caller-owned
buffers and backpressure. Reset session identities, history and pending work on
reconnect. A refused send does not roll back an authority tick.

See [FPS simulation](../crates/ferravex-fps/README.md),
[protocol and replication](../crates/ferravex-net/README.md), and the
[Strike](../examples/strike/README.md) and
[Courtyard](../examples/owned-lab/README.md) consumers. Local deterministic tests
are separate from real transport, device and Internet verification.

## Make accepted actions feel good

Gameplay helpers expose charge edges, combo receipts, progression reports,
magazine readiness, shield recharge, stagger transitions, distinct reward offers
and atomic upgrade choices. Generate sound, notices and presentation impulses
from accepted outcomes, not from a raw click or speculative preview.

`CueBatch` supports bounded accepted audio cues; `NoticeQueue` retains bounded
priority notices. Camera blends support cuts, holds and continuous retargeting;
authored rails sample caller time. Reduced motion and reduced flash are
independent choices. The [presentation recipe](JOURNEY_200_PRESENTATION.md)
connects hints, layout, preferences and diagnostics.

## Work with a coding agent

Give the agent the behavior to change, owning paths, a small reproduction and a
focused verification command. Start from [AGENTS.md](../AGENTS.md) and the
[API map](AGENT_GUIDE.md). Use `scripts/api-index.py` directly for bounded exact
or fuzzy symbol searches over fresh Rustdoc; it rejects stale source snapshots.
Its CLI-forwarding follow-up is deferred.

Prefer `cargo test -p PACKAGE --offline --locked -j 2` while iterating. Validate
marked guide examples with [check-guide-examples.py](../scripts/check-guide-examples.py),
and use [structured diagnostics](JOURNEY_200_DEBUGGING.md) when a result is
unclear. Compile, native tests, browser play and physical-device checks provide
different evidence; keep those distinctions in the handoff.
