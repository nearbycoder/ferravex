//! Task 165: unchanged authored campaigns, driven through real host commands.
//! No fixture assigns gameplay resources, entities, timers, authority or outcomes.

use super::*;
use ferravex_core::session::SessionState;
use ferravex_play::attributes::EffectStatus;
use ferravex_render::input::{self, ActionKind, Binding, InputCode, PadState};
use host::FrameEvents;

const TICK: f64 = 1. / 60.;
const MAX_TICKS: usize = 24_000;
const SECOND_WAVE_TICK: u64 = 3000;

#[derive(Debug, PartialEq)]
struct TowerState {
    kind: Kind,
    level: u8,
    cooldown: f32,
    aim: Vec2,
}

#[derive(Debug, PartialEq)]
struct PestState {
    id: u32,
    pos: Vec2,
    segment: usize,
    distance: f32,
    hp: f32,
    max_hp: f32,
    fast: bool,
    impacts: u32,
    slow: Option<EffectStatus>,
}

#[derive(Debug, PartialEq)]
struct ShotState {
    pos: Vec2,
    target: u32,
    damage: f32,
    kind: Kind,
}

/// Read-only observable state; future trajectory comparisons also exercise the
/// hidden spawn timer, path cursors, effect storage and target selectors.
#[derive(Debug, PartialEq)]
struct State {
    phase: SessionState,
    outcome: Outcome,
    time: f32,
    money: u32,
    lives: u8,
    wave: u8,
    active: bool,
    remaining: u8,
    kills: u32,
    fired: u32,
    escaped: u32,
    selected: usize,
    target: Option<u32>,
    towers: Vec<Option<TowerState>>,
    pests: Vec<PestState>,
    shots: Vec<ShotState>,
}

fn state(c: &Campaign) -> State {
    let g = &c.game;
    State {
        phase: c.session.state(),
        outcome: g.outcome,
        time: g.time,
        money: g.money,
        lives: g.lives,
        wave: g.wave,
        active: g.active,
        remaining: g.remaining,
        kills: g.kills,
        fired: g.fired,
        escaped: g.escaped,
        selected: g.selected,
        target: g.selected_target().map(|pest| pest.id),
        towers: g
            .towers
            .iter()
            .map(|tower| {
                tower.map(|t| TowerState {
                    kind: t.kind,
                    level: t.level,
                    cooldown: t.cooldown,
                    aim: t.aim,
                })
            })
            .collect(),
        pests: g
            .pests
            .iter()
            .map(|p| PestState {
                id: p.id,
                pos: p.pos,
                segment: p.segment,
                distance: p.distance,
                hp: p.hp,
                max_hp: p.max_hp,
                fast: p.fast,
                impacts: p.impacts,
                slow: p.slow(),
            })
            .collect(),
        shots: g
            .shots
            .iter()
            .map(|s| ShotState {
                pos: s.pos,
                target: s.target,
                damage: s.damage,
                kind: s.kind,
            })
            .collect(),
    }
}

fn frame(
    c: &mut Campaign,
    elapsed: f64,
    blocked: bool,
    actions: [Option<Action>; 3],
) -> FrameEvents {
    c.frame_with_input(elapsed, blocked, false, actions, || None)
}

fn command(c: &mut Campaign, action: Action) -> FrameEvents {
    frame(c, 0., false, [Some(action), None, None])
}

fn assert_silent(events: &FrameEvents) {
    assert!(!events.restarted && !events.fired && !events.killed && !events.escaped);
    assert_eq!(events.finished, None);
}

fn assert_events(events: &FrameEvents, before: &State, after: &State) {
    assert!(!events.restarted);
    assert_eq!(events.fired, after.fired > before.fired);
    assert_eq!(events.killed, after.kills > before.kills);
    assert_eq!(events.escaped, after.escaped > before.escaped);
    assert_eq!(
        events.finished,
        (before.outcome == Outcome::Playing && after.outcome != Outcome::Playing)
            .then_some(after.outcome == Outcome::Won)
    );
}

