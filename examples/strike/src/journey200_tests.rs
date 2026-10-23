//! Headless scenarios for the production host, controls, bots and replay driver.
//! No health, weapon timers, outcomes or authority state are assigned by fixtures.

use super::{Controls, Host, Mode, bots::PracticeBots, practice, replay, reset_practice};
use ferravex_fps::{
    BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD, GameEvent, InputFrame, MAGAZINE_SIZE, Match,
    RELOAD_TICKS, RESPAWN_TICKS, TICK_RATE, replay::Capture,
};
use ferravex_play::replay::Limits;
use ferravex_render::input::{
    ActionKind, Binding, InputCode, PadState, PlayerInput, gamepad_sample, host_frame,
    touch::{Event, Phase},
    touch_event,
};

const DT: f64 = 1. / TICK_RATE as f64;

#[derive(Debug, PartialEq)]
struct Tick {
    number: u32,
    hash: u64,
    commands: Vec<(u8, InputFrame)>,
    events: Vec<GameEvent>,
}

struct Practice {
    host: Host,
    game: Match,
    bots: PracticeBots,
    replays: replay::Session,
    commands: Vec<(u8, InputFrame)>,
}

impl Practice {
    fn new(autoplay: bool) -> Self {
        let game = practice();
        Self {
            host: Host::new(autoplay),
            bots: PracticeBots::new(&game.arena),
            game,
            replays: replay::Session::default(),
            commands: Vec::with_capacity(ferravex_fps::MAX_PLAYERS),
        }
    }

    // Same command/authority/capture/bot-event order as the practice branch in game().
    fn tick(&mut self, input: InputFrame) -> Tick {
        self.bots.commands(&self.game, input, &mut self.commands);
        let events = self.game.step(&self.commands);
        self.replays.capture(&self.game, &self.commands, &events);
        self.bots.events(&events);
        Tick {
            number: self.game.tick,
            hash: self.game.state_hash().unwrap(),
            commands: self.commands.clone(),
            events,
        }
    }

    fn frame(
        &mut self,
        elapsed: f64,
        available: bool,
        mut input: impl FnMut(u32) -> InputFrame,
    ) -> Vec<Tick> {
        let frame = self.host.advance(Mode::Practice, elapsed, available, true);
        assert_eq!(frame.dropped_seconds, 0.);
        (0..frame.steps)
            .map(|_| {
                let command = self.host.command(input(self.game.tick), available);
                self.tick(command)
            })
            .collect()
    }
}

fn neutral(_: u32) -> InputFrame {
    InputFrame::default()
}

// Inputs are indexed by authority tick, so render cadence never changes the script.
fn script(tick: u32) -> InputFrame {
    InputFrame {
        move_z: f32::from(tick < 64),
        move_x: f32::from((64..128).contains(&tick)),
        buttons: if tick < 96 { BUTTON_FIRE } else { 0 }
            | if tick == 16 { BUTTON_JUMP } else { 0 }
            | if tick == 96 { BUTTON_RELOAD } else { 0 },
        ..Default::default()
    }
}

fn controls() -> Controls {
    let mut controls = Controls::default();
    // Use the real adapter and touch layout, with a session-only profile so
    // machine-local remaps cannot change the scenario or get overwritten.
    controls.input = PlayerInput::transient("journey200-171");
    for (name, id) in [("fire", 2), ("jump", 3), ("reload", 5)] {
        controls
            .input
            .action(
                name,
                name,
                ActionKind::Button,
                vec![Binding::digital(InputCode::Touch(id))],
            )
            .unwrap();
    }
    controls
        .input
        .global_action(
            "replay_pause",
            "Pause replay",
            ActionKind::Button,
            vec![Binding::digital(InputCode::PadButton(9))],
        )
        .unwrap();
    controls
}

fn tap(id: u64, x: f32, y: f32) {
    for phase in [Phase::Start, Phase::End] {
        touch_event(Event {
            id,
            phase,
            position: ferravex_render::vec2(x, y),
        });
    }
}

fn controlled_frame(run: &mut Practice, controls: &mut Controls, elapsed: f64) -> Vec<Tick> {
    host_frame(elapsed as f32, 800, 600, || {
        controls.prepare(run.host.active());
        let input = controls.sample(run.host.active());
        let available = controls.input.gameplay_active();
        run.frame(elapsed, available, |_| controls.tick(input))
    })
}

