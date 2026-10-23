//! Headless salvage recipe: mine two ore, craft a beacon, wait for rescue.
//! Run: cargo run -p ferravex-play --example journey200_gameplay --offline --locked -j 2
//! Test: cargo test -p ferravex-play --example journey200_gameplay --offline --locked -j 2
//!
//! The host supplies actual fixed ticks, scripted input and an always-reachable
//! deposit/workbench. There is no renderer, wall clock, collision or network I/O.
//! See docs/JOURNEY_200_GAMEPLAY.md for the acceptance and inspection boundaries.

use ferravex_math::Vec3;
use ferravex_play::{
    ability::{AbilityBank, AbilityBlocker, AbilityError, AbilitySpec, ResourcePool},
    audio::{Cue, Surface},
    feedback::{FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackPolicy, ImpactSample},
    gameplay::{
        LoopError,
        crafting::{MissingIngredient, craft, preview},
        inventory::{Inventory, Stack},
    },
    input::{ActionHandle, ActionKind, ActionMap, Binding, ContextId, InputCode},
    objective::{
        ObjectiveGraph, ObjectiveKind, ObjectiveProgress, ObjectiveSnapshot, ObjectiveSpec,
        ObjectiveState,
    },
};

// A deliberately small, binary-exact fixed step makes the timeline easy to audit.
const DT: f32 = 0.125;
const ORE: u32 = 1;
const BEACON: u32 = 2;
const MINE_EVENT: u16 = 10;
const CRAFT_EVENT: u16 = 20;
const MINE_KEY: InputCode = InputCode::Key(77);
const CRAFT_KEY: InputCode = InputCode::Key(67);
const COST: [(u32, u32); 1] = [(ORE, 2)];
const OUTPUT: [Stack; 1] = [Stack {
    item: BEACON,
    amount: 1,
    limit: 1,
}];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rejection {
    Depleted,
    Ability(AbilityError),
    Inventory(LoopError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AcceptedEvent {
    serial: u64,
    kind: u16,
}

// These are recipe-local adapters, not a new engine subsystem or public API.
struct Salvage {
    tick: u64,
    deposit: u32,
    bank: AbilityBank<1>,
    energy: ResourcePool,
    bag: Inventory<1>,
    objectives: ObjectiveGraph<4>,
}

impl Salvage {
    fn new() -> Self {
        let node = |id, all, kind| ObjectiveSpec {
            id,
            all,
            any: 0,
            exclusive_group: 0,
            kind,
        };
        Self {
            tick: 0,
            deposit: 2,
            bank: AbilityBank::new(
                1, // Stable, nonzero actor identity for this run.
                &[AbilitySpec {
                    charges: 1,
                    recharge_seconds: 2.0 * DT,
                    cost: 2.0,
                }],
            )
            .unwrap(),
            energy: ResourcePool::new(4.0, 4.0, 0.0).unwrap(),
            bag: Inventory::new().unwrap(),
            objectives: ObjectiveGraph::new(
                1, // Host-authored rules identifier, not a new wire version.
                [
                    node(
                        100,
                        0,
                        ObjectiveKind::Event {
                            id: MINE_EVENT,
                            count: 2,
                        },
                    ),
                    node(
                        200,
                        1 << 0,
                        ObjectiveKind::Event {
                            id: CRAFT_EVENT,
                            count: 1,
                        },
                    ),
                    node(300, 1 << 1, ObjectiveKind::Timer { seconds: 2.0 * DT }),
                    node(400, 1 << 2, ObjectiveKind::Gate),
                ],
            )
            .unwrap(),
        }
    }

    fn begin_tick(&mut self) {
        self.tick = self.tick.checked_add(1).expect("bounded recipe tick count");
        self.bank.advance(DT).unwrap();
        self.energy.advance(DT).unwrap();
        // Advance before accepting this tick's commands: newly unlocked timers
        // start at this tick's end boundary and receive no retroactive time.
        self.objectives.advance(DT).unwrap();
    }

    fn accepted(&mut self, kind: u16) -> AcceptedEvent {
        let serial = self
            .objectives
            .last_event_serial()
            .checked_add(1)
            .expect("at most three accepted events in this recipe");
        self.objectives.event(kind, 1, serial).unwrap();
        AcceptedEvent { serial, kind }
    }

    fn mine(&mut self) -> Result<AcceptedEvent, Rejection> {
        if self.deposit == 0 {
            return Err(Rejection::Depleted);
        }
        let reservation = self
            .bank
            .begin(0, &mut self.energy, true)
            .map_err(Rejection::Ability)?;
        if let Err(error) = self.bag.add(Stack {
            item: ORE,
            amount: 1,
            limit: 4,
        }) {
            // Refund to the same pool before returning a rejected action.
            self.bank.cancel(reservation, &mut self.energy).unwrap();
            return Err(Rejection::Inventory(error));
        }
        // No intervening bank mutation: this freshly issued token is valid.
        self.bank.commit(reservation).unwrap();
        self.deposit -= 1;
        Ok(self.accepted(MINE_EVENT))
    }

    fn craft_beacon(&mut self) -> Result<AcceptedEvent, Rejection> {
        // Trusted workbench recipe; craft rechecks current costs AND output room.
        craft(&mut self.bag, &COST, &OUTPUT).map_err(Rejection::Inventory)?;
        Ok(self.accepted(CRAFT_EVENT))
    }
}

struct Host {
    game: Salvage,
    input: ActionMap,
    gameplay: ContextId,
    menu: ContextId,
    mine: ActionHandle,
    craft: ActionHandle,
    feedback: FeedbackController<8>,
    accepted: u32,
    rejected: u32,
    cues: u32,
    last_attempt: Option<Result<AcceptedEvent, Rejection>>,
    last_impact: ImpactSample,
}

impl Host {
    fn new(policy: FeedbackPolicy) -> Self {
        let mut input = ActionMap::default();
        let gameplay = input.create_context("gameplay").unwrap();
        let menu = input.create_context("menu").unwrap();
        let mut bind = |name, key| {
            input
                .named_action_handle(
                    name,
                    ActionKind::Button,
                    gameplay,
                    vec![Binding::digital(key)],
                )
                .unwrap()
        };
        let mine = bind("mine", MINE_KEY);
        let craft = bind("craft", CRAFT_KEY);
        input.set_context(Some(gameplay)).unwrap();
        let config = FeedbackConfig {
            networked: true,
            single_player_hitstop: true,
            ..Default::default()
        };
        let mut feedback = FeedbackController::new(config, 7).unwrap();
        feedback.set_policy(policy);
        Self {
            game: Salvage::new(),
            input,
            gameplay,
            menu,
            mine,
            craft,
            feedback,
            accepted: 0,
            rejected: 0,
            cues: 0,
            last_attempt: None,
            last_impact: ImpactSample::default(),
        }
    }

    fn tap(&mut self, key: InputCode) {
        self.input.feed(key, 1.0);
        self.input.feed(key, 0.0);
    }

    fn menu_open(&mut self, open: bool) {
        self.input
            .set_context(Some(if open { self.menu } else { self.gameplay }))
            .unwrap();
    }

    fn present_event(&mut self, event: AcceptedEvent) {
        let (effect, cue, gain) = match event.kind {
            // The accepted mining pulse breaks one unit from the deposit.
            MINE_EVENT => (
                FeedbackEvent::explosion(Vec3::ZERO, 1.0).unwrap(),
                Cue::Impact(Surface::Stone),
                0.8,
            ),
            CRAFT_EVENT => (
                FeedbackEvent::pickup(Vec3::ZERO, 1.0).unwrap(),
                Cue::Pickup,
                0.5,
            ),
            _ => unreachable!("only authored accepted events are routed"),
        };
        if let Some(response) = self.feedback.emit(event.serial, effect).unwrap() {
            // A real host schedules sound/particles only for Some(response).
            assert_eq!(response.cue, cue);
            assert_eq!(response.gain, gain);
            if self.feedback.policy().reduced_motion() {
                assert_eq!(response.particles, 0);
            }
            self.cues += 1;
        }
    }

    fn resolve(&mut self, result: Result<AcceptedEvent, Rejection>) {
        self.last_attempt = Some(result);
        match result {
            Ok(event) => {
                self.accepted += 1;
                self.present_event(event);
            }
            Err(_) => self.rejected += 1,
        }
    }

    fn frame(&mut self, fixed_steps: u8, presentation_dt: f32) {
        // The scripted driver supplies an already-budgeted batch, not elapsed
        // wall time. An interactive host would obtain this from its fixed clock.
        assert!(fixed_steps <= 4);
        for _ in 0..fixed_steps {
            self.game.begin_tick();
            if self.input.consume_tick_handle(self.mine).unwrap().pressed {
                let result = self.game.mine();
                self.resolve(result);
            }
            if self.input.consume_tick_handle(self.craft).unwrap().pressed {
                let result = self.game.craft_beacon();
                self.resolve(result);
            }
        }
        // Presentation runs even with zero ticks or a local menu. It never
        // changes the tick budget. The networked config suppresses hit-stop.
        let before = self.feedback.impact();
        let (frame, impact) = self.feedback.advance_with_impact(presentation_dt).unwrap();
        assert_eq!(impact, before);
        assert_eq!(impact.flash, frame.flash);
        assert_eq!(frame.hitstop_seconds, 0.0);
        self.last_impact = impact;
    }
}

#[derive(Debug, PartialEq)]
struct Outcome {
    tick: u64,
    deposit: u32,
    items: [u32; 2],
    energy: f32,
    charges: u8,
    objectives: ObjectiveSnapshot<4>,
}

fn outcome(host: &Host) -> Outcome {
    Outcome {
        tick: host.game.tick,
        deposit: host.game.deposit,
        items: [host.game.bag.count(ORE), host.game.bag.count(BEACON)],
        energy: host.game.energy.value(),
        charges: host.game.bank.charges(0).unwrap(),
        objectives: host.game.objectives.snapshot(),
    }
}

fn scenario(policy: FeedbackPolicy) -> Host {
    let mut host = Host::new(policy);
    let initial = outcome(&host);
    let availability = host
        .game
        .bank
        .availability(0, &host.game.energy, true)
        .unwrap();
    assert_eq!(availability.check(), Ok(()));
    let missing = preview(&host.game.bag, &COST, &OUTPUT);
    assert_eq!(missing.result(), Err(LoopError::Unavailable));
    assert_eq!(
        missing.missing_ingredients(),
        &[MissingIngredient {
            item: ORE,
            required: 2,
            available: 0,
            missing: 2,
        }]
    );
    let locked = host.game.objectives.inspect(1).unwrap();
    assert_eq!(locked.spec.id, 200);
    assert_eq!(locked.state, ObjectiveState::Locked);
    assert_eq!(locked.blockers.missing_all, 1 << 0); // Slots, not authored IDs.
    assert_eq!(outcome(&host), initial); // All inspection above is read-only.

    host.tap(CRAFT_KEY);
    host.frame(1, DT); // Tick 1: insufficient ore, no event or cue.
    assert_eq!(
        host.last_attempt,
        Some(Err(Rejection::Inventory(LoopError::Unavailable)))
    );
    assert_eq!(host.game.objectives.last_event_serial(), 0);
    assert_eq!(host.cues, 0);

    host.tap(MINE_KEY);
    host.frame(0, DT / 2.0); // A complete tap waits through a zero-tick frame.
    assert!(host.input.peek_handle(host.mine).unwrap().pressed);
    assert_eq!(host.game.tick, 1);
    host.frame(1, DT); // Tick 2: first ore accepted.
    assert_eq!(host.game.bag.count(ORE), 1);
    assert_eq!(
        host.game.objectives.inspect(0).unwrap().progress,
        ObjectiveProgress::Event {
            count: 1,
            required: 2
        }
    );
    assert_eq!(
        host.game
            .bank
            .availability(0, &host.game.energy, true)
            .unwrap()
            .blocker,
        Some(AbilityBlocker::NoCharges)
    );

    host.tap(MINE_KEY);
    host.frame(1, DT); // Tick 3: charge still reloading; no second reward.
    assert_eq!(
        host.last_attempt,
        Some(Err(Rejection::Ability(AbilityError::Empty)))
    );
    assert_eq!(host.game.energy.value(), 2.0);
    assert_eq!(host.game.bag.count(ORE), 1);
    assert_eq!(host.cues, 1);

    host.tap(MINE_KEY);
    host.frame(1, DT); // Tick 4: second ore, craft objective unlocked.
    assert_eq!(host.game.bag.count(ORE), 2);
    assert_eq!(host.game.objectives.state(1), Some(ObjectiveState::Ready));
    assert!(preview(&host.game.bag, &COST, &OUTPUT).is_available());

    host.tap(CRAFT_KEY);
    host.frame(1, DT); // Tick 5: consume the ore stack, reuse its only slot.
    assert_eq!(
        [host.game.bag.count(ORE), host.game.bag.count(BEACON)],
        [0, 1]
    );
    assert_eq!(
        host.game.objectives.inspect(2).unwrap().progress,
        ObjectiveProgress::Timer {
            elapsed: 0.0,
            seconds: 2.0 * DT
        }
    );

    host.menu_open(true);
    host.tap(MINE_KEY); // Suppressed local command; authority keeps advancing.
    host.frame(2, 2.0 * DT); // Ticks 6 and 7 complete the broadcast and rescue.
    assert!(
        host.game
            .objectives
            .iter()
            .all(|view| view.state == ObjectiveState::Complete)
    );
    assert_eq!(host.game.tick, 7);
    assert_eq!(host.game.objectives.last_event_serial(), 3);
    assert_eq!((host.accepted, host.rejected, host.cues), (3, 2, 3));
    assert_eq!(host.game.energy.value(), 0.0);
    assert_eq!(host.feedback.dropped, 0);
    host
}

fn main() {
    let normal = scenario(FeedbackPolicy::default());
    let reduced = scenario(FeedbackPolicy::new(true, true, 1.0).unwrap());
    assert_eq!(outcome(&normal), outcome(&reduced));
    assert_eq!(normal.cues, reduced.cues);
    println!("7 fixed ticks: 2 ore mined, 1 beacon crafted, 4 objectives complete");
    println!("3 accepted events, 2 rejected attempts, 3 sound cues, 0 feedback drops");
    println!("Reduced motion/flash preserve gameplay; local menu preserves rescue timer");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::objective::ObjectiveFrame;

    #[test]
    fn complete_scenario_repeats_and_preferences_preserve_rules() {
        let expected = outcome(&scenario(FeedbackPolicy::default()));
        assert_eq!(outcome(&scenario(FeedbackPolicy::default())), expected);
        for motion in [false, true] {
            for flash in [false, true] {
                assert_eq!(
                    outcome(&scenario(FeedbackPolicy::new(motion, flash, 1.0).unwrap())),
                    expected
                );
            }
        }
    }

    #[test]
    fn queued_tap_survives_zero_ticks_and_is_not_repeated_during_catchup() {
        let mut host = Host::new(FeedbackPolicy::default());
        host.tap(MINE_KEY);
        host.frame(0, 0.0);
        assert_eq!(host.game.bag.count(ORE), 0);
        host.frame(4, 0.25); // Enough time to recharge: repeating the edge would mine twice.
        assert_eq!(host.game.tick, 4);
        assert_eq!(host.game.bag.count(ORE), 1);
        assert_eq!((host.accepted, host.rejected, host.cues), (1, 0, 1));
    }

    #[test]
    fn full_inventory_refunds_the_ability_and_emits_nothing() {
        let mut host = Host::new(FeedbackPolicy::default());
        host.game.bag.add(OUTPUT[0]).unwrap();
        let ready = host
            .game
            .bank
            .availability(0, &host.game.energy, true)
            .unwrap();
        let saved = host.game.objectives.snapshot();
        host.tap(MINE_KEY);
        host.frame(1, DT);
        assert_eq!(
            host.last_attempt,
            Some(Err(Rejection::Inventory(LoopError::Capacity)))
        );
        assert_eq!(
            host.game
                .bank
                .availability(0, &host.game.energy, true)
                .unwrap(),
            ready
        );
        assert_eq!(host.game.deposit, 2);
        assert!(host.game.bag.stacks().eq(OUTPUT));
        assert_eq!(host.game.objectives.snapshot(), saved);
        assert_eq!((host.accepted, host.cues), (0, 0));
        assert_eq!(host.last_impact, ImpactSample::default());
    }

    #[test]
    fn crafting_output_failure_and_stale_preview_preserve_items_and_events() {
        let mut host = Host::new(FeedbackPolicy::default());
        let ore = Stack {
            item: ORE,
            amount: 3,
            limit: 4,
        };
        host.game.bag.add(ore).unwrap();
        let saved = host.game.objectives.snapshot();
        let full = preview(&host.game.bag, &COST, &OUTPUT);
        assert!(full.missing_ingredients().is_empty());
        assert_eq!(full.result(), Err(LoopError::Capacity));
        host.tap(CRAFT_KEY);
        host.frame(1, DT);
        assert_eq!(
            host.last_attempt,
            Some(Err(Rejection::Inventory(LoopError::Capacity)))
        );
        assert!(host.game.bag.stacks().eq([ore]));

        host.game.bag.remove(ORE, 1).unwrap();
        let ready = preview(&host.game.bag, &COST, &OUTPUT);
        assert!(ready.is_available());
        host.game.bag.remove(ORE, 1).unwrap(); // Another accepted host expense.
        host.tap(CRAFT_KEY);
        host.frame(1, DT);
        assert_eq!(
            host.last_attempt,
            Some(Err(Rejection::Inventory(LoopError::Unavailable)))
        );
        assert_eq!(host.game.bag.count(ORE), 1);
        assert_eq!(host.game.bag.count(BEACON), 0);
        assert_eq!(host.game.objectives.snapshot(), saved);
        assert_eq!((host.accepted, host.cues), (0, 0));
    }

    #[test]
    fn duplicate_projection_delivery_does_not_repeat_objectives_or_cues() {
        let mut host = Host::new(FeedbackPolicy::default());
        host.tap(MINE_KEY);
        host.frame(1, DT);
        let accepted = host.last_attempt.unwrap().unwrap();
        let before = outcome(&host);
        assert_eq!(
            host.game
                .objectives
                .event(accepted.kind, 1, accepted.serial)
                .unwrap(),
            ObjectiveFrame::default()
        );
        host.present_event(accepted);
        assert_eq!(outcome(&host), before);
        assert_eq!(host.cues, 1);
        // Do not replay mine(): inventory/ability commands need host-level dedup.
    }

    #[test]
    fn local_menu_masks_commands_without_stopping_active_objectives() {
        let mut host = scenario(FeedbackPolicy::default());
        host.menu_open(false);
        host.frame(1, DT);
        assert_eq!(host.game.tick, 8);
        assert_eq!((host.accepted, host.rejected), (3, 2)); // No stale menu tap.
    }

    #[test]
    fn motion_and_flash_inspection_are_independent_and_do_not_tick_rules() {
        for motion in [false, true] {
            for flash in [false, true] {
                let mut host = Host::new(FeedbackPolicy::new(motion, flash, 1.0).unwrap());
                host.tap(MINE_KEY);
                host.frame(1, 0.0);
                assert_eq!(host.last_impact.emphasis, if motion { 0.0 } else { 1.0 });
                assert_eq!(host.last_impact.flash, if flash { 0.0 } else { 0.5 });
                let before = outcome(&host);
                host.frame(0, DT);
                assert_eq!(outcome(&host), before);
                assert_eq!(host.cues, 1);
            }
        }
    }
}
