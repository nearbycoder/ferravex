//! Task 162: real combat scenarios. Fixtures only supply elapsed time, input and
//! lifecycle requests; they never assign health, enemies, scores or outcomes.

use crate::{
    game::{Bullet, Enemy, Game, Input, Phase, Pickup, Position},
    run::{Run, combat_input},
};
use ferravex_core::session::SessionState;
use ferravex_play::{
    action::ActionPhase,
    feedback::FeedbackKind,
    input::{Action, ActionKind},
};
use ferravex_render::{
    input::{PadState, PlayerInput, Preset, gamepad_sample, host_frame},
    prelude::{Vec2, Vec3},
};
use std::cell::Cell;

const TICK: f64 = 1. / 120.;
const MAX_SECONDS: usize = 100;

/// Read existing game state without adding a production inspection interface.
#[derive(Debug, PartialEq)]
struct State {
    phase: Phase,
    time: f32,
    player: Vec2,
    facing: Vec2,
    health: i32,
    shield: f32,
    wave: u32,
    remaining: u32,
    clear: Option<f32>,
    kills: u32,
    score: u64,
    chain: u32,
    best: u32,
    shots: u32,
    magazine: (u16, u32, f32),
    progression: (u16, u64, f32),
    dash: (f32, f32),
    hurt: f32,
    banner: f32,
    marker: Option<(Vec2, f32, bool)>,
    enemies: Vec<(usize, Vec2, f32, bool, ActionPhase)>,
    bullets: Vec<(Vec2, Vec2, f32)>,
    pickups: Vec<(Vec2, f32)>,
}

fn state(g: &Game) -> State {
    State {
        phase: g.phase,
        time: g.time,
        player: g.player,
        facing: g.facing,
        health: g.health,
        shield: g.shield.value(),
        wave: g.wave,
        remaining: g.remaining_enemies(),
        clear: g.wave_clear_remaining(),
        kills: g.kills,
        score: g.score.total(),
        chain: g.score.chain(),
        best: g.score.best(),
        shots: g.shots,
        magazine: (
            g.magazine.loaded(),
            g.magazine.reserve(),
            g.magazine.remaining(),
        ),
        progression: (g.progression.level(), g.progression.xp(), g.shot_damage()),
        dash: (g.dash_time, g.dash_cooldown),
        hurt: g.hurt_time,
        banner: g.wave_banner,
        marker: g.hit_marker.map(|m| (m.position, m.remaining, m.lethal)),
        enemies: g
            .world
            .query::<(&Position, &Enemy)>()
            .iter()
            .map(|(p, e)| (e.ai_slot, p.0, e.health, e.heavy, e.attack.phase()))
            .collect(),
        bullets: g
            .world
            .query::<(&Position, &Bullet)>()
            .iter()
            .map(|(p, b)| (p.0, b.velocity, b.remaining))
            .collect(),
        pickups: g
            .world
            .query::<(&Position, &Pickup)>()
            .iter()
            .map(|(p, item)| (p.0, item.remaining))
            .collect(),
    }
}

struct Trace {
    // Index is the number of completed simulation updates, independent of the
    // session clock, which accounts for an entire catch-up batch in advance.
    states: Vec<State>,
    events: Vec<(usize, FeedbackKind, Vec3, f32)>,
    zero_frames: usize,
    catchup_frames: usize,
}

fn assert_inactive(run: &mut Run) {
    let before = state(&run.game);
    let clock = run.session.tick();
    for elapsed in [0., 1. / 144., 30.] {
        run.advance(
            elapsed,
            |_| panic!("inactive input callback"),
            |_| panic!("inactive feedback"),
        );
        assert_eq!(state(&run.game), before);
        assert_eq!(run.session.tick(), clock);
    }
}

