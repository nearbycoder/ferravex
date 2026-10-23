//! Task 174: supplied-time scenarios against the authored scene and real simulation.
//! Fixtures configure recording/peer roles and submit input; they never assign
//! positions, health, ammunition, progress, timers, or lifecycle outcomes.

use super::*;
use ferravex_engine::net::{self, ClientPacket, ServerPacket};
use ferravex_engine::play::objective::ObjectiveState;
use ferravex_engine::render::input::{self, ActionKind, Binding, InputCode, PadState, PlayerInput};
use ferravex_fps::{BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD, Player, RELOAD_TICKS};

const TICK: f64 = 1. / 64.;

/// Deliberately excludes presentation timers, audio samples and camera matrices.
#[derive(Debug, PartialEq)]
struct Simulation {
    tick: u32,
    issued: u64,
    time: f32,
    players: Vec<Player>,
    balls: Vec<([f32; 3], [f32; 3])>,
    progress: TrainingProgress,
    objectives: Vec<ObjectiveView>,
    objective_serial: u64,
    kills: u32,
    sequence: u32,
    replay: Vec<u8>,
}

fn simulation(lab: &Lab) -> Simulation {
    Simulation {
        tick: lab.game.tick,
        issued: lab.session.tick(),
        time: lab.time,
        players: lab.game.players.clone(),
        balls: lab
            .balls
            .iter()
            .map(|(handle, _)| {
                let body = lab.physics.body(*handle).unwrap();
                (body.position(), body.velocity())
            })
            .collect(),
        progress: lab.training_progress(),
        objectives: lab.training_objectives().collect(),
        objective_serial: lab.objectives.last_event_serial(),
        kills: lab.kills,
        sequence: lab.peer.sequence,
        replay: lab.replay_bytes(),
    }
}

fn aim(lab: &Lab, target: u8, height: f32) -> InputFrame {
    let delta = lab.game.player(target).unwrap().position + vec3(0., height, 0.)
        - lab.game.player(0).unwrap().eye_position();
    InputFrame {
        yaw: delta.x.atan2(-delta.z),
        pitch: delta.y.atan2(delta.x.hypot(delta.z)),
        buttons: BUTTON_FIRE,
        ..Default::default()
    }
}

fn advance(lab: &mut Lab, elapsed: f64, command: InputFrame, active: bool) {
    let before = lab.game.tick;
    let issued = lab.session.tick();
    lab.update_interactive(elapsed, command, active, 1, 16. / 9.);
    assert_eq!(
        u64::from(lab.game.tick - before),
        lab.session.tick() - issued,
        "every issued tick must actually execute, including completion/respawn batches"
    );
}

fn drill_command(index: u32, opening_shot: InputFrame) -> InputFrame {
    match index {
        0 => opening_shot,
        1..=64 => InputFrame {
            move_x: 1.,
            ..Default::default()
        },
        65 => InputFrame {
            buttons: BUTTON_RELOAD,
            ..Default::default()
        },
        _ => InputFrame::default(),
    }
}

fn assert_success(lab: &Lab) {
    assert_eq!(lab.training_progress().distance, 5.);
    assert_eq!(lab.training_progress().shots, 1);
    assert_eq!(lab.training_progress().hits, 1);
    assert_eq!(lab.training_progress().reloads, 1);
    assert_eq!(lab.training_mask(), 15);
    assert!(
        lab.training_objectives()
            .all(|view| view.state == ObjectiveState::Complete)
    );
    assert_eq!(
        lab.game.player(0).unwrap().ammo,
        ferravex_fps::MAGAZINE_SIZE
    );
    assert_eq!(lab.game.player(0).unwrap().reload_ticks, 0);
    assert_eq!(lab.game.player(3).unwrap().health, 66);
    // Courtyard is a practice sandbox: completing its drills does not finish it.
    assert_eq!(lab.session_state(), SessionState::Playing);
}