fn assert_accounting(g: &Game) {
    assert!(g.pests.len() <= MAX_PESTS);
    assert!(g.shots.len() <= MAX_SHOTS);
    let spent: u32 = g
        .towers
        .iter()
        .flatten()
        .map(|t| {
            t.kind.price()
                + (0..t.level)
                    .map(|level| 65 + 35 * u32::from(level))
                    .sum::<u32>()
        })
        .sum();
    let completed = usize::from(g.wave) - usize::from(g.active);
    let rewards: u32 = g.level.waves[..completed].iter().map(|w| w.reward).sum();
    assert_eq!(
        g.money + spent,
        g.level.money + g.kills * g.level.kill_reward + rewards
    );
    assert_eq!(u32::from(g.lives) + g.escaped, u32::from(g.level.lives));
    let launched: u32 = g.level.waves[..usize::from(g.wave)]
        .iter()
        .map(|w| u32::from(w.count))
        .sum();
    assert_eq!(
        g.kills + g.escaped + g.pests.len() as u32 + u32::from(g.remaining),
        launched
    );
}

struct Scenario {
    campaign: Campaign,
    autoplay: bool,
    second_wave_sent: bool,
}

impl Scenario {
    fn new(autoplay: bool) -> Self {
        let mut campaign = Campaign::new(Game::new(), false);
        assert_eq!(campaign.session.state(), SessionState::Ready);
        command(&mut campaign, Action::Wave);
        assert_eq!(campaign.session.state(), SessionState::Playing);
        assert_eq!(campaign.game.wave, 1);
        Self {
            campaign,
            autoplay,
            second_wave_sent: false,
        }
    }

    fn advance(&mut self, elapsed: f64) -> FrameEvents {
        let events =
            self.campaign
                .frame_with_input(elapsed, false, self.autoplay, [None; 3], || None);
        // Wave one cannot exhaust the authored twelve lives. A real manual
        // launch at 50 supplied seconds lets the undefended second wave do so.
        // All tested cadences land on this boundary, including six-tick batches.
        // Apply this immediate preparation command at the end of that frame so
        // its state is also the reference for subsequent zero-tick frames.
        if !self.autoplay
            && !self.second_wave_sent
            && self.campaign.session.tick() >= SECOND_WAVE_TICK
        {
            assert_eq!(self.campaign.session.tick(), SECOND_WAVE_TICK);
            assert!(!self.campaign.game.active);
            assert_eq!(self.campaign.game.wave, 1);
            assert_silent(&command(&mut self.campaign, Action::Wave));
            self.second_wave_sent = true;
        }
        events
    }
}

fn trace(autoplay: bool) -> Vec<State> {
    let mut scenario = Scenario::new(autoplay);
    let mut states = vec![state(&scenario.campaign)];
    for tick in 1..=MAX_TICKS {
        let events = scenario.advance(TICK);
        let after = state(&scenario.campaign);
        assert_eq!(scenario.campaign.session.tick(), tick as u64);
        assert_events(&events, states.last().unwrap(), &after);
        assert_accounting(&scenario.campaign.game);
        states.push(after);
        if scenario.campaign.game.outcome != Outcome::Playing {
            return states;
        }
    }
    panic!("the authored scenario must finish within 400 simulated seconds");
}

fn assert_terminal_frozen(c: &mut Campaign, autoplay: bool) {
    let terminal = state(c);
    let issued = c.session.tick();
    for elapsed in [0., TICK, 30.] {
        let events = c.frame_with_input(
            elapsed,
            false,
            autoplay,
            [Some(Action::Wave), Some(Action::Upgrade), None],
            || Some(Action::Build(Kind::Seed)),
        );
        assert_silent(&events);
        assert_eq!(state(c), terminal);
        assert_eq!(c.session.tick(), issued);
    }
}

