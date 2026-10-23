# Compose a headless salvage scenario

The runnable [Journey200 gameplay recipe](../crates/ferravex-play/examples/journey200_gameplay.rs)
mines two ore, crafts a beacon, and waits for a rescue timer. It composes the
registered `ferravex_play` ability, inventory, crafting, objective, input and
feedback APIs. Assertions cover both success and rejection. The existing
[wave/crafting recipe](GAMEPLAY_RECIPES.md) remains a separate example of waves,
progression and upgrades.

Run from the repository root:

```sh
cargo run -p ferravex-play --example journey200_gameplay --offline --locked -j 2
cargo test -p ferravex-play --example journey200_gameplay --offline --locked -j 2
cargo clippy -p ferravex-play --example journey200_gameplay --offline --locked -j 2 -- -D warnings
python3 -B scripts/check-docs.py docs/JOURNEY_200_GAMEPLAY.md --json
```

The executable prints these lines after its assertions pass:

```text
7 fixed ticks: 2 ore mined, 1 beacon crafted, 4 objectives complete
3 accepted events, 2 rejected attempts, 3 sound cues, 0 feedback drops
Reduced motion/flash preserve gameplay; local menu preserves rescue timer
```

The sound count means returned cue requests; this executable does not play audio.
It requires no display, renderer, browser, assets or external packages. Its only
crate dependencies are the existing `ferravex-play` and `ferravex-math` packages.
Cargo discovers the example automatically; no manifest or module registration
is necessary.

## Read the recipe in three parts

