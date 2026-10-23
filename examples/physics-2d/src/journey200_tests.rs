//! Task 172: real Lab scenarios, without a renderer or synthetic solver outcomes.
//!
//! Inputs enter the existing controller; snapshots only observe the live world.
//! Equality here is for supplied time on this native target, not a cross-platform
//! determinism guarantee. This sandbox starts playing and has no win/loss state.

use super::*;
use ferravex_physics::{BodySleepState, ColliderHandle};
use ferravex_render::input::{Action, PadState, gamepad_sample, host_frame};

#[derive(Debug, PartialEq)]
struct BodyState {
    handle: RigidBodyHandle,
    position: Vec2,
    previous_position: Vec2,
    angle: f32,
    previous_angle: f32,
    velocity: Vec2,
    angular_velocity: f32,
    sleep: BodySleepState,
}

#[derive(Debug, PartialEq)]
struct LabState {
    tick: u64,
    paused: bool,
    ball: bool,
    selected: Option<RigidBodyHandle>,
    bodies: Vec<BodyState>,
    anchors: Vec<[Vec3; 2]>,
    contacts: Vec<(ColliderHandle, ColliderHandle, [f32; 3], bool)>,
    events: Vec<ContactEvent>,
    pulse_time: f32,
    pulse_center: Vec2,
    impact_cooldown: f32,
}

fn state(lab: &Lab) -> LabState {
    let sandbox = &lab.sandbox;
    LabState {
        tick: lab.session.tick(),
        paused: lab.session.is_paused(),
        ball: lab.ball,
        selected: lab.selected,
        bodies: sandbox
            .bodies
            .iter()
            .map(|body| {
                let rigid = &sandbox.physics.bodies[body.handle];
                BodyState {
                    handle: body.handle,
                    position: rigid.translation(),
                    previous_position: body.previous_position,
                    angle: rigid.rotation().angle(),
                    previous_angle: body.previous_angle,
                    velocity: rigid.linvel(),
                    angular_velocity: rigid.angvel(),
                    sleep: sandbox.physics.sleep_state(body.handle).unwrap(),
                }
            })
            .collect(),
        anchors: sandbox
            .links
            .iter()
            .map(|&joint| sandbox.physics.joint_anchors(joint).unwrap())
            .collect(),
        contacts: sandbox
            .physics
            .contacts()
            .map(|pair| (pair.collider1, pair.collider2, pair.normal, pair.sensor))
            .collect(),
        events: sandbox.physics.contact_events().to_vec(),
        pulse_time: sandbox.pulse_time,
        pulse_center: sandbox.pulse_center,
        impact_cooldown: lab.impact_cooldown,
    }
}

fn toggle_pause(lab: &mut Lab) {
    let was_paused = lab.session.is_paused();
    let frame = lab.update(
        30.,
        LabInput {
            toggle_pause: true,
            ..Default::default()
        },
    );
    assert_eq!(frame.steps, 0, "transition time must not enter the clock");
    assert_eq!(lab.session.is_paused(), !was_paused);
}

fn run_frames(lab: &mut Lab, rate: u32, frames: u32) {
    let before = lab.session.tick();
    for _ in 0..frames {
        let frame = lab.update(1. / f64::from(rate), LabInput::default());
        assert_eq!(frame.dropped_seconds, 0.);
        assert_eq!(frame.dt, 1. / 120.);
    }
    assert_eq!(lab.session.tick() - before, u64::from(frames * 120 / rate));
}