#[test]
fn authored_drills_pause_mid_reload_complete_inside_batch_and_restart_cleanly() {
    let mut lab = Lab::new_interactive();
    lab.recording = true;
    let shot = aim(&lab, 3, 0.9);
    let ready = simulation(&lab);
    advance(&mut lab, 120., shot, false);
    assert_eq!(lab.session_state(), SessionState::Ready);
    assert_eq!(simulation(&lab), ready);

    for index in 0..74 {
        advance(&mut lab, TICK, drill_command(index, shot), true);
    }
    assert_eq!(lab.training_mask(), 0b1011);
    assert_eq!(lab.game.player(0).unwrap().reload_ticks, RELOAD_TICKS - 8);
    assert_eq!(lab.training_progress().reloads, 0);
    // Preserve a pre-menu fraction, but never accumulate menu wall time.
    advance(&mut lab, TICK / 2., InputFrame::default(), true);
    let paused = simulation(&lab);
    for elapsed in [120., 0., 1. / 144.] {
        advance(&mut lab, elapsed, shot, false);
        assert_eq!(lab.session_state(), SessionState::Paused);
        assert_eq!(simulation(&lab), paused);
    }
    advance(&mut lab, TICK / 4., InputFrame::default(), true);
    assert_eq!(simulation(&lab), paused);
    advance(&mut lab, TICK / 4., InputFrame::default(), true);
    assert_eq!(lab.game.tick, 75);
    assert_eq!(lab.game.player(0).unwrap().reload_ticks, RELOAD_TICKS - 9);

    for _ in 75..193 {
        advance(&mut lab, TICK, InputFrame::default(), true);
    }
    assert_eq!(lab.game.player(0).unwrap().reload_ticks, 1);
    assert_eq!(lab.training_progress().reloads, 0);
    advance(&mut lab, 8. * TICK, InputFrame::default(), true);
    assert_eq!(
        lab.game.tick, 201,
        "completion must not truncate free practice"
    );
    assert_success(&lab);
    let replay = Replay::decode(&lab.replay_bytes()).unwrap();
    assert_eq!(replay.frames().len(), 201);
    assert_eq!(replay.frames().last().unwrap().tick, 201);

    // Leave a fractional tick before resetting through the actual host API.
    advance(&mut lab, TICK / 2., InputFrame::default(), true);
    assert!(lab.reset_interactive());
    assert_eq!(lab.session_state(), SessionState::Ready);
    assert!(!lab.recording);
    assert_eq!(simulation(&lab), simulation(&Lab::new_interactive()));
    assert_eq!(lab.particles.len(), 0);
    assert_eq!(lab.hit_marker, 0.);
    assert_eq!(lab.drill_notice, 0.);
    lab.recording = true;
    advance(&mut lab, TICK / 2., shot, true);
    assert_eq!(lab.game.tick, 0, "reset discards the old fraction");
    advance(&mut lab, TICK / 2., shot, true);
    let mut fresh = Lab::new_interactive();
    fresh.recording = true;
    advance(&mut fresh, TICK, shot, true);
    assert_eq!(simulation(&lab), simulation(&fresh));
}

fn scheduled_drills(hz: u32) -> Simulation {
    let mut lab = Lab::new_interactive();
    lab.recording = true;
    let shot = aim(&lab, 3, 0.9);
    // Start via the real lifecycle; only the per-tick input source is scripted.
    advance(&mut lab, 0., InputFrame::default(), true);
    let mut sampled = 0;
    let mut zero_frames = 0;
    let mut catch_up_frames = 0;
    let mut supplied = 0.;
    for frame in 0..4 * hz {
        if frame == 2 * hz {
            let before = simulation(&lab);
            advance(&mut lab, 90., shot, false);
            assert_eq!(simulation(&lab), before);
            advance(&mut lab, 0., InputFrame::default(), true);
        }
        // Differences of supplied timestamps total exactly four seconds. No
        // sleeps, compensating ticks or loops that run until a desired outcome.
        let elapsed = f64::from(frame + 1) / f64::from(hz) - f64::from(frame) / f64::from(hz);
        supplied += elapsed;
        let before = lab.game.tick;
        lab.update_sampled(elapsed, 1, 16. / 9., || {
            let command = drill_command(sampled, shot);
            sampled += 1;
            command
        });
        let executed = lab.game.tick - before;
        zero_frames += u32::from(executed == 0);
        catch_up_frames += u32::from(executed > 1);
        assert_eq!(lab.game.tick, sampled);
        assert_eq!(lab.session.tick(), u64::from(sampled));
        assert!(executed <= 3);
    }
    assert!((supplied - 4.).abs() < 1e-12);
    assert_eq!(sampled, 256, "{hz} Hz supplied time");
    if hz == 144 {
        assert!(zero_frames > 0);
    } else {
        assert!(catch_up_frames > 0);
    }
    assert_success(&lab);
    let replay = Replay::decode(&lab.replay_bytes()).unwrap();
    assert_eq!(replay.frames().len(), 256);
    for (index, frame) in replay.frames().iter().enumerate() {
        assert_eq!(frame.tick, index as u64 + 1);
        assert_eq!(frame.input.sequence, index as u32);
        let expected = InputFrame {
            sequence: index as u32,
            ..drill_command(index as u32, shot).sanitized()
        };
        assert_eq!(frame.input, expected);
    }
    simulation(&lab)
}