fn play(run: &mut Run, hz: usize, input: fn(&Game) -> Input, pause_on_clear: bool) -> Trace {
    assert!(run.session.is_playing());
    assert_eq!(run.game.time, 0.);
    let mut trace = Trace {
        states: Vec::new(),
        events: Vec::new(),
        zero_frames: 0,
        catchup_frames: 0,
    };
    let ticks = Cell::new(0);
    let mut paused = false;
    let mut supplied = 0.;
    // Match the host's f32 render delta -> f64 supplied elapsed conversion.
    let elapsed = f64::from(1. / hz as f32);
    for _ in 0..hz * MAX_SECONDS {
        if pause_on_clear && !paused && run.game.wave_clear_remaining().is_some() {
            assert!(run.pause());
            assert_inactive(run);
            assert!(run.activate());
            paused = true;
        }
        let before = ticks.get();
        supplied += elapsed;
        run.advance(
            elapsed,
            |g| {
                trace.states.push(state(g));
                ticks.set(ticks.get() + 1);
                input(g)
            },
            |e| {
                trace
                    .events
                    .push((ticks.get(), e.kind, e.position, e.intensity))
            },
        );
        let steps = ticks.get() - before;
        trace.zero_frames += usize::from(steps == 0);
        trace.catchup_frames += usize::from(steps > 1);
        if !run.session.is_playing() {
            break;
        }
        assert!(
            (ticks.get() as f64 * TICK - supplied).abs() < TICK + 1e-8,
            "supplied elapsed time must advance combat at 120 Hz"
        );
    }
    assert!(
        !run.session.is_playing(),
        "scenario exceeded {MAX_SECONDS}s"
    );
    assert!(
        !pause_on_clear || paused,
        "never reached a genuine wave clear"
    );
    trace.states.push(state(&run.game));
    assert_inactive(run);
    trace
}

fn assert_same_trace(expected: &Trace, actual: &Trace) {
    assert_eq!(
        expected.states.len(),
        actual.states.len(),
        "executed update count"
    );
    for (tick, (a, b)) in expected.states.iter().zip(&actual.states).enumerate() {
        assert_eq!(a, b, "combat state after update {tick}");
    }
    assert_eq!(
        expected.events, actual.events,
        "accepted events and their update indices"
    );
}

fn assert_victory(trace: &Trace) {
    let end = trace.states.last().unwrap();
    assert_eq!(end.phase, Phase::Won);
    assert_eq!((end.wave, end.kills, end.remaining), (3, 41, 0));
    assert!(end.enemies.is_empty());
    assert!(end.health > 0);
    assert_eq!(end.progression, (4, 170, 36.));
    assert!(end.score > 4100 && end.best > 3);
    assert!(
        trace.states.iter().any(|s| s.magazine.2 > 0.),
        "ordinary combat must reload"
    );
    assert!(
        trace.states.iter().any(|s| s.enemies.iter().any(|e| e.3)),
        "later waves include heavies"
    );
    assert!(
        trace
            .events
            .iter()
            .any(|e| e.1 == FeedbackKind::Hit && e.3 < 1.),
        "heavy armor takes nonlethal hits"
    );
    assert_eq!(
        trace
            .events
            .iter()
            .filter(|e| e.1 == FeedbackKind::Fire)
            .count(),
        end.shots as usize
    );
    assert_eq!(
        trace
            .events
            .iter()
            .filter(|e| e.1 == FeedbackKind::Hit && e.3 == 1.)
            .count(),
        41
    );
    for pair in trace.states.windows(2) {
        let kills = pair[1].kills - pair[0].kills;
        let points = pair[1].score - pair[0].score;
        assert!(
            (100 * u64::from(kills)..=500 * u64::from(kills)).contains(&points),
            "only lethal hits award points, within the authored 1x..5x combo"
        );
    }

    // Assert the authored 8/14/19 wave credits, real kill accounting and the
    // full securing interval. No synthetic despawn or forced victory is used.
    for (wave, prior_kills, count) in [(1, 0, 8), (2, 8, 14), (3, 22, 19)] {
        let start = trace.states.iter().position(|s| s.wave == wave).unwrap();
        let clear = trace
            .states
            .iter()
            .position(|s| s.wave == wave && s.clear.is_some())
            .unwrap();
        let next = trace
            .states
            .iter()
            .position(|s| s.wave > wave || s.phase == Phase::Won)
            .unwrap();
        let entry = &trace.states[start];
        assert_eq!(
            (entry.kills, entry.remaining, entry.enemies.len()),
            (prior_kills, count, 0)
        );
        assert_eq!(trace.states[clear].kills, prior_kills + count);
        assert!(
            (191..=193).contains(&(next - clear)),
            "wave {wave} securing ticks: {}",
            next - clear
        );
        let first_spawn = trace.states[start..clear]
            .iter()
            .position(|s| !s.enemies.is_empty())
            .unwrap();
        let minimum_delay = if wave == 1 { 96 } else { 84 };
        assert!(
            (minimum_delay..=minimum_delay + 1).contains(&first_spawn),
            "wave {wave} entry delay: {first_spawn}"
        );
        for s in &trace.states[start..clear] {
            assert_eq!(
                s.remaining + s.kills - prior_kills,
                count,
                "wave credits cannot vanish"
            );
        }
        for pair in trace.states[clear..next].windows(2) {
            assert_eq!(
                (pair[1].kills, pair[1].score),
                (pair[0].kills, pair[0].score)
            );
            assert!(pair[1].clear.unwrap() < pair[0].clear.unwrap());
        }
        let kills = trace
            .events
            .iter()
            .filter(|e| e.0 > start && e.0 <= clear && e.1 == FeedbackKind::Hit && e.3 == 1.)
            .count();
        assert_eq!(kills, count as usize);
    }
}

