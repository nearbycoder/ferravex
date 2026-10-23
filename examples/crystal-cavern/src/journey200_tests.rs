//! Authored-course scenarios: only lifecycle calls and real movement/use inputs
//! change the game. No teleports, relocated objects or injected objective events.

use super::*;
use ferravex_play::input::{ActionMap, ContextId};
use ferravex_play::objective::ObjectiveSnapshot;

const TICK: f64 = 1. / 60.;
const MAX_TICKS: u64 = 7_200;

#[derive(Debug, PartialEq)]
struct Observation {
    position: Vec2,
    velocity: Vec2,
    yaw: f32,
    elapsed: f32,
    notice: String,
    notice_time: f32,
    collected: [bool; 3],
    switches: u8,
    won: bool,
    focus: Option<u64>,
    objectives: ObjectiveSnapshot<6>,
}

fn observe(game: &Game) -> Observation {
    Observation {
        position: game.position,
        velocity: game.velocity,
        yaw: game.yaw,
        elapsed: game.elapsed,
        notice: game.notice.clone(),
        notice_time: game.notice_time,
        collected: game.collected,
        switches: game.switches,
        won: game.won,
        focus: game.focus.current(),
        objectives: game.objectives.snapshot(),
    }
}

fn assert_no_progress(progress: Progress) {
    assert_eq!(progress.completed, 0);
    assert_eq!(progress.pickups, 0);
    assert_eq!(progress.altars, 0);
    assert!(!progress.won);
}

fn start() -> Expedition {
    let mut expedition = Expedition::new(false);
    let ready = observe(&expedition.game);
    assert_no_progress(expedition.advance(90., || panic!("Ready sampled gameplay input")));
    assert_eq!(expedition.session.state(), SessionState::Ready);
    assert_eq!(expedition.session.tick(), 0);
    assert_eq!(observe(&expedition.game), ready);
    expedition.start();
    assert_eq!(expedition.session.state(), SessionState::Playing);
    assert_no_progress(expedition.advance(90., || panic!("start frame sampled gameplay input")));
    assert_eq!(expedition.session.tick(), 0);
    assert_eq!(observe(&expedition.game), ready);
    expedition
}

// Exercise manual input dispatch using the existing smoke driver's legitimate
// commands. One fixed tick per call lets the driver observe every game step.
fn pilot_tick(expedition: &mut Expedition) -> Progress {
    assert!(!expedition.autoplay);
    let input = expedition.pilot.input(&expedition.game);
    expedition.advance(TICK, || input)
}

fn finish_course(expedition: &mut Expedition) {
    for _ in 0..MAX_TICKS {
        if expedition.game.won {
            break;
        }
        pilot_tick(expedition);
    }
    assert_won(expedition);
}

fn assert_won(expedition: &Expedition) {
    assert_eq!(expedition.session.state(), SessionState::Won);
    assert!(expedition.game.won);
    assert_eq!(expedition.game.collected, [true; 3]);
    assert_eq!(expedition.game.switches, 2);
    assert_eq!(expedition.game.completed_mask(), 0b11_1111);
    assert_eq!(expedition.game.objectives.last_event_serial(), 6);
    assert!(expedition.game.open());
    assert!(expedition.game.position.distance(at(b'X')) < 1.2);
    assert!(expedition.game.elapsed > 1. && expedition.game.elapsed < 120.);
}

#[derive(Default, Debug, PartialEq)]
struct Accepted {
    seen: u64,
    order: Vec<usize>,
    pickups: usize,
    altars: u8,
    wins: usize,
}

impl Accepted {
    fn record(&mut self, progress: Progress) {
        assert_eq!(self.seen & progress.completed, 0, "duplicate completion");
        assert_eq!(progress.completed & !0b11_1111, 0);
        self.seen |= progress.completed;
        for index in 0..6 {
            if progress.completed & (1 << index) != 0 {
                self.order.push(index);
            }
        }
        self.pickups += progress.pickups;
        self.altars += progress.altars;
        self.wins += usize::from(progress.won);
    }

    fn assert_complete(&self) {
        assert_eq!(self.seen, 0b11_1111);
        assert_eq!(self.order, [0, 1, 2, 3, 4, 5]);
        assert_eq!((self.pickups, self.altars, self.wins), (3, 2, 1));
    }
}

struct CourseRun {
    checkpoints: Vec<Observation>,
    ticks: Vec<u64>,
    accepted: Accepted,
}