fn compare_cadence(autoplay: bool, reference: &[State], hz: u32) {
    let mut scenario = Scenario::new(autoplay);
    let mut executed = 0;
    let mut zero_frames = 0;
    let mut catch_up_frames = 0;
    assert_eq!(state(&scenario.campaign), reference[0]);
    for frame_index in 0..MAX_TICKS * 3 {
        let events = scenario.advance(1. / f64::from(hz));
        let actual = state(&scenario.campaign);
        // f32 simulation time accumulates one fixed dt per executed update.
        // Match its exact reference value instead of dividing it by dt or
        // mistaking the clock's already-issued terminal batch for executed work.
        let end = reference
            .binary_search_by(|s| s.time.total_cmp(&actual.time))
            .expect("simulation time must match an executed fixed-tick prefix");
        assert!(end >= executed && end - executed <= 8);
        zero_frames += usize::from(end == executed);
        catch_up_frames += usize::from(end - executed > 1);
        assert_events(&events, &reference[executed], &reference[end]);
        assert_eq!(
            actual, reference[end],
            "{hz} Hz frame {frame_index}, tick {end}"
        );
        let issued = scenario.campaign.session.tick() as usize;
        if actual.outcome != Outcome::Playing {
            assert_eq!(end, reference.len() - 1);
            assert!(issued >= end && issued - end < 8);
            assert_terminal_frozen(&mut scenario.campaign, autoplay);
            if hz == 144 {
                assert!(zero_frames > 0);
            }
            if hz <= 30 {
                assert!(catch_up_frames > 0);
            }
            return;
        }
        assert_eq!(issued, end);
        executed = end;
    }
    panic!("{hz} Hz campaign did not finish within the bounded frame budget");
}

#[test]
fn authored_autoplay_win_matches_every_frame_at_30_60_144_hz_and_catch_up() {
    let reference = trace(true);
    let result = reference.last().unwrap();
    assert_eq!(
        (result.phase, result.outcome),
        (SessionState::Won, Outcome::Won)
    );
    assert_eq!(
        (result.wave, result.remaining, result.active),
        (4, 0, false)
    );
    assert_eq!(result.kills + result.escaped, 64);
    assert!(result.lives > 0 && result.fired > 0 && result.kills > 0);
    assert!(result.pests.is_empty() && result.shots.is_empty());
    assert!(result.towers.iter().flatten().any(|t| t.level > 0));
    assert!(
        reference
            .iter()
            .any(|s| s.pests.iter().any(|p| p.impacts > 0 && p.hp < p.max_hp))
    );
    for hz in [30, 60, 144, 10] {
        compare_cadence(true, &reference, hz);
    }
}

#[test]
fn undefended_manual_waves_lose_identically_at_30_60_144_hz_and_catch_up() {
    let reference = trace(false);
    let result = reference.last().unwrap();
    assert_eq!(
        (result.phase, result.outcome),
        (SessionState::Lost, Outcome::Lost)
    );
    assert_eq!((result.wave, result.lives, result.escaped), (2, 0, 12));
    assert_eq!((result.kills, result.fired, result.money), (0, 0, 245));
    assert!(result.towers.iter().all(Option::is_none));
    assert!(result.shots.is_empty());
    for hz in [30, 60, 144, 10] {
        compare_cadence(false, &reference, hz);
    }
}

#[test]
fn real_terminal_update_stops_the_remaining_seven_issued_ticks_and_restarts_cleanly() {
    for autoplay in [true, false] {
        let reference = trace(autoplay);
        let last = reference.len() - 1;
        let mut scenario = Scenario::new(autoplay);
        for _ in 1..last {
            scenario.advance(TICK);
        }
        let c = &mut scenario.campaign;
        assert_eq!(state(c), reference[last - 1]);
        assert!(c.game.active);
        let before = c.session.tick();
        let mut callbacks = 0;
        let events = c.frame_with_input(TICK * 8., false, autoplay, [None; 3], || {
            callbacks += 1;
            None
        });
        assert_eq!(
            c.session.tick(),
            before + 8,
            "advance issues the whole batch"
        );
        assert_eq!(
            callbacks, 1,
            "terminal updates must stop further input consumption"
        );
        assert_eq!(state(c), reference[last]);
        assert_events(&events, &reference[last - 1], &reference[last]);
        assert_terminal_frozen(c, autoplay);
        let level = c.game.level.clone();
        let restarted = frame(
            c,
            60.,
            false,
            [Some(Action::Upgrade), Some(Action::Restart), None],
        );
        assert!(restarted.restarted);
        assert!(!restarted.fired && !restarted.killed && !restarted.escaped);
        assert_eq!(restarted.finished, None);
        assert!(Arc::ptr_eq(&level, &c.game.level));
        assert_eq!(c.session.tick(), 0);
        assert_eq!(state(c), state(&Campaign::new(Game::new(), true)));
        assert!(c.notice().is_none());
        command(c, Action::Wave);
        let event = frame(c, TICK, false, [None; 3]);
        assert_silent(&event);
        assert_eq!(
            (c.session.tick(), c.game.wave, c.game.pests.len()),
            (1, 1, 1)
        );
        assert_eq!(c.game.pests[0].id, 1);
    }
}