fn workshop(rate: u32) -> Vec<LabState> {
    let mut lab = Lab::new();
    assert!(lab.session.is_playing());
    assert_eq!(lab.sandbox.bodies.len(), 34);
    let mut checkpoints = vec![state(&lab)];
    run_frames(&mut lab, rate, rate / 2);
    toggle_pause(&mut lab);

    // A crate in the clear strip beside the right platform falls to the floor.
    // Placement must be immediate while paused, with no hidden physics update.
    let before = state(&lab);
    let frame = lab.update(
        120.,
        LabInput {
            ball: Some(false),
            drop: Some((vec2(10.8, 4.), false)),
            ..Default::default()
        },
    );
    assert_eq!(frame.steps, 0);
    assert_eq!(state(&lab).bodies[..34], before.bodies);
    let handle = lab.selected.unwrap();
    assert!(matches!(
        lab.sandbox.bodies.last().unwrap().shape,
        Shape::Box(_)
    ));
    assert_eq!(
        lab.sandbox.physics.bodies[handle].translation(),
        vec2(10.8, 4.)
    );
    let placed = state(&lab);
    assert_eq!(lab.update(3600., LabInput::default()).steps, 0);
    assert_eq!(state(&lab), placed);

    // Manual ticks use the real paused-step path; wall time still contributes none.
    for _ in 0..12 {
        let frame = lab.update(
            60.,
            LabInput {
                single_step: true,
                ..Default::default()
            },
        );
        assert_eq!((frame.steps, frame.alpha), (1, 1.));
        assert!(lab.session.is_paused());
    }
    let rigid = &lab.sandbox.physics.bodies[handle];
    assert!(rigid.translation().y < 4. && rigid.linvel().y < 0.);
    checkpoints.push(state(&lab));
    toggle_pause(&mut lab);
    run_frames(&mut lab, rate, rate * 4);

    let rigid = &lab.sandbox.physics.bodies[handle];
    assert!((rigid.translation().y - 0.48).abs() < 0.03);
    assert!(rigid.linvel().length() < 0.05);
    assert!(lab.sandbox.physics.sleep_state(handle).unwrap().sleeping);
    assert!(lab.sandbox.physics.contacts().any(|pair| {
        !pair.sensor
            && [pair.collider1, pair.collider2]
                .iter()
                .any(|&collider| lab.sandbox.physics.colliders[collider].parent() == Some(handle))
    }));
    let at_rest = state(&lab);
    let query = lab.sandbox.inspect(rigid.translation()).unwrap();
    assert_eq!(query.body(&lab.sandbox), Some(handle));
    lab.select_body(Some(&query));
    assert!(lab.selected_label().contains("BOX ASLEEP"));
    assert_eq!(
        state(&lab),
        at_rest,
        "inspection cannot wake a settled crate"
    );
    checkpoints.push(at_rest);

    // The real pulse wakes this naturally sleeping body. A second request inside
    // the cooldown must not restart it or multiply the impulse.
    let frame = lab.update(
        0.,
        LabInput {
            pulse: Some(vec2(10.8, 0.)),
            ..Default::default()
        },
    );
    assert_eq!(frame.steps, 0);
    assert!(!lab.pulsed);
    run_frames(&mut lab, rate, rate / 2);
    let rigid = &lab.sandbox.physics.bodies[handle];
    assert!(rigid.translation().y > 2.);
    assert!(!lab.sandbox.physics.sleep_state(handle).unwrap().sleeping);
    assert!(lab.sandbox.pulse_time > 0.);
    checkpoints.push(state(&lab));
    lab.update(
        0.,
        LabInput {
            pulse: Some(vec2(10.8, 0.)),
            ..Default::default()
        },
    );
    lab.update(1. / TICK_HZ, LabInput::default());
    assert!(!lab.pulsed);
    assert_eq!(lab.feedback, "Pulse recharging.");
    toggle_pause(&mut lab);
    for _ in 0..24 {
        lab.update(
            10.,
            LabInput {
                single_step: true,
                ..Default::default()
            },
        );
    }
    assert_eq!(lab.sandbox.pulse_time, 0.);
    let paused = state(&lab);
    lab.update(
        60.,
        LabInput {
            pulse: Some(vec2(10.8, 0.)),
            ..Default::default()
        },
    );
    assert!(!lab.pulsed);
    assert_eq!(state(&lab), paused);
    checkpoints.push(paused);

    // Reset beats conflicting same-frame input, preserves crate/manual pause,
    // and really rebuilds the scene rather than only resetting its tick counter.
    let frame = lab.update(
        60.,
        LabInput {
            reset: true,
            toggle_pause: true,
            single_step: true,
            ball: Some(true),
            drop: Some((vec2(0., 10.), false)),
            pulse: Some(vec2(0., 1.)),
        },
    );
    assert_eq!(frame.steps, 0);
    let mut fresh = Lab::new();
    fresh.update(
        0.,
        LabInput {
            toggle_pause: true,
            ball: Some(false),
            ..Default::default()
        },
    );
    assert_eq!(state(&lab), state(&fresh));
    assert!(lab.pending.drop.is_none() && lab.pending.pulse.is_none());
    checkpoints.push(state(&lab));
    toggle_pause(&mut lab);
    toggle_pause(&mut fresh);
    run_frames(&mut lab, rate, rate / 2);
    run_frames(&mut fresh, 120, 60);
    assert_eq!(state(&lab), state(&fresh));
    checkpoints.push(state(&lab));
    checkpoints
}