fn run_at(hz: u64) -> CourseRun {
    let mut expedition = Expedition::new(true);
    let mut run = CourseRun {
        checkpoints: Vec::new(),
        ticks: Vec::new(),
        accepted: Accepted::default(),
    };
    let mut frames = 0;
    // All three rates divide into half-second blocks. Compare actual state at
    // identical supplied times throughout the route, not merely a win boolean.
    for _ in 0..240 {
        for _ in 0..hz / 2 {
            let playing = expedition.session.is_playing();
            run.accepted
                .record(expedition.advance(1. / hz as f64, Input::default));
            frames += 1;
            if playing {
                assert_eq!(expedition.session.tick(), frames * 60 / hz);
            }
        }
        run.checkpoints.push(observe(&expedition.game));
        run.ticks.push(expedition.session.tick());
        if expedition.game.won {
            break;
        }
    }
    assert_won(&expedition);
    run.accepted.assert_complete();
    run
}

#[test]
fn authored_course_matches_at_30_60_and_144_hz_through_all_six_objectives() {
    let reference = run_at(60);
    let winning_tick = *reference.ticks.last().unwrap();
    let expected_elapsed = (0..winning_tick).fold(0_f32, |time, _| time + TICK as f32);
    assert_eq!(
        reference.checkpoints.last().unwrap().elapsed,
        expected_elapsed
    );
    for hz in [30, 144] {
        let candidate = run_at(hz);
        assert_eq!(candidate.checkpoints.len(), reference.checkpoints.len());
        assert_eq!(candidate.accepted, reference.accepted);
        for (index, (actual, expected)) in candidate
            .checkpoints
            .iter()
            .zip(&reference.checkpoints)
            .enumerate()
        {
            assert_eq!(actual, expected, "{hz} Hz, half-second block {}", index + 1);
            if !actual.won {
                assert_eq!(candidate.ticks[index], (index as u64 + 1) * 30);
            }
        }
        // Session ticks count scheduled work, including any unused final batch
        // tick. The game timer/pose/objectives must still stop at the winning tick.
        let expected_ticks = if hz == 30 {
            winning_tick.div_ceil(2) * 2
        } else {
            winning_tick
        };
        assert_eq!(*candidate.ticks.last().unwrap(), expected_ticks);
    }
}

#[test]
fn walked_course_survives_pause_resume_then_replays_identically_after_retry() {
    let mut expedition = start();
    for _ in 0..MAX_TICKS {
        if pilot_tick(&mut expedition).pickups == 1 {
            break;
        }
    }
    assert_eq!(expedition.game.count(), 1);
    assert!(expedition.game.velocity.length() > 0.);
    let tick = expedition.session.tick();
    let mut paused = observe(&expedition.game);
    paused.velocity = Vec2::ZERO;
    let goal = expedition.pilot.goal;
    let path = expedition.pilot.path.clone();
    assert_no_progress(expedition.advance(TICK / 4., || panic!("fractional tick sampled input")));
    expedition.set_paused(true);
    assert_eq!(expedition.session.state(), SessionState::Paused);
    for elapsed in [0., 1. / 144., 120.] {
        assert_no_progress(expedition.advance(elapsed, || panic!("pause sampled gameplay input")));
        assert_eq!(expedition.session.tick(), tick);
        assert_eq!(observe(&expedition.game), paused);
        assert_eq!(expedition.pilot.goal, goal);
        assert_eq!(expedition.pilot.path, path);
    }
    expedition.start();
    assert_eq!(expedition.session.state(), SessionState::Paused);
    expedition.set_paused(false);
    assert_no_progress(expedition.advance(120., || panic!("refocus frame sampled gameplay input")));
    assert_no_progress(expedition.advance(TICK / 2., || panic!("fractional tick sampled input")));
    assert_eq!(expedition.session.tick(), tick);
    assert_eq!(observe(&expedition.game), paused);
    let mut samples = 0;
    assert_no_progress(expedition.advance(TICK / 4., || {
        samples += 1;
        Input::default()
    }));
    assert_eq!(samples, 1);
    assert_eq!(expedition.session.tick(), tick + 1);
    assert_eq!(expedition.game.elapsed, paused.elapsed + TICK as f32);
    assert_eq!(expedition.game.position, paused.position);
    finish_course(&mut expedition);

    expedition.restart();
    assert_eq!(expedition.session.state(), SessionState::Playing);
    assert_eq!(expedition.session.tick(), 0);
    assert_eq!(expedition.game.position, at(b'S'));
    assert_eq!(expedition.game.velocity, Vec2::ZERO);
    assert_eq!(expedition.game.yaw, 0.);
    assert_eq!(expedition.game.elapsed, 0.);
    assert_eq!(
        expedition.game.objectives.snapshot(),
        Game::new().objectives.snapshot()
    );
    assert_eq!(expedition.game.collected, [false; 3]);
    assert_eq!(expedition.game.switches, 0);
    assert!(!expedition.game.won);
    assert!(!expedition.game.open());
    assert_eq!(expedition.game.focus.current(), None);
    assert_eq!(expedition.pilot.goal, None);
    assert!(expedition.pilot.path.is_empty());
    assert_no_progress(expedition.advance(120., || panic!("retry frame sampled gameplay input")));
    assert_eq!(expedition.session.tick(), 0);
    finish_course(&mut expedition);
    let reference = run_at(60);
    assert_eq!(
        observe(&expedition.game),
        *reference.checkpoints.last().unwrap()
    );
    assert_eq!(expedition.session.tick(), *reference.ticks.last().unwrap());
}