`Salvage` owns the authoritative rules. `begin_tick` advances the ability bank,
resource pool and objective timers by the constant `DT = 0.125` seconds. The
8 Hz step is intentionally binary-exact for this small, inspectable scenario;
it is not a recommended game update rate. The host supplies tick counts directly,
without sleeping or deriving ticks from presentation time. An interactive game
should reuse the existing fixed clock/session described in the
[agent guide](AGENT_GUIDE.md#fixed-ticks-input-and-feedback).

`Host` adapts scripted button taps and presentation. It consumes each action's
edges inside the actual tick loop, routes only accepted events to feedback, then
advances presentation once per frame. A frame may contain zero to four ticks.
The recipe uses two registered input actions, one ability slot, one inventory
slot, four objective nodes and eight feedback history/burst slots. No event log
grows during the run. Action-map setup uses its existing bounded collections;
this recipe does not claim an allocation benchmark.

`scenario` drives the complete run and asserts the intermediate states. `main`
runs it with normal visuals and with both reduced-motion/reduced-flash choices,
then compares authoritative outcomes and cue counts. Inline tests exercise the
failure paths and every independent motion/flash flag combination.

## Follow the accepted-event timeline

The deposit contains two ore. Mining has one charge, a two-tick recharge, and an
energy cost of two. The player starts with four energy and no regeneration.
The always-reachable workbench exchanges two ore for one beacon. It consumes
the entire ore stack before placing the output in the inventory's only slot.
The example's host assumes range, visibility and permissions; it does not
implement collision, movement or remote command authentication.

| Tick | Command and rule result | Objective and presentation result |
| --- | --- | --- |
| 1 | Craft fails with `LoopError::Unavailable`. | No accepted serial, objective progress or cue. |
| Between 1 and 2 | A mining press and release arrive in a frame with zero ticks. | The pending edge remains available; presentation can still advance. |
| 2 | Mining spends one charge and two energy; adds one ore. | Accepted serial 1 advances collection to 1/2 and requests one mining cue. |
| 3 | Mining fails with `AbilityError::Empty`; recharge is unfinished. | Ore, energy, accepted serial and cue count stay unchanged by the rejection. |
| 4 | Recharge finishes; mining spends the second two energy and adds ore. | Serial 2 completes collection and unlocks crafting. |
| 5 | Crafting consumes both ore and places one beacon. | Serial 3 completes crafting and unlocks the rescue timer at elapsed zero. |
| 6–7 | A local menu masks gameplay input while two fixed ticks execute. | The timer reaches 0.25 seconds; the final gate completes automatically. |

Timers advance before commands in each tick. A timer unlocked by the tick-5
craft receives its first time on tick 6. Feedback hit-stop never alters this
schedule. The feedback configuration sets `networked = true`, so even with
`single_player_hitstop = true`, sampled hit-stop remains zero. This demonstrates
the presentation configuration and a host that continues authoritative ticks;
it does not establish networking or replay transport support.

## Inspect, then execute against current state

The recipe calls the actual public methods below; the recipe's adapters are
private example code.

| Need | Existing API | Acceptance boundary |
| --- | --- | --- |
| Explain ability readiness | [`AbilityBank::availability`](../crates/ferravex-play/src/ability.rs) and `AbilityAvailability::check` | Immutable, advisory snapshot. `begin` validates again and reserves a charge/resource cost. |
| Commit or refund mining | `AbilityBank::begin`, `commit`, `cancel`; [`Inventory::add`](../crates/ferravex-play/src/gameplay/inventory.rs) | An inventory error cancels the fresh reservation back into the same resource pool. Deposit/objectives/feedback change only after success. |
| Explain and execute crafting | [`crafting::preview`, `CraftPreview::{result,is_available,missing_ingredients}`, `crafting::craft`](../crates/ferravex-play/src/gameplay/crafting.rs) | Preview reserves nothing. Craft validates the current inventory and commits only when all costs and outputs fit. |
| Show quest progress | [`ObjectiveGraph::{inspect,iter}`](../crates/ferravex-play/src/objective.rs) | Read-only views include progress, state and immediate blockers. They do not advance timers or consume serials. |
| Apply quest events | `ObjectiveGraph::event(id, amount, serial)` and `advance(dt)` | Feed ordered accepted events, and advance by simulation time. The graph does not validate world rules or inventory. |
| Consume a button once | [`ActionMap::consume_tick_handle`](../crates/ferravex-play/src/input/mod.rs) | Call on an actual tick. `peek_handle` may inspect a queued tap without consuming it. |
| Inspect effects | [`FeedbackController::{emit,impact,advance_with_impact}`](../crates/ferravex-play/src/feedback.rs) | Only a newly accepted `Some(response)` requests cues/particles. Sampling effects never updates the game. |

Objective IDs 100, 200, 300 and 400 identify authored nodes. Their prerequisite
bits refer to declaration slots 0, 1, 2 and 3, **not** those IDs. Event kinds 10
and 20 identify mining/crafting; serials 1, 2 and 3 identify accepted occurrences.
The recipe uses the graph's last serial to allocate the next value in this one
ordered stream. A real authoritative host must coordinate all event producers
and deduplicate commands before mutating inventory or ability resources.

The inspection assertions compare snapshots before and after reads. The failure
tests also show two traps: an empty `missing_ingredients()` slice does not mean
the output fits, and a successful preview can become stale after another expense.
Neither case consumes ingredients, advances objectives or emits feedback when
the subsequent craft fails. On a failed mining inventory write, charge and energy
are refunded; the bank's internal reservation serial is still consumed.

## Keep feedback separate from game rules

The recipe sends `FeedbackEvent::explosion` only after its mining pulse actually
removes one ore from the deposit and stores it in the bag. Craft success sends
`FeedbackEvent::pickup`. Rejected attempts never enter `present_event`.
`FeedbackController::emit` suppresses repeated IDs within its bounded history;
only `Some(response)` increments the cue counter. `ObjectiveGraph::event` ignores
duplicate/older serials. These consumers do not make repeated inventory/ability
commands safe: the duplicate-delivery test replays the accepted event to the
consumers, **not** the command that originally created it.

`impact()` reads current envelopes without advancing them or freeing burst
capacity. `advance_with_impact(dt)` replaces a call to `advance(dt)` and returns
paired frame/impact samples. Both scalar samples are taken before decay;
`impact.flash` and `frame.flash` represent the same channel. Render that channel
once. Presentation deltas must be finite and in `0..=0.25` seconds.

This smaller, complete program demonstrates accepted gating, duplicate suppression
and independent visual inspection. It can be extracted and compiled by the docs
checker because it is marked `rust,check`; its `main` must also be executed to
check the assertions.

```rust,check crate-type=bin
use ferravex_math::Vec3;
use ferravex_play::feedback::{
    FeedbackController, FeedbackError, FeedbackEvent, FeedbackPolicy,
};

fn main() -> Result<(), FeedbackError> {
    let mut feedback = FeedbackController::<8>::new(Default::default(), 7)?;
    let effect = FeedbackEvent::explosion(Vec3::ZERO, 1.0)?;
    // The host supplies acceptance after its game rules commit.
    assert!(feedback.emit_if_accepted(1, false, effect)?.is_none());
    let response = feedback.emit_if_accepted(1, true, effect)?.unwrap();
    assert_eq!(response.gain, 0.8);
    assert!(feedback.emit(1, effect)?.is_none());

    // Suppress motion while retaining the independent flash channel.
    feedback.set_policy(FeedbackPolicy::new(true, false, 1.0)?);
    let before = feedback.impact();
    assert_eq!(before.emphasis, 0.0);
    assert_eq!(before.flash, 0.5);
    let (frame, impact) = feedback.advance_with_impact(0.125)?;
    assert_eq!(impact, before);
    assert_eq!(impact.flash, frame.flash);
    assert_eq!(frame.shake.translation, Vec3::ZERO);
    assert!(feedback.impact().flash < before.flash);

    feedback.set_policy(FeedbackPolicy::new(true, true, 1.0)?);
    assert_eq!(feedback.impact().flash, 0.0);
    Ok(())
}
```

A live host must also clear or suppress particles/reactions it already owns when
preferences change; the controller cannot recall them. This snippet checks
returned values and contains no particle or audio renderer. The main recipe
asserts cue/gain values under all tested policies and samples presentation even
when the frame contains no simulation tick or gameplay is masked by a menu.

Feedback storage is bounded. The main scenario has at most three accepted events
in total and uses `FeedbackController<8>`; it asserts `dropped == 0`. A production
host must handle `FeedbackError::Capacity` as presentation loss after gameplay
has already committed, and inspect `dropped` for burst/shake saturation. It must
not retry or undo a game command because feedback ran out of room. The example's
`unwrap` calls assert its fixed, valid fixture configuration and stated bounds;
they are not a general input/error-handling policy.

## Validation scope and integration

The seven inline tests cover the complete repeatable scenario, all four visual
policy combinations, queued taps through zero/catch-up ticks, ability refunds
on full inventory, crafting output rollback and stale previews, duplicate
accepted-event delivery, local-menu suppression/resume, and independent impact
inspection. They compile against the registered crate modules. No replacement
gameplay implementations or test-only API stubs are used.

The command above checks this guide's local links. To type-check its one marked
program manually, supply `--check-rust` and fresh,
matching `--extern ferravex_play=...` and `--extern ferravex_math=...` artifacts.
The [checker](../scripts/check-docs.py) accepts repeated `--extern NAME=PATH`
arguments and adds their dependency search directories. `--extract` writes to a
new directory; `--check-rust` emits metadata only and does **not** run assertions.
The task receipt records the actual extracted-program compilation and execution.

This guide is registered in the shared documentation/site scopes. The full
validator's `rust.guide-examples` step separately compiles and executes its
marked program with [the trusted-example runner](../scripts/check-guide-examples.py).
This guide does not establish whole-workspace compatibility, Wasm behavior,
browser/game playability, performance, physical audio/haptics or cross-platform
bitwise determinism. Those validations remain separate from this native headless
recipe. See [dependency policy](../DEPENDENCIES.md) and
[Journey200 coordination](JOURNEY_200.md#integration-contract).