#[test]
fn thirty_sixty_and_144_hz_match_full_simulation_and_replay() {
    let expected = scheduled_drills(30);
    for hz in [60, 144] {
        assert_eq!(scheduled_drills(hz), expected, "{hz} Hz diverged");
    }
    // Re-execute the exported controls through Lab, checking every recorded hash.
    let replay = Replay::decode(&expected.replay).unwrap();
    let mut played = Lab::new_interactive();
    played.recording = true;
    for frame in replay.frames() {
        advance(&mut played, TICK, frame.input, true);
        assert_eq!(played.replay.frames().last(), Some(frame));
    }
    assert_eq!(simulation(&played), expected);
}

#[test]
fn accepted_headshot_kill_pauses_respawn_and_executes_entire_respawn_batch() {
    let mut lab = Lab::new_interactive();
    lab.recording = true;
    let shot = aim(&lab, 3, 1.6);
    advance(&mut lab, TICK, shot, true);
    assert_eq!(lab.training_progress().hits, 1);
    assert_eq!(lab.kills, 1);
    assert_eq!(lab.game.player(0).unwrap().ammo, 29);
    assert!(!lab.game.player(3).unwrap().is_alive());
    assert_eq!(
        lab.game.player(3).unwrap().respawn_ticks,
        ferravex_fps::RESPAWN_TICKS
    );
    let before = simulation(&lab);
    advance(&mut lab, 600., shot, false);
    assert_eq!(simulation(&lab), before);
    for _ in 0..ferravex_fps::RESPAWN_TICKS - 1 {
        advance(&mut lab, TICK, InputFrame::default(), true);
    }
    assert_eq!(lab.game.player(3).unwrap().respawn_ticks, 1);
    assert!(!lab.game.player(3).unwrap().is_alive());
    let tick = lab.game.tick;
    advance(&mut lab, 8. * TICK, InputFrame::default(), true);
    assert_eq!(lab.game.tick, tick + 8);
    assert_eq!(lab.game.player(3).unwrap().health, 100);
    assert_eq!(lab.game.player(3).unwrap().respawn_ticks, 0);
    assert_ne!(
        lab.game.player(3).unwrap().position,
        lab.game.arena.spawn(3)
    );
    assert_eq!(lab.kills, 1);
    assert_eq!(lab.training_progress().hits, 1);
    assert_eq!(lab.replay.frames().len(), lab.game.tick as usize);
}

fn pad(buttons: &[usize]) {
    let mut state = PadState {
        connected: true,
        ..Default::default()
    };
    for &button in buttons {
        state.buttons[button] = 1.;
    }
    assert!(input::gamepad_sample(0, state));
}

// A transient public input profile avoids reading a user's saved remappings.
// The action journal and Lab's production tick sampler both execute unchanged.
fn input_fixture() -> (PlayerInput, Lab) {
    let mut input = PlayerInput::transient("journey200-174");
    for (name, button) in [("fire", 7), ("jump", 0), ("reload", 3)] {
        input
            .action(
                name,
                name,
                ActionKind::Button,
                vec![Binding::digital(InputCode::PadButton(button))],
            )
            .unwrap();
    }
    let mut lab = Lab::new_interactive();
    lab.recording = true;
    pad(&[]);
    input_frame(&mut input, &mut lab, 0., true);
    input_frame(&mut input, &mut lab, 0., true);
    (input, lab)
}

fn input_frame(input: &mut PlayerInput, lab: &mut Lab, elapsed: f64, active: bool) {
    input::host_frame(elapsed as f32, 320, 480, || {
        input.set_paused(!active);
        input.sample(active);
        lab.reconcile_interactive(active);
        lab.update_sampled(elapsed, 1, 2. / 3., || {
            let mut buttons = 0;
            for (action, bit) in input
                .take_tick(["fire", "jump", "reload"])
                .into_iter()
                .zip([BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD])
            {
                if action.pressed || action.value > 0.5 {
                    buttons |= bit;
                }
            }
            InputFrame {
                buttons,
                ..Default::default()
            }
        });
    });
}