fn interaction_map() -> (ActionMap, usize, InputCode) {
    let mut map = ActionMap::default();
    let key = InputCode::Key(KeyCode::E as u16);
    let action = map
        .named_action(
            "interact",
            ActionKind::Button,
            ContextId::GLOBAL,
            vec![Binding::digital(key)],
        )
        .unwrap();
    (map, action, key)
}

#[test]
fn quick_tap_outside_range_is_not_replayed_when_catchup_walks_into_range() {
    let mut expedition = start();
    // Walk the authored route to just outside crystal 0's range, withholding
    // use input. Eight catch-up ticks will carry the player into real range.
    for _ in 0..MAX_TICKS {
        let distance = expedition
            .game
            .position
            .distance(expedition.game.crystals[0]);
        if (1.85..2.).contains(&distance) {
            break;
        }
        let mut input = expedition.pilot.input(&expedition.game);
        input.interact = false;
        assert_no_progress(expedition.advance(TICK, || input));
    }
    let distance = expedition
        .game
        .position
        .distance(expedition.game.crystals[0]);
    assert!(
        (1.85..2.).contains(&distance),
        "approach distance: {distance}"
    );
    assert_eq!(expedition.game.target(), None);
    assert_eq!(expedition.game.count(), 0);
    let tick = expedition.session.tick();
    let before = observe(&expedition.game);
    let (mut map, action, key) = interaction_map();
    map.feed(key, 1.);
    map.feed(key, 0.);
    let mut samples = 0;
    for _ in 0..2 {
        assert_no_progress(expedition.advance(TICK / 4., || {
            samples += 1;
            Input {
                interact: map.consume_tick(action).pressed,
                ..Default::default()
            }
        }));
    }
    assert_eq!(samples, 0);
    assert!(map.peek(action).pressed && map.peek(action).released);
    assert_eq!(map.peek(action).value, 0.);
    assert_eq!(expedition.session.tick(), tick);
    assert_eq!(observe(&expedition.game), before);
    let movement = expedition.pilot.input(&expedition.game);
    assert_no_progress(expedition.advance(7.5 * TICK, || {
        samples += 1;
        Input {
            interact: map.consume_tick(action).pressed,
            ..movement
        }
    }));
    assert_eq!(samples, 1);
    assert_eq!(expedition.session.tick(), tick + 8);
    assert_eq!(expedition.game.target(), Some(Interactable::Crystal(0)));
    assert_eq!(expedition.game.objectives.last_event_serial(), 0);
    assert!(!map.peek(action).pressed && !map.peek(action).released);

    // A new press/release survives another zero-tick frame, then accepts exactly
    // one real pickup. Holding no key cannot generate later pickups or cues.
    map.feed(key, 1.);
    map.feed(key, 0.);
    assert_no_progress(expedition.advance(TICK / 4., || panic!("tap consumed without a tick")));
    let progress = expedition.advance(7.75 * TICK, || Input {
        interact: map.consume_tick(action).pressed,
        ..Default::default()
    });
    assert_eq!(
        (progress.completed, progress.pickups, progress.altars),
        (1, 1, 0)
    );
    assert!(!progress.won);
    assert_eq!(expedition.session.tick(), tick + 16);
    assert_eq!(expedition.game.collected, [true, false, false]);
    assert_eq!(expedition.game.objectives.last_event_serial(), 1);
    assert_no_progress(expedition.advance(8. * TICK, || Input {
        interact: map.consume_tick(action).pressed,
        ..Default::default()
    }));
    assert_eq!(expedition.session.tick(), tick + 24);
    assert_eq!(
        expedition.game.elapsed,
        (0..24).fold(before.elapsed, |time, _| time + TICK as f32)
    );
    finish_course(&mut expedition);
}