#[test]
fn start_pause_settings_resume_restart_reproduce_the_actual_bot_run() {
    let mut run = Practice::new(false);
    let initial = run.game.state_hash().unwrap();
    assert!(!run.host.has_played());
    assert!(run.frame(90., true, script).is_empty());
    assert_eq!(run.game.state_hash().unwrap(), initial);

    run.host.resume();
    assert!(run.host.has_played());
    let mut first = Vec::new();
    for _ in 0..16 {
        first.extend(run.frame(8. * DT, true, script));
    }
    assert_eq!(first.len(), 128);
    assert!(first.iter().any(|t| {
        t.events
            .iter()
            .any(|e| matches!(e, GameEvent::Shot { player: 0, .. }))
    }));
    assert_ne!(run.game.players[0].position, run.game.arena.spawn(0));
    assert!(run.game.players[0].reload_ticks > 0);
    let paused = run.game.state_hash().unwrap();
    run.host.open_menu();
    assert!(run.frame(90., true, script).is_empty());
    run.host.resume();
    assert!(run.frame(90., false, script).is_empty());
    assert_eq!(run.game.state_hash().unwrap(), paused);
    assert!(run.frame(0., true, script).is_empty());
    assert_eq!(run.frame(DT, true, script).len(), 1);
    assert_eq!(run.game.tick, 129);

    // A retry clears a partial batch as well as the world and every bot's memory.
    assert!(run.frame(DT / 2., true, script).is_empty());
    run.host.open_menu();
    assert!(run.host.restart_practice(Mode::Practice));
    reset_practice(&mut run.game, &mut run.bots, &mut run.replays);
    assert_eq!(run.game.state_hash().unwrap(), initial);
    assert!(run.frame(DT / 2., true, script).is_empty());
    let mut repeated = run.frame(DT / 2., true, script);
    for _ in 1..128 {
        repeated.extend(run.frame(DT, true, script));
    }
    assert_eq!(
        repeated, first,
        "retry must also reset bot decisions and events"
    );

    // Production's Back to briefing path resets the practice world before Ready.
    reset_practice(&mut run.game, &mut run.bots, &mut run.replays);
    run.host.return_to_briefing();
    assert!(!run.host.active());
    assert!(!run.host.has_played());
    assert!(run.frame(90., true, script).is_empty());
    assert_eq!(run.game.state_hash().unwrap(), initial);
}

#[test]
fn supplied_30_60_144_hz_frames_have_identical_commands_events_and_states() {
    let mut expected = None;
    for hz in [30, 60, 144] {
        let mut run = Practice::new(true);
        let mut trace = Vec::new();
        let mut zero = 0;
        let mut catch_up = 0;
        // Match the host's f32 presentation delta widened to f64, for 12 seconds.
        let elapsed = f64::from(1. / hz as f32);
        for _ in 0..hz * 12 {
            let ticks = run.frame(elapsed, true, script);
            zero += usize::from(ticks.is_empty());
            catch_up += usize::from(ticks.len() > 1);
            trace.extend(ticks);
        }
        assert_eq!(run.game.tick, 12 * TICK_RATE, "{hz} Hz executed tick count");
        assert_eq!(trace.len(), 12 * TICK_RATE as usize);
        assert!(
            trace.iter().any(|t| t
                .events
                .iter()
                .any(|e| matches!(e, GameEvent::Hit { victim: 0, .. }))),
            "the script must reach actual combat"
        );
        if hz == 144 {
            assert!(zero > 0);
        } else {
            assert!(catch_up > 0);
        }
        if let Some(expected) = &expected {
            assert_eq!(&trace, expected, "{hz} Hz changed authoritative history");
        } else {
            expected = Some(trace);
        }
    }
}