#[test]
fn workshop_settle_inspect_pulse_step_and_reset_match_at_30_60_144_hz() {
    let expected = workshop(120);
    for rate in [30, 60, 144] {
        assert_eq!(workshop(rate), expected, "workshop supplied at {rate} Hz");
    }
}

fn input_frame(lab: &mut Lab, input: &mut PlayerInput, elapsed: f64) -> (FrameStep, Vec<Action>) {
    host_frame(elapsed as f32, 800, 600, || {
        input.sample(true);
        let single_step = input.pressed("step");
        let mut consumed = Vec::new();
        let frame = lab.update_with_actions(
            elapsed,
            LabInput {
                single_step,
                ..Default::default()
            },
            || {
                let [drop] = input.take_tick(["drop"]);
                consumed.push(drop);
                LabCommands {
                    drop: drop.pressed.then_some((vec2(0., 10.), false)),
                    ..Default::default()
                }
            },
        );
        (frame, consumed)
    })
}

fn pad(button: usize, value: f32) {
    let mut sample = PadState {
        connected: true,
        ..Default::default()
    };
    sample.buttons[button] = value;
    assert!(gamepad_sample(0, sample));
}

#[test]
fn real_player_tap_survives_zero_tick_frames_and_hitch_without_repeating() {
    let mut lab = Lab::new();
    let mut reference = Lab::new();
    let mut input = PlayerInput::transient("journey200-172-taps");
    register_lab_actions(&mut input);
    pad(7, 0.); // Establish a neutral connected pad before sending a tap.
    input_frame(&mut lab, &mut input, 0.);
    pad(7, 1.);
    pad(7, 0.); // Press AND release before the next render sample.
    let before = state(&lab);
    for _ in 0..2 {
        let (frame, consumed) = input_frame(&mut lab, &mut input, 0.125 / TICK_HZ);
        assert_eq!(frame.steps, 0);
        assert!(consumed.is_empty());
        assert_eq!(state(&lab), before);
    }
    let (frame, consumed) = input_frame(&mut lab, &mut input, 1.);
    assert_eq!(frame.steps, 8);
    assert!(frame.dropped_seconds > 0.9);
    assert_eq!(consumed.len(), 8);
    assert!(consumed[0].pressed && consumed[0].released);
    assert_eq!(consumed[0].value, 0.);
    assert!(
        consumed[1..]
            .iter()
            .all(|action| !action.pressed && !action.released)
    );
    reference.update(
        8. / TICK_HZ,
        LabInput {
            drop: Some((vec2(0., 10.), false)),
            ..Default::default()
        },
    );
    assert_eq!(state(&lab), state(&reference));
    assert_eq!(lab.sandbox.bodies.len(), 35);
    assert_eq!(input_frame(&mut lab, &mut input, 0.).0.steps, 0);
    // Only the fractional remainder survives the catch-up cap.
    assert_eq!(input_frame(&mut lab, &mut input, 0.75 / TICK_HZ).0.steps, 1);
    reference.update(1. / TICK_HZ, LabInput::default());
    assert_eq!(state(&lab), state(&reference));

    pad(7, 1.);
    input_frame(&mut lab, &mut input, 4. / TICK_HZ);
    assert_eq!(lab.sandbox.bodies.len(), 36);
    let (_, held) = input_frame(&mut lab, &mut input, 4. / TICK_HZ);
    assert!(
        held.iter()
            .all(|action| action.value == 1. && !action.pressed)
    );
    assert_eq!(lab.sandbox.bodies.len(), 36);
    pad(7, 0.);
    input_frame(&mut lab, &mut input, 1. / TICK_HZ);
    pad(7, 1.);
    pad(7, 0.);
    input_frame(&mut lab, &mut input, 1. / TICK_HZ);
    assert_eq!(lab.sandbox.bodies.len(), 37, "release re-arms the next tap");
    assert!(gamepad_sample(0, PadState::default()));
}