#[test]
fn walking_to_the_closed_seal_cannot_skip_the_puzzle_and_active_retry_recovers() {
    let mut expedition = start();
    let north = Input {
        movement: Vec2::Y,
        ..Default::default()
    };
    // The real central corridor leads from spawn to the closed seal, without
    // needing to recover a crystal or alter any authored geometry.
    for _ in 0..600 {
        assert_no_progress(expedition.advance(TICK, || north));
    }
    assert_eq!(expedition.session.tick(), 600);
    assert_eq!(expedition.game.target(), Some(Interactable::Gate));
    assert!((expedition.game.position.x - at(b'G').x).abs() < 0.001);
    assert!(expedition.game.position.y >= expedition.game.gate.max.y + RADIUS);
    assert!(expedition.game.position.y < expedition.game.gate.max.y + RADIUS + 0.01);
    let position = expedition.game.position;
    let objectives = expedition.game.objectives.snapshot();
    let before = expedition.game.elapsed;
    assert_no_progress(expedition.advance(8. * TICK, || Input {
        interact: true,
        ..north
    }));
    assert_eq!(expedition.session.tick(), 608);
    assert_eq!(expedition.game.position, position);
    assert_eq!(expedition.game.objectives.snapshot(), objectives);
    assert!(expedition.game.notice.contains("Seal locked"));
    assert_eq!(
        expedition.game.elapsed,
        (0..8).fold(before, |time, _| time + TICK as f32)
    );
    assert!(!expedition.game.won);
    // Restart also clears a partially accumulated tick during active play.
    assert_no_progress(expedition.advance(TICK * 0.75, || panic!("no tick due")));
    expedition.restart();
    assert_no_progress(expedition.advance(30., || panic!("retry frame")));
    assert_no_progress(expedition.advance(TICK / 2., || panic!("stale pre-retry fraction")));
    assert_eq!(expedition.session.tick(), 0);
    assert_eq!(expedition.game.elapsed, 0.);
    assert_eq!(expedition.game.position, at(b'S'));
    assert_no_progress(expedition.advance(TICK / 2., Input::default));
    assert_eq!(expedition.session.tick(), 1);
    finish_course(&mut expedition);
}

#[test]
fn real_exit_stops_on_the_first_winning_tick_of_an_eight_tick_batch() {
    let mut reference = Expedition::new(true);
    let mut catchup = Expedition::new(true);
    let mut accepted = Accepted::default();
    let mut reached_exit = false;
    for _ in 0..MAX_TICKS {
        let progress = reference.advance(TICK, Input::default);
        if progress.won {
            // The second expedition reached the same pre-exit pose by walking
            // every preceding tick; only its last supplied frame is longer.
            assert!(catchup.game.open());
            assert!(!catchup.game.won);
            assert!(catchup.game.position.distance(at(b'X')) >= 1.2);
            let tick = catchup.session.tick();
            let elapsed = catchup.game.elapsed;
            let progress = catchup.advance(8. * TICK, Input::default);
            assert_eq!(progress.completed, 1 << 5);
            assert_eq!(
                (progress.pickups, progress.altars, progress.won),
                (0, 0, true)
            );
            accepted.record(progress);
            assert_eq!(catchup.session.tick(), tick + 8);
            assert_eq!(reference.session.tick(), tick + 1);
            assert_eq!(catchup.game.elapsed, elapsed + TICK as f32);
            assert_eq!(observe(&catchup.game), observe(&reference.game));
            reached_exit = true;
            break;
        }
        accepted.record(catchup.advance(TICK, Input::default));
    }
    assert!(
        reached_exit,
        "autoplay never reached the authored daylight exit"
    );
    assert_won(&catchup);
    accepted.assert_complete();
    let finished = observe(&catchup.game);
    let tick = catchup.session.tick();
    catchup.set_paused(true);
    catchup.start();
    for elapsed in [0., 1. / 144., 8. * TICK, 120.] {
        assert_no_progress(catchup.advance(elapsed, || panic!("terminal frame sampled input")));
        assert_eq!(catchup.session.state(), SessionState::Won);
        assert_eq!(catchup.session.tick(), tick);
        assert_eq!(observe(&catchup.game), finished);
    }
}