#[test]
fn combat_wave_accounting_and_every_accepted_event_match_at_30_60_144_hz() {
    let mut baseline = Run::new(false);
    assert_inactive(&mut baseline);
    assert!(baseline.activate());
    let expected = play(&mut baseline, 60, Game::autoplay, false);
    assert_victory(&expected);
    assert!(expected.catchup_frames > 0);
    for hz in [30, 144] {
        let mut run = Run::new(false);
        assert!(run.activate());
        let actual = play(&mut run, hz, Game::autoplay, false);
        assert_victory(&actual);
        assert_same_trace(&expected, &actual);
        assert_eq!(run.session.state(), SessionState::Won);
        if hz == 144 {
            assert!(
                actual.zero_frames > 0,
                "144 Hz must exercise frames without an update"
            );
        } else {
            assert!(actual.catchup_frames > 0);
        }
    }
}

#[test]
fn natural_loss_retry_clear_pause_and_mid_reload_restart_reproduce_a_fresh_run() {
    let mut run = Run::new(false);
    assert!(run.activate());
    let loss = play(&mut run, 30, |_| Input::default(), false);
    assert_eq!(run.session.state(), SessionState::Lost);
    let end = loss.states.last().unwrap();
    assert_eq!((end.health, end.kills, end.shots, end.score), (0, 0, 0, 0));
    assert!(!end.enemies.is_empty());
    assert!(loss.events.iter().all(|e| e.1 == FeedbackKind::Hurt));
    assert!(
        loss.events.iter().any(|e| e.3 == 0.55),
        "first ordinary hit spends shield"
    );
    assert!(
        loss.events.iter().any(|e| e.3 == 1.),
        "later attacks damage health"
    );
    assert_eq!(
        loss.events.last().unwrap().0,
        loss.states.len() - 1,
        "lethal hit stops the catch-up batch"
    );
    assert!(run.activate());
    assert_eq!(run.session.tick(), 0);
    assert_eq!(state(&run.game), state(&Game::new()));
    let retried = play(&mut run, 144, Game::autoplay, true);
    assert_victory(&retried);
    assert!(run.activate(), "retry a real victory too");
    assert_eq!(state(&run.game), state(&Game::new()));

    // Restart while real combat has spent ammunition and is reloading. A fresh
    // trace afterwards catches hidden AI, ability, director and clock leftovers.
    for _ in 0..120 * 20 {
        run.advance(TICK, Game::autoplay, |_| {});
        if run.game.magazine.reloading() {
            break;
        }
    }
    assert!(run.game.magazine.reloading());
    assert!(run.game.shots >= 12);
    assert!(run.pause());
    assert_inactive(&mut run);
    run.restart();
    assert_eq!(run.session.state(), SessionState::Playing);
    assert_eq!(run.session.tick(), 0);
    assert_eq!(state(&run.game), state(&Game::new()));
    let restarted = play(&mut run, 60, Game::autoplay, false);
    assert_same_trace(&retried, &restarted);
}

/// Use the real TopDown bindings and renderer journal, with storage disabled.
/// This is synthetic controller input, not a physical-controller verification.
struct Controls {
    player: PlayerInput,
    pad: PadState,
}

impl Controls {
    fn new() -> Self {
        let preset = PlayerInput::preset("journey200-162-bindings", Preset::TopDown);
        let mut player = PlayerInput::transient("journey200-162");
        for name in ["fire", "boost"] {
            player
                .action(
                    name,
                    name,
                    ActionKind::Button,
                    preset.bindings(name).unwrap().to_vec(),
                )
                .unwrap();
        }
        let pad = PadState {
            connected: true,
            ..Default::default()
        };
        assert!(gamepad_sample(0, pad));
        host_frame(0., 1280, 800, || player.sample(true));
        Self { player, pad }
    }