fn planted_campaign() -> Campaign {
    let mut c = Campaign::new(Game::new(), false);
    let ready = state(&c);
    assert_silent(&frame(&mut c, 120., false, [None; 3]));
    assert_eq!(state(&c), ready);
    for action in [
        Action::Build(Kind::Seed),
        Action::Select(Some(3)),
        Action::Build(Kind::Frost),
        Action::Wave,
    ] {
        assert_silent(&command(&mut c, action));
    }
    assert_eq!(c.game.money, 60);
    assert_eq!(c.session.tick(), 0);
    c
}

#[test]
fn pause_and_overlay_during_real_frost_combat_preserve_the_uninterrupted_trajectory() {
    let mut c = planted_campaign();
    let mut uninterrupted = planted_campaign();
    for _ in 0..7200 {
        frame(&mut c, TICK, false, [None; 3]);
        frame(&mut uninterrupted, TICK, false, [None; 3]);
        if !c.game.shots.is_empty() && c.game.pests.iter().any(|p| p.slow().is_some()) {
            break;
        }
    }
    assert!(c.game.active && !c.game.shots.is_empty());
    assert!(c.game.pests.iter().any(|p| p.slow().is_some()));
    assert_eq!(state(&c), state(&uninterrupted));
    for campaign in [&mut c, &mut uninterrupted] {
        assert_silent(&frame(campaign, TICK * 0.25, false, [None; 3]));
    }
    command(&mut c, Action::Pause);
    let paused = state(&c);
    let issued = c.session.tick();
    assert_eq!(paused.phase, SessionState::Paused);
    for blocked in [false, true, false] {
        assert_silent(&frame(
            &mut c,
            120.,
            blocked,
            [Some(Action::Upgrade), Some(Action::Wave), None],
        ));
        assert_eq!(state(&c), paused);
        assert_eq!(c.session.tick(), issued);
    }
    command(&mut c, Action::Pause);
    assert_eq!(state(&c), state(&uninterrupted));
    // An automatic overlay pause resumes on close without undoing a manual
    // pause above or accumulating the overlay's elapsed time.
    assert_silent(&frame(&mut c, 120., true, [None; 3]));
    assert_eq!(c.session.state(), SessionState::Paused);
    assert_silent(&frame(&mut c, 0., false, [None; 3]));
    assert_eq!(state(&c), state(&uninterrupted));
    for campaign in [&mut c, &mut uninterrupted] {
        frame(campaign, TICK * 0.75, false, [None; 3]);
        assert_eq!(campaign.session.tick(), issued + 1);
    }
    for _ in 0..600 {
        frame(&mut c, TICK, false, [None; 3]);
        frame(&mut uninterrupted, TICK, false, [None; 3]);
        assert_eq!(state(&c), state(&uninterrupted));
        assert_accounting(&c.game);
    }
}

/// Real shared input journal, with transient bindings and no save-store writes.
struct Controller {
    input: PlayerInput,
    pad: PadState,
}

impl Controller {
    fn new() -> Self {
        let mut input = PlayerInput::transient("journey200-orchard-165");
        for (name, button) in [("upgrade", 5), ("select_one", 6)] {
            input
                .action(
                    name,
                    name,
                    ActionKind::Button,
                    vec![Binding::digital(InputCode::PadButton(button))],
                )
                .unwrap();
        }
        let pad = PadState {
            connected: true,
            ..Default::default()
        };
        assert!(input::gamepad_sample(0, pad));
        input::host_frame(0., 1200, 760, || input.sample(true));
        Self { input, pad }
    }