#[test]
fn bot_hits_kill_the_player_and_respawn_counts_only_executed_ticks() {
    let mut run = Practice::new(true);
    let mut death = None;
    for _ in 0..30 * TICK_RATE {
        let before = run.game.players[0].health;
        let tick = run.frame(DT, true, neutral).pop().unwrap();
        let mut remaining = before;
        for event in &tick.events {
            if let GameEvent::Hit {
                attacker,
                victim: 0,
                damage,
                ..
            } = *event
            {
                assert!(
                    tick.events.iter().any(
                        |e| matches!(e, GameEvent::Shot { player, .. } if *player == attacker)
                    )
                );
                remaining = remaining.saturating_sub(damage);
            }
        }
        assert_eq!(run.game.players[0].health, remaining);
        if tick
            .events
            .iter()
            .any(|e| matches!(e, GameEvent::Killed { victim: 0, .. }))
        {
            death = Some(tick.number);
            break;
        }
    }
    let death = death.expect("unmodified Dockyard bots must kill an idle player within 30 seconds");
    assert_eq!(run.game.players[0].health, 0);
    assert_eq!(run.game.players[0].respawn_ticks, RESPAWN_TICKS);
    let epoch = run.game.life_epochs()[0];
    let dead_hash = run.game.state_hash().unwrap();
    run.host.open_menu();
    assert!(run.frame(90., false, neutral).is_empty());
    assert_eq!(run.game.state_hash().unwrap(), dead_hash);
    run.host.resume();
    assert!(run.frame(0., true, neutral).is_empty());
    for elapsed in 1..=RESPAWN_TICKS {
        let tick = run.frame(DT, true, neutral).pop().unwrap();
        assert_eq!(tick.number, death + u32::from(elapsed));
        let respawned = tick.events.contains(&GameEvent::Respawned { player: 0 });
        assert_eq!(respawned, elapsed == RESPAWN_TICKS);
        if !respawned {
            assert!(!run.game.players[0].is_alive());
            assert_eq!(run.game.players[0].respawn_ticks, RESPAWN_TICKS - elapsed);
            assert_eq!(run.game.life_epochs()[0], epoch);
        }
    }
    assert_eq!(run.game.life_epochs()[0], epoch + 1);
    assert!(run.game.players[0].is_alive());
    assert_eq!(run.game.players[0].position, run.game.arena.spawn(0));
    assert_eq!(run.game.players[0].ammo, MAGAZINE_SIZE);
}

#[test]
fn quick_fire_jump_and_reload_taps_reach_gameplay_once_after_zero_ticks() {
    let mut run = Practice::new(true);
    let mut controls = controls();
    assert!(controlled_frame(&mut run, &mut controls, 0.).is_empty());
    tap(1710, 726., 504.); // FIRE in the production 800x600 layout.
    tap(1711, 630., 534.); // UP.
    let initial = run.game.state_hash().unwrap();
    assert!(controlled_frame(&mut run, &mut controls, DT / 2.).is_empty());
    assert_eq!(run.game.state_hash().unwrap(), initial);
    let ticks = controlled_frame(&mut run, &mut controls, 7.5 * DT);
    assert_eq!(ticks.len(), 8);
    let buttons: Vec<_> = ticks.iter().map(|t| t.commands[0].1.buttons).collect();
    assert_eq!(buttons, [BUTTON_FIRE | BUTTON_JUMP, 0, 0, 0, 0, 0, 0, 0]);
    let shots = ticks
        .iter()
        .flat_map(|t| &t.events)
        .filter(|e| matches!(e, GameEvent::Shot { player: 0, .. }))
        .count();
    assert_eq!(shots, 1, "eight ticks span a second fire opportunity");
    assert_eq!(run.game.players[0].ammo, MAGAZINE_SIZE - 1);
    assert!(run.game.players[0].position.y > 0.);
    assert!(!run.game.players[0].grounded);

    tap(1712, 736., 403.); // LOAD.
    assert!(controlled_frame(&mut run, &mut controls, DT / 2.).is_empty());
    assert_eq!(run.game.players[0].reload_ticks, 0);
    let ticks = controlled_frame(&mut run, &mut controls, 2.5 * DT);
    assert_eq!(ticks.len(), 3);
    assert_eq!(ticks[0].commands[0].1.buttons, BUTTON_RELOAD);
    assert!(ticks[1..].iter().all(|t| t.commands[0].1.buttons == 0));
    assert_eq!(run.game.players[0].reload_ticks, RELOAD_TICKS - 2);
    assert_eq!(run.game.players[0].ammo, MAGAZINE_SIZE - 1);
    // A held trigger has ammo and no fire cooldown, but reload rejects the shots.
    touch_event(Event {
        id: 1715,
        phase: Phase::Start,
        position: ferravex_render::vec2(726., 504.),
    });
    let ticks = controlled_frame(&mut run, &mut controls, 8. * DT);
    assert_eq!(ticks.len(), 8);
    assert!(ticks.iter().all(|t| t.commands[0].1.buttons == BUTTON_FIRE));
    assert!(
        ticks
            .iter()
            .flat_map(|t| &t.events)
            .all(|e| !matches!(e, GameEvent::Shot { player: 0, .. }))
    );
    assert_eq!(run.game.players[0].reload_ticks, RELOAD_TICKS - 10);
    assert_eq!(run.game.players[0].ammo, MAGAZINE_SIZE - 1);
    touch_event(Event {
        id: 1715,
        phase: Phase::End,
        position: ferravex_render::vec2(726., 504.),
    });
    controlled_frame(&mut run, &mut controls, 0.);
}