#[test]
fn actual_step_button_hold_does_not_repeat_and_manual_physics_matches_automatic() {
    let mut lab = Lab::new();
    let mut reference = Lab::new();
    let mut input = PlayerInput::transient("journey200-172-step");
    register_lab_actions(&mut input);
    pad(5, 0.);
    input_frame(&mut lab, &mut input, 0.);
    toggle_pause(&mut lab);
    // The production drop binding places immediately even though no tick runs.
    pad(7, 1.);
    pad(7, 0.);
    assert_eq!(input_frame(&mut lab, &mut input, 300.).0.steps, 0);
    assert_eq!(lab.sandbox.bodies.len(), 35);
    toggle_pause(&mut reference);
    reference.update(
        0.,
        LabInput {
            drop: Some((vec2(0., 10.), false)),
            ..Default::default()
        },
    );
    assert_eq!(state(&lab), state(&reference));
    toggle_pause(&mut reference);

    for tick in 1..=16 {
        pad(5, 1.);
        let (frame, _) = input_frame(&mut lab, &mut input, 300.);
        assert_eq!((frame.steps, frame.alpha), (1, 1.));
        reference.update(1. / TICK_HZ, LabInput::default());
        assert_eq!(lab.session.tick(), tick);
        // Bring the automatic reference to the same lifecycle state for an
        // exact comparison, using the real pause path without advancing time.
        toggle_pause(&mut reference);
        assert_eq!(state(&lab), state(&reference));
        toggle_pause(&mut reference);
        let before = state(&lab);
        for _ in 0..3 {
            assert_eq!(input_frame(&mut lab, &mut input, 10.).0.steps, 0);
            assert_eq!(state(&lab), before);
        }
        pad(5, 0.);
        assert_eq!(input_frame(&mut lab, &mut input, 10.).0.steps, 0);
    }
    toggle_pause(&mut lab);
    run_frames(&mut lab, 60, 15);
    run_frames(&mut reference, 120, 30);
    assert_eq!(state(&lab), state(&reference));
    assert!(gamepad_sample(0, PadState::default()));
}

#[test]
fn filled_editor_rejects_extra_or_invalid_drops_then_reset_can_replay_cleanly() {
    let mut lab = Lab::new();
    toggle_pause(&mut lab);
    for index in 34..MAX_BODIES {
        let ball = index % 2 == 0;
        lab.update(
            0.,
            LabInput {
                ball: Some(ball),
                drop: Some((
                    vec2(
                        if ball { -100. } else { 100. },
                        if ball { -5. } else { 100. },
                    ),
                    false,
                )),
                ..Default::default()
            },
        );
        let body = lab.sandbox.bodies.last().unwrap();
        assert_eq!(
            body.previous_position,
            if ball {
                vec2(-11.2, 0.6)
            } else {
                vec2(11.2, 12.4)
            }
        );
        assert_eq!(lab.selected, Some(body.handle));
        assert_eq!(lab.session.tick(), 0);
    }
    assert_eq!(lab.sandbox.bodies.len(), 240);
    assert_eq!(lab.sandbox.physics.bodies.len(), 248);
    assert!(!lab.button_enabled(2));
    let full = state(&lab);
    for point in [vec2(0., 10.), vec2(f32::NAN, 1.), vec2(0., f32::INFINITY)] {
        lab.update(
            90.,
            LabInput {
                drop: Some((point, false)),
                ..Default::default()
            },
        );
        assert_eq!(
            state(&lab),
            full,
            "rejection must preserve the selected body and world"
        );
        assert!(lab.pending.drop.is_none());
    }
    lab.update(
        0.,
        LabInput {
            reset: true,
            ..Default::default()
        },
    );
    assert!(lab.button_enabled(2));
    assert!(!lab.ball && lab.session.is_paused());
    assert!(lab.selected.is_none());
    let mut fresh = Lab::new();
    fresh.update(
        0.,
        LabInput {
            ball: Some(false),
            toggle_pause: true,
            ..Default::default()
        },
    );
    assert_eq!(state(&lab), state(&fresh));
    for candidate in [&mut lab, &mut fresh] {
        candidate.update(
            0.,
            LabInput {
                drop: Some((vec2(0., 8.), false)),
                ..Default::default()
            },
        );
        toggle_pause(candidate);
        candidate.update(
            0.,
            LabInput {
                pulse: Some(vec2(0., 1.)),
                ..Default::default()
            },
        );
        run_frames(candidate, 60, 60);
    }
    assert_eq!(state(&lab), state(&fresh));
    assert_eq!(lab.sandbox.bodies.len(), 35);
}