    fn buttons(&mut self, fire: bool, dash: bool) {
        self.pad.buttons[7] = f32::from(fire);
        self.pad.buttons[0] = f32::from(dash);
        assert!(gamepad_sample(0, self.pad));
    }

    fn frame(&mut self, run: &mut Run, elapsed: f32) -> (Vec<(Action, Action)>, Vec<FeedbackKind>) {
        host_frame(elapsed, 1280, 800, || {
            self.player.set_paused(!run.session.is_playing());
            self.player.sample(run.session.is_playing());
            let mut consumed = Vec::new();
            let mut events = Vec::new();
            run.advance(
                f64::from(self.player.simulation_dt(elapsed)),
                |g| {
                    let fire = self.player.take("fire");
                    let dash = self.player.take("boost");
                    consumed.push((fire, dash));
                    combat_input(
                        Input {
                            aim: g.player + Vec2::X,
                            ..Default::default()
                        },
                        fire,
                        dash,
                        false,
                    )
                },
                |e| events.push(e.kind),
            );
            (consumed, events)
        })
    }
}

impl Drop for Controls {
    fn drop(&mut self) {
        gamepad_sample(0, PadState::default());
    }
}

#[test]
fn renderer_journal_taps_survive_empty_frames_and_held_fire_reloads_without_repeating_dash() {
    let mut run = Run::new(true);
    let mut controls = Controls::new();
    controls.buttons(true, true);
    controls.buttons(false, false);
    for elapsed in [0., 1. / 480., 1. / 480.] {
        let (taken, events) = controls.frame(&mut run, elapsed);
        assert!(taken.is_empty() && events.is_empty());
    }
    let (taken, events) = controls.frame(&mut run, 1. / 30.);
    assert_eq!(taken.len(), 4);
    for action in [taken[0].0, taken[0].1] {
        assert!(action.pressed && action.released);
        assert_eq!(action.value, 0.);
    }
    assert!(
        taken[1..]
            .iter()
            .all(|a| *a == (Action::default(), Action::default()))
    );
    assert_eq!(events, [FeedbackKind::Dash, FeedbackKind::Fire]);
    assert_eq!(run.game.shots, 1);
    let (_, events) = controls.frame(&mut run, 0.1);
    assert!(events.is_empty(), "catch-up cannot replay consumed taps");

    controls.buttons(true, true);
    let mut reloaded = false;
    let mut accepted_shots = 1;
    for _ in 0..22 {
        let (_, events) = controls.frame(&mut run, 0.1);
        assert!(
            !events.contains(&FeedbackKind::Dash),
            "held dash cannot renew when its cooldown ends"
        );
        accepted_shots += events
            .iter()
            .filter(|kind| **kind == FeedbackKind::Fire)
            .count();
        reloaded |= run.game.magazine.reloading();
    }
    assert!(reloaded);
    assert!(run.game.shots > 12);
    assert_eq!(run.game.shots as usize, accepted_shots);
    assert_eq!(run.game.dash_cooldown, 0.);
    controls.buttons(false, false);
    controls.frame(&mut run, 0.);
    controls.buttons(false, true);
    let (_, events) = controls.frame(&mut run, 1. / 120.);
    assert_eq!(
        events.iter().filter(|e| **e == FeedbackKind::Dash).count(),
        1
    );
}

#[test]
fn pause_discards_real_unsimulated_edges_and_suppresses_held_controls_until_release() {
    let mut run = Run::new(true);
    let mut controls = Controls::new();
    controls.buttons(true, true);
    assert!(controls.frame(&mut run, 0.).0.is_empty());
    assert!(run.pause());
    assert!(controls.frame(&mut run, 0.1).0.is_empty());
    assert_inactive(&mut run);
    assert!(run.activate());
    for _ in 0..3 {
        let (taken, events) = controls.frame(&mut run, 0.1);
        assert!(
            taken
                .iter()
                .all(|a| *a == (Action::default(), Action::default()))
        );
        assert!(events.is_empty());
    }
    assert_eq!((run.game.shots, run.game.dash_cooldown), (0, 0.));
    controls.buttons(false, false);
    controls.frame(&mut run, 0.);
    controls.buttons(true, true);
    controls.buttons(false, false);
    let (_, events) = controls.frame(&mut run, 0.1);
    assert_eq!(events, [FeedbackKind::Dash, FeedbackKind::Fire]);
    assert_eq!(run.game.shots, 1);
}