#[test]
fn menu_settings_and_retry_discard_pending_fire_before_authority_resumes() {
    for transition in ["menu", "settings", "retry"] {
        let mut run = Practice::new(true);
        let mut controls = controls();
        controlled_frame(&mut run, &mut controls, 0.);
        tap(1713, 726., 504.);
        assert!(controlled_frame(&mut run, &mut controls, DT / 2.).is_empty());
        if transition == "settings" {
            controls.input.set_controls_open(true);
        }
        run.host.open_menu();
        assert!(controlled_frame(&mut run, &mut controls, 90.).is_empty());
        if transition == "settings" {
            controls.input.set_controls_open(false);
        }
        if transition == "retry" {
            assert!(run.host.restart_practice(Mode::Practice));
            reset_practice(&mut run.game, &mut run.bots, &mut run.replays);
            controls.cut();
            run.host.cut_replay();
        } else {
            run.host.resume();
        }
        let ticks = controlled_frame(&mut run, &mut controls, 8. * DT);
        assert_eq!(ticks.len(), 8);
        assert!(
            ticks.iter().all(|t| t.commands[0].1.buttons == 0),
            "{transition}"
        );
        assert_eq!(run.game.players[0].ammo, MAGAZINE_SIZE, "{transition}");
        tap(1714, 726., 504.);
        let ticks = controlled_frame(&mut run, &mut controls, DT);
        assert!(
            ticks[0]
                .events
                .iter()
                .any(|e| matches!(e, GameEvent::Shot { player: 0, .. }))
        );
        assert_eq!(run.game.players[0].ammo, MAGAZINE_SIZE - 1);
    }
}

fn recording(ticks: u32) -> (Practice, Vec<Tick>, Vec<u8>) {
    let mut run = Practice::new(true);
    // Public in-memory capture avoids any user replay storage.
    let mut capture = Capture::new(&run.game, 0, Limits::default()).unwrap();
    let mut trace = Vec::new();
    for _ in 0..ticks {
        let tick = run.frame(DT, true, script).pop().unwrap();
        capture
            .push(&run.game, &tick.commands, &[], &tick.events)
            .unwrap();
        trace.push(tick);
    }
    (run, trace, capture.finish().encode())
}

fn toggle_replay_pause(controls: &mut Controls, replays: &mut replay::Session, game: &Match) {
    let mut pad = PadState {
        connected: true,
        ..Default::default()
    };
    assert!(gamepad_sample(0, pad));
    host_frame(0., 800, 600, || controls.prepare(false));
    for pressed in [true, false] {
        pad.buttons[9] = f32::from(pressed);
        assert!(gamepad_sample(0, pad));
        host_frame(0., 800, 600, || {
            controls.prepare(false);
            assert!(!replays.controls(&mut controls.input, game, false));
        });
    }
    assert!(gamepad_sample(0, PadState::default()));
}

