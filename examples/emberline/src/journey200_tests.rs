//! Task 161: scenarios reached through authored terrain, real input and fixed ticks.
//! Fixtures never assign character poses, pickups, checkpoints, timers or outcomes.

use super::*;
use ferravex_play::gameplay::{
    checkpoint::CheckpointRecovery, platforms::PlatformPhase, trial::Medal,
};
use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};

const STEP: f64 = 1. / 120.;
const DT: f32 = STEP as f32;
const MAX_COURSE_TICKS: usize = 2400;

/// Compare accepted simulation ticks, not the session's already-budgeted batch
/// counter: the winning update can stop partway through a catch-up frame.
#[derive(Debug, PartialEq)]
struct State {
    phase: SessionState,
    motor: Character,
    platforms: Vec<Rect>,
    embers: Vec<(Vec2, bool)>,
    checkpoint: CheckpointRecovery<Vec2>,
    deaths: u32,
    jumps: u32,
    inventory: u32,
    score: (u64, u32, u32, u32),
    air_jump_used: bool,
    spring_flight: bool,
    crumbles: [(PlatformPhase, f32); 2],
    elapsed: f32,
    trial_elapsed: f64,
    splits: [Option<f64>; 3],
    medal: Option<Medal>,
    events: [bool; 6],
    notices: [f32; 4],
    landing: (Vec2, f32),
}

fn state(game: &Game) -> State {
    State {
        phase: game.session.state(),
        motor: game.motor,
        platforms: game.platforms.clone(),
        embers: game.embers.clone(),
        checkpoint: game.checkpoints.recovery(),
        deaths: game.deaths,
        jumps: game.jumps,
        inventory: game.ember_inventory.count(1),
        score: (
            game.score.total(),
            game.score.chain(),
            game.score.best(),
            game.score.multiplier(),
        ),
        air_jump_used: game.air_jump_used,
        spring_flight: game.spring_flight,
        crumbles: game.crumbles.map(|p| (p.phase(), p.remaining())),
        elapsed: game.elapsed,
        trial_elapsed: game.trial.elapsed(),
        splits: *game.trial.splits(),
        medal: game.trial.medal(),
        events: [
            game.feedback.jumped,
            game.feedback.landed,
            game.feedback.pickup,
            game.feedback.checkpoint,
            game.feedback.respawned,
            game.feedback.won,
        ],
        notices: [
            game.respawn_notice,
            game.checkpoint_notice,
            game.jump_feedback,
            game.landing_feedback,
        ],
        landing: (game.landing_at, game.landing_strength),
    }
}

fn started_game() -> Game {
    let mut game = Game::new(false);
    let ready = state(&game);
    assert_eq!(ready.phase, SessionState::Ready);
    assert_eq!(
        game.advance(
            60.,
            |_| panic!("Ready sampled input"),
            |_, _| panic!("Ready ticked")
        ),
        0.
    );
    assert_eq!(state(&game), ready);
    assert!(!game.menu(false, true, false));
    assert!(game.session.is_playing());
    assert_eq!(game.session.tick(), 0);
    game
}

fn tick(game: &mut Game, input: Input) {
    assert_eq!(game.advance(STEP, |_| input, |_, _| {}), DT);
}

fn drive_until(
    game: &mut Game,
    mut input: impl FnMut(&Game) -> Input,
    reached: impl Fn(&Game) -> bool,
) {
    for _ in 0..MAX_COURSE_TICKS {
        if reached(game) {
            return;
        }
        let next = input(game);
        tick(game, next);
    }
    assert!(
        reached(game),
        "scenario did not reach its target: {:?}",
        state(game)
    );
}

struct Run {
    ticks: Vec<State>,
    zero_frames: usize,
    catch_up_frames: usize,
    largest_batch: usize,
}

fn finish_course(game: &mut Game, elapsed_frames: &[f64]) -> Run {
    let mut run = Run {
        ticks: Vec::new(),
        zero_frames: 0,
        catch_up_frames: 0,
        largest_batch: 0,
    };
    // A bounded 30-second budget at the highest tested render rate.
    for frame in 0..144 * 30 {
        let before = run.ticks.len();
        game.advance(
            elapsed_frames[frame % elapsed_frames.len()],
            Game::autoplay,
            |g, dt| {
                assert_eq!(dt, DT);
                assert!(run.ticks.len() < MAX_COURSE_TICKS);
                assert!(
                    run.ticks
                        .last()
                        .is_none_or(|s| s.phase != SessionState::Won)
                );
                run.ticks.push(state(g));
            },
        );
        let count = run.ticks.len() - before;
        run.zero_frames += usize::from(count == 0);
        run.catch_up_frames += usize::from(count > 1);
        run.largest_batch = run.largest_batch.max(count);
        if game.won() {
            return run;
        }
    }
    panic!("authored course did not finish: {:?}", state(game));
}