#[test]
fn quick_fire_jump_and_reload_survive_zero_ticks_without_catch_up_repetition() {
    let (mut input, mut lab) = input_fixture();
    pad(&[7, 0]);
    pad(&[]);
    input_frame(&mut input, &mut lab, TICK / 2., true);
    assert_eq!(lab.game.tick, 0);
    assert!(lab.replay.frames().is_empty());
    input_frame(&mut input, &mut lab, 7.5 * TICK, true);
    assert_eq!(lab.game.tick, 8);
    assert_eq!(lab.training_progress().shots, 1);
    assert_eq!(lab.game.player(0).unwrap().ammo, 29);
    assert!(!lab.game.player(0).unwrap().grounded);
    assert!(lab.game.player(0).unwrap().position.y > 0.);
    assert_eq!(
        lab.replay.frames()[0].input.buttons,
        BUTTON_FIRE | BUTTON_JUMP
    );
    assert!(
        lab.replay.frames()[1..]
            .iter()
            .all(|frame| frame.input.buttons == 0)
    );

    pad(&[3]);
    pad(&[]);
    input_frame(&mut input, &mut lab, 0., true);
    assert_eq!(lab.game.player(0).unwrap().reload_ticks, 0);
    input_frame(&mut input, &mut lab, 8. * TICK, true);
    assert_eq!(lab.game.player(0).unwrap().reload_ticks, RELOAD_TICKS - 7);
    assert_eq!(lab.replay.frames()[8].input.buttons, BUTTON_RELOAD);
    assert!(
        lab.replay.frames()[9..]
            .iter()
            .all(|frame| frame.input.buttons == 0)
    );
    assert_eq!(lab.training_progress().reloads, 0);
    for _ in 0..121 {
        input_frame(&mut input, &mut lab, TICK, true);
    }
    assert_eq!(lab.training_progress().reloads, 1);
    assert_eq!(lab.game.player(0).unwrap().ammo, 30);
    assert!(lab.game.player(0).unwrap().grounded);
    assert_eq!(lab.training_progress().shots, 1);
}

fn deliver(from: &Lab, to: &mut Lab) -> bool {
    to.peer.receive(&from.peer.packet(&from.game), &mut to.game)
}

#[test]
fn peer_menus_keep_packets_ticks_and_authoritative_damage_live_with_neutral_local_input() {
    let mut host = Lab::new_interactive();
    let mut guest = Lab::new_interactive();
    host.peer.set_mode(1);
    guest.peer.set_mode(2);
    host.recording = true;
    guest.recording = true;
    assert!(deliver(&host, &mut guest));
    let host_local = host.game.player(0).unwrap().clone();
    let guest_target = guest.game.player(2).unwrap().clone();
    let shot = aim(&guest, 2, 0.9);
    advance(&mut guest, TICK, shot, true);
    assert_eq!(guest.game.player(0).unwrap().ammo, 29);
    assert_eq!(
        guest.game.player(2).unwrap(),
        &guest_target,
        "prediction cannot damage remote players"
    );
    assert!(deliver(&guest, &mut host));
    let unwanted = InputFrame {
        move_x: 1.,
        move_z: 1.,
        yaw: 2.,
        pitch: 0.5,
        buttons: BUTTON_FIRE | BUTTON_JUMP | BUTTON_RELOAD,
        ..Default::default()
    };
    advance(&mut host, TICK, unwanted, false);
    let local = host.game.player(0).unwrap();
    assert_eq!(local.position, host_local.position);
    assert_eq!(local.velocity, host_local.velocity);
    assert_eq!((local.yaw, local.pitch), (host_local.yaw, host_local.pitch));
    assert_eq!(local.ammo, host_local.ammo);
    assert_eq!(local.reload_ticks, 0);
    assert!(!local.jump.held);
    assert_eq!(host.game.player(1).unwrap().ammo, 29);
    assert_eq!(
        host.game.player(2).unwrap().health,
        66,
        "only the host applies the accepted shot"
    );
    assert!(deliver(&host, &mut guest));
    assert!(guest.peer.pending.is_empty());
    assert_eq!(guest.game.player(2).unwrap().health, 66);

    for _ in 0..9 {
        advance(&mut guest, 8. * TICK, unwanted, false);
        assert_eq!(guest.peer.pending.len(), 8);
        let ClientPacket::Input { inputs, .. } =
            net::decode_client(&guest.peer.packet(&guest.game)).unwrap()
        else {
            panic!("guest input packet");
        };
        assert_eq!(inputs.len(), net::MAX_INPUTS_PER_PACKET.min(8));
        assert_eq!(inputs.last().unwrap().sequence, guest.peer.sequence - 1);
        for command in &inputs {
            assert_eq!(command.buttons, 0);
            assert_eq!((command.move_x, command.move_z), (0., 0.));
            assert_eq!(command.yaw, shot.sanitized().yaw);
            assert_eq!(command.pitch, shot.pitch);
        }
        assert!(deliver(&guest, &mut host));
        advance(&mut host, 8. * TICK, unwanted, false);
        let ServerPacket::Snapshot(snapshot) =
            net::decode_server(&host.peer.packet(&host.game)).unwrap()
        else {
            panic!("host snapshot");
        };
        assert!(snapshot.ack_valid);
        assert_eq!(snapshot.ack_sequence, guest.peer.sequence - 1);
        assert!(deliver(&host, &mut guest));
        assert!(guest.peer.pending.is_empty());
        assert_eq!(
            guest.game.player(0).unwrap().position,
            host.game.player(1).unwrap().position
        );
        for lab in [&host, &guest] {
            assert_eq!(lab.session_state(), SessionState::Playing);
            assert_eq!(lab.training_progress(), TrainingProgress::default());
            assert_eq!(lab.training_mask(), 0);
            assert_eq!(lab.game.tick, lab.peer.sequence);
            assert_eq!(lab.replay.frames().len(), lab.game.tick as usize);
        }
    }
    assert_eq!(host.game.tick, 73);
    assert_eq!(guest.game.tick, 73);
    assert_eq!(host.game.player(0).unwrap().ammo, 30);
    assert_eq!(guest.game.player(0).unwrap().ammo, 29);
    assert_eq!(host.game.player(2).unwrap().health, 66);
    for lab in [&mut host, &mut guest] {
        let before = simulation(lab);
        assert!(!lab.reset_interactive());
        assert_eq!(simulation(lab), before);
        lab.peer.set_mode(0);
        let disconnected = simulation(lab);
        advance(lab, 120., unwanted, false);
        assert_eq!(lab.session_state(), SessionState::Paused);
        assert_eq!(simulation(lab), disconnected);
        assert!(lab.reset_interactive());
        assert_eq!(lab.session_state(), SessionState::Ready);
        assert_eq!(lab.game.tick, 0);
    }
}