#[test]
fn real_combat_replays_finish_under_menus_at_all_cadences_without_extra_ticks() {
    let (live, trace, bytes) = recording(137);
    let live_hash = live.game.state_hash().unwrap();
    assert!(trace.iter().any(|t| !t.events.is_empty()));
    // The fourth cadence forces 7 unexecuted slots in the terminal 8-tick batch.
    for hz in [30, 60, 144, 8] {
        let mut replays = replay::Session::default();
        replays.import(&bytes, &live.game).unwrap();
        let mut host = Host::new(true);
        host.open_menu();
        assert!(!host.restart_practice(Mode::Replay));
        let mut controls = controls();
        toggle_replay_pause(&mut controls, &mut replays, &live.game);
        assert!(replays.paused);
        assert_eq!(
            host.advance(Mode::Replay, 90., false, !replays.paused)
                .steps,
            0
        );
        assert!(!replays.advance());
        assert_eq!(replays.playback.as_ref().unwrap().position(), 0);
        toggle_replay_pause(&mut controls, &mut replays, &live.game);
        assert!(!replays.paused);
        assert_eq!(host.advance(Mode::Replay, 0., false, true).steps, 0);

        let mut issued = 0;
        let mut executed = 0;
        for _ in 0..hz * 4 {
            let frame = host.advance(
                Mode::Replay,
                f64::from(1. / hz as f32),
                false,
                !replays.paused,
            );
            assert_eq!(frame.dropped_seconds, 0.);
            issued += frame.steps;
            for _ in 0..frame.steps {
                if replays.advance() {
                    let world = replays.world.as_ref().unwrap();
                    let expected = &trace[executed];
                    assert_eq!(world.game.tick, expected.number);
                    assert_eq!(world.game.state_hash().unwrap(), expected.hash, "{hz} Hz");
                    assert_eq!(world.events, expected.events, "{hz} Hz");
                    executed += 1;
                } else {
                    assert!(replays.playback.as_ref().unwrap().finished());
                }
            }
            if replays.playback.as_ref().unwrap().finished() {
                break;
            }
        }
        assert_eq!(executed, 137);
        assert!(issued >= executed as u32);
        if hz == 8 {
            assert_eq!(issued, 144);
        }
        assert_eq!(replays.world.as_ref().unwrap().game.tick, 137);
        assert!(replays.playback.as_ref().unwrap().finished());
        assert!(!replays.advance());
        assert_eq!(
            replays.world.as_ref().unwrap().game.state_hash().unwrap(),
            live_hash
        );
        assert_eq!(live.game.state_hash().unwrap(), live_hash);
        assert!(!host.active());
        replays.toggle_playback(&live.game);
        assert!(!replays.playing());
        assert_eq!(live.game.state_hash().unwrap(), live_hash);
    }
}

#[test]
fn network_mode_menus_keep_authority_running_with_neutral_local_commands() {
    // Exercise host-to-authority commands without sockets or an invented client.
    // Transport, remote timing and replication remain separate integration checks.
    for menu in [true, false] {
        let mut run = Practice::new(true);
        let mut expected = Practice::new(true);
        let held = InputFrame {
            yaw: 0.25,
            pitch: 0.1,
            move_x: 1.,
            move_z: 1.,
            buttons: BUTTON_FIRE | BUTTON_JUMP | BUTTON_RELOAD,
            ..Default::default()
        };
        let neutral = InputFrame {
            yaw: held.yaw,
            pitch: held.pitch,
            ..Default::default()
        };
        if menu {
            run.host.open_menu();
        }
        assert!(!run.host.restart_practice(Mode::Network));
        assert_eq!(
            run.host.advance(Mode::Network, DT / 2., false, false).steps,
            0
        );
        run.host.cut_replay();
        let mut events = Vec::new();
        for index in 0..12 * TICK_RATE {
            let elapsed = if index == 0 { DT / 2. } else { DT };
            let frame = run.host.advance(Mode::Network, elapsed, menu, false);
            assert_eq!(
                frame.steps, 1,
                "menus and replay cuts preserve network cadence"
            );
            let command = run.host.command(held, menu);
            assert_eq!(command, neutral);
            let tick = run.tick(command);
            assert_eq!(tick, expected.tick(neutral));
            events.extend(tick.events);
        }
        assert_eq!(run.game.tick, 12 * TICK_RATE);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::Hit { victim: 0, .. }))
        );
        assert!(
            events
                .iter()
                .all(|e| !matches!(e, GameEvent::Shot { player: 0, .. }))
        );
    }
}