fn same_ticks(expected: &Run, actual: &Run) {
    assert_eq!(actual.ticks.len(), expected.ticks.len());
    for (i, (actual, expected)) in actual.ticks.iter().zip(&expected.ticks).enumerate() {
        assert_eq!(actual, expected, "accepted simulation tick {}", i + 1);
    }
}

#[test]
fn authored_course_has_identical_tick_outcomes_at_30_60_144_hz_and_jitter() {
    let mut baseline = started_game();
    let reference = finish_course(&mut baseline, &[STEP]);
    assert_eq!(reference.ticks.len(), 1092);
    assert_eq!(baseline.deaths, 0);
    assert_eq!(baseline.jumps, 7);
    assert_eq!(baseline.checkpoints.index(), 2);
    assert!(baseline.checkpoints.recovery().next.is_none());
    assert!(baseline.trial.finished());
    assert_eq!(baseline.trial.medal(), Some(Medal::Gold));
    let splits = baseline.trial.splits().map(Option::unwrap);
    assert_eq!(
        splits,
        [354., 766., 1092.].map(|ticks| ticks * f64::from(DT))
    );
    assert!(0. < splits[0] && splits[0] < splits[1] && splits[1] < splits[2]);
    assert_eq!(splits[2], baseline.trial.elapsed());
    assert_eq!(
        baseline.trial.elapsed(),
        reference.ticks.len() as f64 * f64::from(DT)
    );
    let collected = baseline.embers.iter().filter(|(_, found)| *found).count();
    // The ground-route autopilot collects the mill gap ember and the last ember;
    // the three-ember air-jump unlock is exercised on a separate manual route.
    assert_eq!(collected, 2);
    assert!(baseline.embers[5].1 && baseline.embers[9].1);
    assert_eq!(baseline.ember_inventory.count(1) as usize, collected);
    assert_eq!(baseline.score.chain() as usize, collected);
    let expected_score: u64 = (0..collected)
        .map(|i| 100 * (1 + i as u64 / 2).min(4))
        .sum();
    assert_eq!(baseline.score.total(), expected_score);
    assert_eq!(
        reference
            .ticks
            .iter()
            .filter(|s| s.events[3])
            .map(|s| s.checkpoint.current.index)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(reference.ticks.iter().filter(|s| s.events[5]).count(), 1);
    assert!(!reference.ticks.iter().any(|s| s.events[4]));

    for hz in [30, 60, 144] {
        let mut game = started_game();
        // Match the host's supplied f32 elapsed -> f64 conversion.
        let run = finish_course(&mut game, &[f64::from(1. / hz as f32)]);
        same_ticks(&reference, &run);
        if hz == 144 {
            assert!(run.zero_frames > 0);
            assert_eq!(run.largest_batch, 1);
        } else {
            assert!(run.catch_up_frames > 0);
            assert_eq!(run.largest_batch, 120 / hz as usize);
        }
    }
    let mut game = started_game();
    let jitter = finish_course(&mut game, &[0., STEP * 0.25, STEP * 11.75, 0., STEP * 2.]);
    same_ticks(&reference, &jitter);
    assert!(jitter.zero_frames > 0 && jitter.catch_up_frames > 0);
    assert_eq!(jitter.largest_batch, 12);
}

#[test]
fn real_checkpoint_death_preserves_earnings_then_pause_resume_and_retry_clear_the_course() {
    let mut game = started_game();
    drive_until(&mut game, Game::autoplay, |g| g.checkpoints.index() == 1);
    assert_eq!(game.deaths, 0);
    assert!(game.feedback.checkpoint);
    assert!(game.score.total() > 0 && game.ember_inventory.count(1) > 0);
    let saved = game.checkpoints.recovery();
    let split = game.trial.splits()[0];
    let earnings = (
        game.embers.clone(),
        game.ember_inventory.count(1),
        game.score.total(),
    );
    let before_death_time = game.trial.elapsed();
    // Stop jumping: ordinary rightward movement reaches the authored spike at x=1600.
    drive_until(
        &mut game,
        |_| Input {
            direction: 1.,
            ..Input::default()
        },
        |g| g.deaths == 1,
    );
    assert!(game.feedback.respawned);
    assert!(!game.feedback.jumped && !game.feedback.pickup && !game.feedback.checkpoint);
    assert_eq!(game.position(), saved.current.anchor);
    assert_eq!(game.velocity(), Vec2::ZERO);
    assert_eq!(game.checkpoints.recovery(), saved);
    assert_eq!(
        (
            game.embers.clone(),
            game.ember_inventory.count(1),
            game.score.total()
        ),
        earnings
    );
    assert_eq!(game.score.chain(), 0);
    assert_eq!(game.trial.splits()[0], split);
    assert!(game.trial.elapsed() > before_death_time);
    assert!(!game.air_jump_used && !game.spring_flight);
    assert_eq!(game.motor.jump.buffer, 0.);
    assert!(game.crumbles.iter().all(|c| c.solid()));

    // Stand at the real recovery anchor: touching it again awards nothing.
    for _ in 0..120 {
        tick(&mut game, Input::default());
        assert!(!game.feedback.checkpoint && !game.feedback.respawned && !game.feedback.pickup);
    }
    assert_eq!(game.score.total(), earnings.2);
    assert_eq!(game.checkpoints.recovery(), saved);
    assert_eq!(
        game.advance(STEP * 0.25, |_| panic!("fractional tick"), |_, _| {}),
        0.
    );
    assert!(!game.menu(false, false, true));
    assert!(game.session.is_paused());
    let paused = state(&game);
    let paused_tick = game.session.tick();
    for elapsed in [0., 1. / 144., 1. / 30., 120.] {
        assert_eq!(
            game.advance(
                elapsed,
                |_| panic!("paused input"),
                |_, _| panic!("paused tick")
            ),
            0.
        );
        assert_eq!(state(&game), paused);
        assert_eq!(game.session.tick(), paused_tick);
    }
    assert!(!game.menu(false, true, false));
    assert!(game.session.is_playing());
    let mut resumed = 0;
    game.advance(STEP * 0.75, |_| Input::default(), |_, _| resumed += 1);
    assert_eq!(resumed, 1, "pause retains only the pre-menu fraction");
    assert_eq!(game.session.tick(), paused_tick + 1);
    finish_course(&mut game, &[1. / 30.]);
    assert_eq!(game.deaths, 1);
    let won = state(&game);
    assert!(!game.menu(false, false, true));
    game.advance(120., |_| panic!("won input"), |_, _| panic!("won tick"));
    assert_eq!(state(&game), won);

    assert!(game.menu(false, true, false));
    assert_eq!(state(&game), state(&started_game()));
    assert_eq!(game.session.tick(), 0);
    assert_eq!(
        game.advance(STEP * 0.5, |_| panic!("retry debt"), |_, _| {}),
        0.
    );
    let retry = finish_course(&mut game, &[f64::from(1. / 144_f32)]);
    let reference = finish_course(&mut started_game(), &[STEP]);
    same_ticks(&reference, &retry);
}

/// Use the shared action queue, including actual press/release coalescing.
/// This is the pure tick-input boundary; it does not drive a browser device.
struct JumpButton {
    map: ActionMap,
    jump: usize,
}

impl JumpButton {
    fn new() -> Self {
        let mut map = ActionMap::default();
        let jump = map
            .named_action(
                "jump",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        Self { map, jump }
    }

    fn down(&mut self, down: bool) {
        self.map.feed(InputCode::Key(32), f32::from(down));
    }

    fn advance(&mut self, game: &mut Game, elapsed: f64) -> Vec<State> {
        let mut states = Vec::new();
        game.advance(
            elapsed,
            |_| {
                let action = self.map.consume_tick(self.jump);
                Input {
                    jump: action.pressed,
                    jump_held: action.value > 0.5,
                    direction: 0.,
                }
            },
            |g, _| states.push(state(g)),
        );
        states
    }
}

#[test]
fn quick_released_tap_survives_zero_ticks_and_catchup_without_an_automatic_rejump() {
    let mut game = started_game();
    tick(&mut game, Input::default());
    assert!(game.motor.grounded);
    let mut button = JumpButton::new();
    button.down(true);
    assert!(button.advance(&mut game, STEP * 0.25).is_empty());
    button.down(false);
    assert!(button.advance(&mut game, STEP * 0.25).is_empty());
    let queued = button.map.peek(button.jump);
    assert!(queued.pressed && queued.released);
    assert_eq!(queued.value, 0.);
    let batch = button.advance(&mut game, STEP * 11.5);
    assert_eq!(batch.len(), 12);
    assert!(batch[0].events[0]);
    assert_eq!(batch.iter().filter(|s| s.events[0]).count(), 1);
    assert_eq!(game.jumps, 1);
    assert!(!game.motor.grounded);
    let mut landings = 0;
    for _ in 0..30 {
        for s in button.advance(&mut game, STEP * 12.) {
            landings += usize::from(s.events[1]);
            assert!(!s.events[0]);
        }
    }
    assert_eq!(landings, 1);
    assert!(game.motor.grounded);
    assert_eq!(game.jumps, 1);
    assert_eq!(game.deaths, 0);
}

fn first_gap() -> Game {
    let mut game = started_game();
    drive_until(&mut game, Game::autoplay, |g| {
        g.position().x > 490. && g.motor.grounded
    });
    drive_until(
        &mut game,
        |_| Input {
            direction: 1.,
            ..Input::default()
        },
        |g| !g.motor.grounded,
    );
    assert_eq!(game.deaths, 0);
    assert!(game.position().x > 570. && game.position().x < 620.);
    assert!(game.motor.jump.coyote > 0.);
    assert!(
        game.ember_inventory.count(1) < 3,
        "no earned air jump may mask coyote expiry"
    );
    game
}

#[test]
fn walking_off_the_real_first_gap_preserves_the_coyote_window_and_then_expires() {
    let mut early = first_gap();
    let mut late = first_gap();
    assert_eq!(state(&early), state(&late));
    let prior_jumps = early.jumps;
    // Leave only a few ticks before the authored 100 ms window closes.
    for _ in 0..9 {
        tick(
            &mut early,
            Input {
                direction: 1.,
                ..Input::default()
            },
        );
        tick(
            &mut late,
            Input {
                direction: 1.,
                ..Input::default()
            },
        );
    }
    let press = Input {
        direction: 1.,
        jump: true,
        jump_held: true,
    };
    tick(&mut early, press);
    assert_eq!(early.jumps, prior_jumps + 1);
    assert!(early.feedback.jumped && early.motor.velocity.y > 600.);
    for _ in 0..4 {
        tick(
            &mut late,
            Input {
                direction: 1.,
                ..Input::default()
            },
        );
    }
    assert_eq!(late.motor.jump.coyote, 0.);
    tick(&mut late, press);
    assert_eq!(late.jumps, prior_jumps);
    assert!(!late.feedback.jumped);
    assert!(late.motor.jump.buffer > 0.);
    // Continue the missed gap without steering back to safety.
    drive_until(&mut late, |_| Input::default(), |g| g.deaths == 1);
    assert!(late.feedback.respawned);
    assert_eq!(late.checkpoints.index(), 0);
    assert_eq!(late.position(), late.checkpoints.recovery().current.anchor);
    assert_eq!(late.motor.jump.buffer, 0.);
}

fn descending_jump(below_y: f32) -> Game {
    let mut game = started_game();
    tick(&mut game, Input::default());
    tick(
        &mut game,
        Input {
            jump: true,
            jump_held: true,
            ..Input::default()
        },
    );
    drive_until(
        &mut game,
        |_| Input {
            jump_held: true,
            ..Input::default()
        },
        |g| g.velocity().y > 0. && g.position().y > below_y,
    );
    assert_eq!(game.jumps, 1);
    assert!(!game.motor.grounded);
    assert_eq!(game.deaths, 0);
    game
}

#[test]
fn natural_landing_accepts_a_quick_buffered_tap_but_pause_and_restart_cancel_it() {
    let mut baseline = descending_jump(480.);
    let mut button = JumpButton::new();
    button.down(true);
    button.down(false);
    let before = baseline.jumps;
    let press = button.advance(&mut baseline, STEP);
    assert_eq!(press.len(), 1);
    assert_eq!(baseline.jumps, before);
    assert!(!baseline.feedback.jumped);
    assert!(baseline.motor.jump.buffer > 0.);
    let landing_batch = button.advance(&mut baseline, STEP * 12.);
    let landed = landing_batch
        .iter()
        .position(|s| s.events[1])
        .expect("real floor contact");
    let jumped = landing_batch
        .iter()
        .position(|s| s.events[0])
        .expect("buffered takeoff");
    assert_eq!(jumped, landed + 1);
    assert!((jumped + 1) as f32 * DT < BUFFER);
    assert_eq!(landing_batch.iter().filter(|s| s.events[0]).count(), 1);
    assert_eq!(baseline.jumps, before + 1);

    for restart in [false, true] {
        let mut game = descending_jump(480.);
        let mut button = JumpButton::new();
        button.down(true);
        button.down(false);
        button.advance(&mut game, STEP);
        assert!(game.motor.jump.buffer > 0.);
        assert!(button.advance(&mut game, STEP * 0.5).is_empty());
        assert!(!game.menu(false, false, true));
        assert_eq!(game.motor.jump.buffer, 0.);
        button.advance(&mut game, 60.);
        if restart {
            assert!(game.menu(true, false, false));
            assert_eq!(state(&game), state(&started_game()));
            assert!(
                button.advance(&mut game, STEP * 0.5).is_empty(),
                "restart clears fraction"
            );
        } else {
            assert!(!game.menu(false, true, false));
        }
        for _ in 0..20 {
            let ticks = button.advance(&mut game, STEP * 12.);
            assert!(ticks.iter().all(|s| !s.events[0]));
        }
        assert!(game.motor.grounded);
        assert_eq!(game.jumps, u32::from(!restart));
        assert_eq!(game.deaths, 0);
    }

    // The same press earlier in a real fall must expire before floor contact.
    let mut early = descending_jump(400.);
    let mut button = JumpButton::new();
    button.down(true);
    button.down(false);
    button.advance(&mut early, STEP);
    assert!(early.motor.jump.buffer > 0.);
    assert_eq!(early.jumps, 1);
    let pressed_at = early.trial.elapsed();
    let mut landings = 0;
    for _ in 0..20 {
        for s in button.advance(&mut early, STEP * 12.) {
            assert!(!s.events[0]);
            if s.events[1] {
                assert!(s.trial_elapsed - pressed_at > f64::from(BUFFER));
                landings += 1;
            }
        }
    }
    assert_eq!(landings, 1);
    assert!(early.motor.grounded);
    assert_eq!(early.motor.jump.buffer, 0.);
    assert_eq!(early.jumps, 1);
    assert_eq!(early.deaths, 0);
}

#[test]
fn three_authored_pickups_unlock_adjacent_air_jump_presses_after_real_recovery() {
    let mut game = started_game();
    tick(&mut game, Input::default());
    tick(
        &mut game,
        Input {
            direction: 1.,
            jump: true,
            jump_held: true,
        },
    );
    drive_until(
        &mut game,
        |_| Input {
            direction: 1.,
            jump_held: true,
            ..Input::default()
        },
        |g| g.ember_inventory.count(1) > 0,
    );
    assert!(
        game.embers[0].1,
        "the opening manual jump earns the raised ember"
    );
    // Follow the high arc into two further real pickups, then the missed gap.
    // This route naturally returns to the trailhead with its earned inventory.
    drive_until(&mut game, Game::autoplay, |g| {
        g.ember_inventory.count(1) >= 3 && g.motor.grounded
    });
    assert_eq!(game.ember_inventory.count(1), 3);
    assert_eq!(game.embers.iter().filter(|(_, found)| *found).count(), 3);
    assert_eq!(game.score.total(), 400);
    assert_eq!(game.score.chain(), 0);
    assert_eq!(game.deaths, 1);
    assert_eq!(game.checkpoints.index(), 0);
    let prior_jumps = game.jumps;
    let mut button = JumpButton::new();
    button.down(true);
    assert_eq!(button.advance(&mut game, STEP).len(), 1);
    assert_eq!(game.jumps, prior_jumps + 1);
    assert!(game.feedback.jumped);
    assert!(!game.air_jump_used);
    button.down(false);
    button.down(true); // Two distinct presses on consecutive accepted ticks.
    assert_eq!(button.advance(&mut game, STEP).len(), 1);
    assert_eq!(game.jumps, prior_jumps + 2);
    assert!(game.air_jump_used && game.feedback.jumped);
    assert!(game.motor.velocity.y > 500.);
    button.down(false);
    button.down(true);
    let rejected = button.advance(&mut game, STEP * 12.);
    assert_eq!(rejected.len(), 12);
    assert!(rejected.iter().all(|s| !s.events[0]));
    assert_eq!(game.jumps, prior_jumps + 2);
    // A physical hold cannot become another jump when landing restores the ability.
    let mut landings = 0;
    for _ in 0..20 {
        for s in button.advance(&mut game, STEP * 12.) {
            landings += usize::from(s.events[1]);
            assert!(!s.events[0]);
        }
    }
    assert_eq!(landings, 1);
    assert!(game.motor.grounded);
    assert!(!game.air_jump_used);
    assert_eq!(game.jumps, prior_jumps + 2);
    button.down(false);
    button.down(true);
    button.advance(&mut game, STEP);
    button.down(false);
    button.down(true);
    button.advance(&mut game, STEP);
    assert!(game.air_jump_used && game.feedback.jumped);
    assert_eq!(game.jumps, prior_jumps + 4);
    assert_eq!(game.ember_inventory.count(1), 3);
    assert_eq!(game.score.total(), 400);
    assert_eq!(game.deaths, 1);
}

// Public host_frame advances the real native input journal without a window or
// GPU. The gameplay adapter below follows the live host's sample/gate/advance
// order; drawing, audio, browser events and physical touch hardware are excluded.
fn touch_frame(game: &mut Game, input: &mut PlayerInput, elapsed: f64) -> Vec<State> {
    ferravex_render::input::host_frame(elapsed as f32, 1280, 720, || {
        input.sample(game.session.is_playing());
        game.gate_input(input.gameplay_active());
        let mut ticks = Vec::new();
        game.advance(
            f64::from(input.simulation_dt(elapsed as f32)),
            |_| Input {
                direction: 0.,
                jump: input.pressed("jump"),
                jump_held: input.held("jump"),
            },
            |g, _| ticks.push(state(g)),
        );
        ticks
    })
}

fn touch_jump(phase: ferravex_render::input::touch::Phase) {
    ferravex_render::input::touch_event(ferravex_render::input::touch::Event {
        id: 161,
        phase,
        position: vec2(50., 50.),
    });
}

#[test]
fn player_input_journal_retains_a_quick_touch_tap_and_rearms_after_pause() {
    use ferravex_render::input::touch::Phase;
    let mut game = started_game();
    let mut input = PlayerInput::transient("emberline-journey200-161");
    input
        .action(
            "jump",
            "Jump",
            ActionKind::Button,
            vec![Binding::digital(InputCode::Touch(3))],
        )
        .unwrap();
    input
        .set_touch_layout(&[Control {
            region: Region::Rect(Rect::new(0., 0., 100., 100.)),
            output: Output::Button(3),
        }])
        .unwrap();
    assert_eq!(touch_frame(&mut game, &mut input, STEP).len(), 1);
    assert!(game.motor.grounded);
    touch_jump(Phase::Start);
    assert!(touch_frame(&mut game, &mut input, STEP * 0.25).is_empty());
    assert!(input.held("jump"));
    touch_jump(Phase::End);
    assert!(touch_frame(&mut game, &mut input, STEP * 0.25).is_empty());
    assert!(!input.held("jump"));
    assert!(touch_frame(&mut game, &mut input, 0.).is_empty());
    // Supply a full catch-up frame: f32 host elapsed is intentionally rounded,
    // so the two quarter ticks plus 11.5 ticks need not total exactly 12.
    let catch_up = touch_frame(&mut game, &mut input, STEP * 12.);
    assert_eq!(catch_up.len(), 12);
    assert!(catch_up[0].events[0]);
    assert_eq!(catch_up.iter().filter(|s| s.events[0]).count(), 1);
    for _ in 0..20 {
        assert!(
            touch_frame(&mut game, &mut input, STEP * 12.)
                .iter()
                .all(|s| !s.events[0])
        );
    }
    assert!(game.motor.grounded);
    assert_eq!(game.jumps, 1);

    // A pending press and a continued touch both cross a real pause/resume.
    touch_jump(Phase::Start);
    assert!(touch_frame(&mut game, &mut input, 0.).is_empty());
    assert!(!game.menu(false, false, true));
    assert!(touch_frame(&mut game, &mut input, 60.).is_empty());
    assert!(!game.menu(false, true, false));
    let resumed = touch_frame(&mut game, &mut input, STEP * 12.);
    assert_eq!(resumed.len(), 12);
    assert!(resumed.iter().all(|s| !s.events[0]));
    assert!(game.motor.grounded);
    assert_eq!(game.jumps, 1);
    touch_jump(Phase::End);
    assert!(touch_frame(&mut game, &mut input, 0.).is_empty());
    touch_jump(Phase::Start);
    let fresh = touch_frame(&mut game, &mut input, STEP);
    assert_eq!(fresh.len(), 1);
    assert!(game.feedback.jumped);
    assert_eq!(game.jumps, 2);
    touch_jump(Phase::End);
    touch_frame(&mut game, &mut input, 0.);
    assert_eq!(game.deaths, 0);
}