    fn tap(&mut self, button: usize) {
        self.pad.buttons[button] = 1.;
        assert!(input::gamepad_sample(0, self.pad));
        self.pad.buttons[button] = 0.;
        assert!(input::gamepad_sample(0, self.pad));
    }

    fn frame(
        &mut self,
        c: &mut Campaign,
        elapsed: f64,
        blocked: bool,
        action: Option<Action>,
    ) -> FrameEvents {
        input::host_frame(elapsed as f32, 1200, 760, || {
            // The real game leaves lifecycle UI active during manual pause.
            self.input.sample(true);
            c.frame_with_input(elapsed, blocked, false, [action, None, None], || {
                take_command(&mut self.input)
            })
        })
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        input::gamepad_sample(0, PadState::default());
        input::host_frame(0., 1200, 760, || self.input.sample(true));
    }
}

fn seed_campaign() -> Campaign {
    let mut c = Campaign::new(Game::new(), false);
    command(&mut c, Action::Build(Kind::Seed));
    command(&mut c, Action::Wave);
    c
}

#[test]
fn quick_controller_upgrade_coalesces_with_hud_then_rejects_an_unaffordable_second_tap() {
    let mut controller = Controller::new();
    let mut c = seed_campaign();
    let before = state(&c);
    controller.tap(5);
    for elapsed in [0., TICK * 0.25, TICK * 0.25] {
        assert_silent(&controller.frame(&mut c, elapsed, false, Some(Action::Upgrade)));
        assert_eq!(state(&c), before);
    }
    assert_silent(&controller.frame(&mut c, TICK * 5.5, false, None));
    assert_eq!(c.session.tick(), 6);
    assert_eq!(c.game.towers[0].unwrap().level, 1);
    assert_eq!(c.game.money, 75);
    assert_silent(&controller.frame(&mut c, 0.1, false, None));
    assert!(
        c.notice().unwrap().contains("upgraded"),
        "a stale edge must not retry the purchase"
    );
    controller.tap(5);
    assert_silent(&controller.frame(&mut c, 0., false, None));
    assert_silent(&controller.frame(&mut c, 0.1, false, None));
    assert_eq!((c.game.money, c.game.towers[0].unwrap().level), (75, 1));
    assert!(c.notice().unwrap().contains("Need 25 more coins"));
    assert_accounting(&c.game);
}

#[test]
fn buffered_controller_and_hud_builds_are_discarded_on_pause_overlay_and_restart() {
    for boundary in [Some(Action::Pause), None, Some(Action::Restart)] {
        let mut controller = Controller::new();
        let mut c = seed_campaign();
        command(&mut c, Action::Select(Some(1)));
        controller.tap(6);
        assert_silent(&controller.frame(&mut c, 0., false, Some(Action::Build(Kind::Seed))));
        assert!(c.game.towers[1].is_none());
        let events = controller.frame(&mut c, 120., boundary.is_none(), boundary);
        assert_eq!(events.restarted, boundary == Some(Action::Restart));
        if boundary == Some(Action::Pause) {
            controller.frame(&mut c, 0., false, Some(Action::Pause));
        }
        controller.frame(&mut c, 0.1, false, None);
        assert!(c.game.towers[1].is_none());
        assert_eq!(c.game.money, if events.restarted { 200 } else { 140 });
        if events.restarted {
            assert!(c.game.towers.iter().all(Option::is_none));
            command(&mut c, Action::Select(Some(1)));
            command(&mut c, Action::Wave);
        }
        controller.tap(6);
        assert_silent(&controller.frame(&mut c, 0., false, None));
        assert!(c.game.towers[1].is_none());
        controller.frame(&mut c, 0.1, false, None);
        assert_eq!(c.game.towers[1].unwrap().kind, Kind::Seed);
        assert_eq!(c.game.money, if events.restarted { 140 } else { 80 });
        assert_accounting(&c.game);
    }
}