#[test]
fn tls_abi_reset_and_peer_reset_refusal_are_isolated_while_another_thread_is_live() {
    use std::{sync::mpsc, time::Duration};
    // A failed assertion must not leave the other worker stuck on a barrier.
    let (ready_tx, ready_rx) = mpsc::channel();
    let (reset_tx, reset_rx) = mpsc::channel();
    let child = std::thread::spawn(move || {
        assert_eq!(ferravex_training_reset(), 0);
        ferravex_training_init();
        ferravex_peer_mode(2);
        ferravex_record(1);
        // The actual adapter must keep a peer ticking even when local play is off.
        interactive::ferravex_controls_step(8. * TICK, 0, 1, 1., 320, 480);
        assert_eq!(ferravex_training_state(), 1);
        let before = with_lab(simulation);
        let matrix = ferravex_matrix();
        let mesh = ferravex_mesh(0);
        assert_eq!(before.tick, 8);
        assert_eq!(
            ferravex_training_reset(),
            0,
            "connected ABI reset is refused"
        );
        ready_tx.send(()).unwrap();
        reset_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(with_lab(simulation), before);
        assert_eq!(ferravex_matrix(), matrix);
        assert_eq!(ferravex_mesh(0), mesh);
        ferravex_peer_mode(0);
        assert_eq!(ferravex_training_reset(), 1);
        assert_eq!(ferravex_training_state(), 0);
        assert_eq!(ferravex_training_progress(), 0);
    });
    ferravex_init();
    ferravex_record(1);
    ferravex_step(TICK, 0., 0., 0., 0., u32::from(BUTTON_FIRE), 1, 1.);
    assert_eq!(ferravex_training_progress(), 0b0010);
    ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(ferravex_training_reset(), 1);
    assert_eq!(ferravex_training_state(), 0);
    interactive::ferravex_controls_step(TICK / 2., 1, 1, 1., 320, 480);
    assert_eq!(with_lab(|lab| lab.game.tick), 0);
    interactive::ferravex_controls_step(TICK / 2., 1, 1, 1., 320, 480);
    assert_eq!(with_lab(|lab| lab.game.tick), 1);
    let before = with_lab(simulation);
    reset_tx.send(()).unwrap();
    child.join().unwrap();
    assert_eq!(with_lab(simulation), before);
    assert_eq!(ferravex_training_state(), 1);
}
