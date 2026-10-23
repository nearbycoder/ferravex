//! Task 173: scenarios through the real lab command/step path, without drawing.
//!
//! Body comparisons are exact on this native build: every elapsed-time partition
//! must execute the same ordered f32 physics ticks. Only clock interpolation and
//! geometric query assertions use tolerances. This is not a cross-platform claim.

use super::*;
use ferravex_core::session::SessionState;
use ferravex_physics::{ColliderHandle, SleepStats};
use ferravex_render::input::{PadState, gamepad_sample, host_frame};

const TICK: f64 = 1. / 120.;

#[derive(Debug, PartialEq)]
struct BodyState {
    handle: RigidBodyHandle,
    ball: bool,
    compound: bool,
    position: Vec3,
    rotation: Quat,
    velocity: Vec3,
    angular_velocity: Vec3,
    sleeping: bool,
}

#[derive(Debug, PartialEq)]
struct PhysicsState {
    bodies: Vec<BodyState>,
    population: (usize, usize, usize),
    contacts: Vec<(ColliderHandle, ColliderHandle, [f32; 3], bool)>,
    events: Vec<ContactEvent>,
    sleep: SleepStats,
    sequence: u32,
    pulse_time: f32,
    pulse_center: Vec3,
    impact: Option<(f32, Surface)>,
    impact_cooldown: f32,
}

fn physics_state(lab: &Lab) -> PhysicsState {
    let sandbox = &lab.sandbox;
    PhysicsState {
        bodies: sandbox
            .bodies
            .iter()
            .map(|body| {
                let rigid = &sandbox.physics.bodies[body.handle];
                BodyState {
                    handle: body.handle,
                    ball: body.ball,
                    compound: body.compound,
                    position: rigid.translation(),
                    rotation: *rigid.rotation(),
                    velocity: rigid.linvel(),
                    angular_velocity: rigid.angvel(),
                    sleeping: rigid.is_sleeping(),
                }
            })
            .collect(),
        population: (
            sandbox.physics.bodies.len(),
            sandbox.physics.colliders.len(),
            sandbox.physics.joints.len(),
        ),
        contacts: sandbox
            .physics
            .contacts()
            .map(|pair| (pair.collider1, pair.collider2, pair.normal, pair.sensor))
            .collect(),
        events: sandbox.physics.contact_events().to_vec(),
        sleep: sandbox.physics.sleep_stats(),
        sequence: sandbox.sequence,
        pulse_time: sandbox.pulse_time,
        pulse_center: sandbox.pulse_center,
        impact: sandbox.impact,
        impact_cooldown: lab.impact_cooldown,
    }
}

fn history(lab: &Lab) -> Vec<(Vec3, Quat)> {
    lab.sandbox
        .bodies
        .iter()
        .map(|body| (body.previous_position, body.previous_rotation))
        .collect()
}

fn assert_snapped(lab: &Lab) {
    for body in &lab.sandbox.bodies {
        let rigid = &lab.sandbox.physics.bodies[body.handle];
        assert_eq!(body.previous_position, rigid.translation());
        assert_eq!(body.previous_rotation, *rigid.rotation());
    }
}

fn assert_alpha(frame: &LabFrame, expected: f32) {
    // Alpha is f32 after accumulating f64 elapsed partitions. One millionth of
    // a tick allows representation rounding, not a missing simulation step.
    assert!(
        (frame.alpha - expected).abs() < 1e-6,
        "alpha {} != {expected}",
        frame.alpha
    );
}

fn run_seconds(lab: &mut Lab, hz: u32, seconds: u32) {
    let start = lab.session.tick();
    for _ in 0..hz * seconds {
        let frame = lab.advance(1. / f64::from(hz), Commands::default());
        assert!(!frame.pulse && !frame.reset);
    }
    assert_eq!(lab.session.tick(), start + u64::from(seconds) * 120);
    assert_eq!(lab.dropped_seconds, 0.);
}

#[test]
fn scripted_yard_matches_at_30_60_and_144_hz_through_pause_steps_pulse_and_restart() {
    let mut labs = [Lab::new(), Lab::new(), Lab::new()];
    let rates = [30, 60, 144];
    for (lab, hz) in labs.iter_mut().zip(rates) {
        assert_eq!(lab.session.state(), SessionState::Playing);
        assert_eq!(lab.session.tick(), 0);
        lab.advance(
            0.,
            Commands {
                ball_drop: true,
                ..Default::default()
            },
        );
        run_seconds(lab, hz, 1);
        assert_eq!(lab.sandbox.bodies.len(), 40);
    }
    for lab in &labs[1..] {
        assert_eq!(physics_state(lab), physics_state(&labs[0]));
        assert_eq!(history(lab), history(&labs[0]));
    }

    for (lab, hz) in labs.iter_mut().zip(rates) {
        assert_alpha(&lab.advance(0.375 * TICK, Commands::default()), 0.375);
        lab.advance(
            0.,
            Commands {
                pause: true,
                crate_drop: true,
                ..Default::default()
            },
        );
        let paused = physics_state(lab);
        for _ in 0..hz * 2 {
            let frame = lab.advance(1. / f64::from(hz), Commands::default());
            assert_alpha(&frame, 1.);
            assert!(!frame.pulse && frame.impact.is_none());
        }
        assert_eq!(physics_state(lab), paused);
        assert_eq!(lab.session.tick(), 120);
        for tick in 121..=129 {
            lab.advance(
                600.,
                Commands {
                    step: true,
                    ..Default::default()
                },
            );
            assert_eq!(lab.session.state(), SessionState::Paused);
            assert_eq!(lab.session.tick(), tick);
            assert_snapped(lab);
        }
        let inspected = physics_state(lab);
        let resumed = lab.advance(
            0.,
            Commands {
                pause: true,
                ..Default::default()
            },
        );
        assert_alpha(&resumed, 0.375);
        assert_eq!(lab.session.state(), SessionState::Playing);
        assert_eq!(physics_state(lab), inspected);
        assert_snapped(lab);

        // An immediate inspector drop and a queued pulse share a zero-tick
        // frame. Only the drop edits the world before the next actual tick.
        let frame = lab.advance(
            0.,
            Commands {
                drop: true,
                pulse: true,
                ..Default::default()
            },
        );
        assert!(!frame.pulse);
        assert_eq!(lab.sandbox.bodies.len(), 42);
        let frame = lab.advance(0.625 * TICK, Commands::default());
        assert!(frame.pulse);
        assert_alpha(&frame, 0.);
        assert_eq!(lab.session.tick(), 130);
        run_seconds(lab, hz, 1);
        assert_eq!(lab.session.tick(), 250);
        assert_eq!(lab.sandbox.pulse_time, 0.);
    }
    for lab in &labs[1..] {
        assert_eq!(physics_state(lab), physics_state(&labs[0]));
        assert_eq!(history(lab), history(&labs[0]));
    }

    let fresh = Lab::new();
    for (lab, hz) in labs.iter_mut().zip(rates) {
        let frame = lab.advance(
            17.,
            Commands {
                reset: true,
                ..Default::default()
            },
        );
        assert!(frame.reset);
        assert_eq!(lab.session.tick(), 0);
        assert_eq!(lab.session.state(), SessionState::Playing);
        assert_eq!(physics_state(lab), physics_state(&fresh));
        assert_snapped(lab);
        lab.advance(
            0.,
            Commands {
                ball_drop: true,
                ..Default::default()
            },
        );
        run_seconds(lab, hz, 1);
    }
    let mut replayed = Lab::new();
    replayed.advance(
        0.,
        Commands {
            ball_drop: true,
            ..Default::default()
        },
    );
    run_seconds(&mut replayed, 120, 1);
    for lab in &labs {
        assert_eq!(physics_state(lab), physics_state(&replayed));
        assert_eq!(history(lab), history(&replayed));
    }
}

#[test]
fn overload_and_many_manual_steps_preserve_remainder_and_execute_only_counted_ticks() {
    let mut lab = Lab::new();
    let mut reference = Lab::new();
    for yard in [&mut lab, &mut reference] {
        yard.advance(
            0.,
            Commands {
                ball_drop: true,
                ..Default::default()
            },
        );
    }
    let frame = lab.advance(20.25 * TICK, Commands::default());
    assert_eq!(lab.session.tick(), 8);
    assert_alpha(&frame, 0.25);
    assert!((lab.dropped_seconds - 12. * TICK).abs() < 1e-12);
    for _ in 0..8 {
        reference.advance(TICK, Commands::default());
    }
    assert_eq!(physics_state(&lab), physics_state(&reference));
    lab.advance(
        3600.,
        Commands {
            pause: true,
            ..Default::default()
        },
    );
    let dropped = lab.dropped_seconds;
    for tick in 9..=19 {
        let before = physics_state(&lab);
        lab.advance(3600., Commands::default());
        assert_eq!(physics_state(&lab), before);
        let frame = lab.advance(
            3600.,
            Commands {
                step: true,
                ..Default::default()
            },
        );
        reference.advance(TICK, Commands::default());
        assert_eq!(lab.session.tick(), tick);
        assert_eq!(lab.session.state(), SessionState::Paused);
        assert_alpha(&frame, 1.);
        assert_snapped(&lab);
        assert_eq!(physics_state(&lab), physics_state(&reference));
        assert_eq!(lab.dropped_seconds, dropped);
    }
    let frame = lab.advance(
        0.5 * TICK,
        Commands {
            pause: true,
            step: true,
            ..Default::default()
        },
    );
    assert_eq!(
        lab.session.tick(),
        19,
        "STEP while resuming must not add a tick"
    );
    assert_alpha(&frame, 0.75);
    assert_snapped(&lab);
    let frame = lab.advance(0.25 * TICK, Commands::default());
    reference.advance(TICK, Commands::default());
    assert_eq!(lab.session.tick(), 20);
    assert_alpha(&frame, 0.);
    assert_eq!(physics_state(&lab), physics_state(&reference));
    assert_eq!(history(&lab), history(&reference));
    assert_eq!(lab.dropped_seconds, dropped);
}

fn ball_probe(lab: &Lab, handle: RigidBodyHandle, eye: Vec3) -> RayHit {
    let before = physics_state(lab);
    let prior_poses = history(lab);
    let tick = lab.session.tick();
    let center = lab.sandbox.physics.bodies[handle].translation();
    let mut inspector = Inspector::new();
    inspector.sample(&lab.sandbox.physics, eye, center);
    assert!(inspector.query.written > 0 && inspector.query.written <= 8);
    assert_eq!(inspector.query.written, inspector.query.total.min(8));
    let hit = inspector.hits[0].expect("the dropped ball is directly below the probe");
    assert_eq!(hit.body, Some(handle));
    // The authored sphere has radius 0.58 m. 0.1 mm allows f32 ray arithmetic.
    assert!((hit.distance - (eye.distance(center) - 0.58)).abs() < 1e-4);
    assert_eq!(physics_state(lab), before, "queries must not simulate");
    assert_eq!(history(lab), prior_poses);
    assert_eq!(lab.session.tick(), tick);
    hit
}

#[test]
fn dropped_ball_lands_and_queries_current_poses_while_pulse_wakes_a_settled_crate() {
    let mut lab = Lab::new();
    let crate_handle = lab.sandbox.bodies[0].handle;
    lab.advance(
        0.,
        Commands {
            pause: true,
            ball_drop: true,
            ..Default::default()
        },
    );
    let ball = lab.sandbox.bodies.last().unwrap().handle;
    let eye = lab.sandbox.physics.bodies[ball].translation() + Vec3::Y * 2.;
    let initial_hit = ball_probe(&lab, ball, eye);
    lab.advance(
        60.,
        Commands {
            step: true,
            ..Default::default()
        },
    );
    let stepped_hit = ball_probe(&lab, ball, eye);
    assert!(stepped_hit.distance > initial_hit.distance + 1e-4);
    assert_eq!(lab.session.tick(), 1);
    assert_snapped(&lab);
    let frame = lab.advance(
        2. * TICK,
        Commands {
            pause: true,
            ..Default::default()
        },
    );
    assert_alpha(&frame, 0.);
    let body = lab.sandbox.bodies.last().unwrap();
    let position = lab.sandbox.physics.bodies[ball].translation();
    assert!(body.previous_position.y - position.y > 1e-4);
    let falling_hit = ball_probe(&lab, ball, eye);
    let previous_distance = eye.distance(body.previous_position) - 0.58;
    assert!((falling_hit.distance - previous_distance).abs() > 1e-4);

    let mut contacted = false;
    let mut audible_contact = false;
    for _ in 0..960 {
        let frame = lab.advance(TICK, Commands::default());
        for event in lab.sandbox.physics.contact_events() {
            if let ContactEvent::Started(a, b) = *event {
                let a = lab.sandbox.physics.colliders[a].parent();
                let b = lab.sandbox.physics.colliders[b].parent();
                contacted |= a == Some(ball) || b == Some(ball);
            }
        }
        if let Some((strength, _)) = frame.impact {
            assert!(strength.is_finite() && strength > 1.2);
            assert_eq!(frame.impact, lab.sandbox.impact);
            assert!(lab.sandbox.physics.contact_events().iter().any(|event| {
                matches!(*event, ContactEvent::Started(a, b)
                    if lab.sandbox.physics.contact_pair(a, b).is_some_and(|pair| !pair.sensor))
            }));
            audible_contact = true;
        }
        let rigid = &lab.sandbox.physics.bodies[ball];
        assert!(rigid.translation().is_finite() && rigid.translation().y > 0.3);
        assert!(rigid.linvel().is_finite() && rigid.angvel().is_finite());
    }
    assert!(contacted, "the dropped ball must reach a real collider");
    assert!(
        audible_contact,
        "falling objects must produce accepted contact feedback"
    );
    let rigid = &lab.sandbox.physics.bodies[ball];
    // The ball can still roll laterally. Test its real floor support, and use
    // the original bottom crate for the natural sleep/wake outcome instead.
    // 2 cm permits contact slop, not penetration through the floor.
    assert!((rigid.translation().y - 0.58).abs() < 0.02);
    assert!(rigid.linvel().y.abs() < 0.02);
    let landed_center = rigid.translation();
    let rigid = &lab.sandbox.physics.bodies[crate_handle];
    assert!(
        rigid.is_sleeping(),
        "the original bottom crate settles naturally"
    );
    assert!((rigid.translation().y - 0.58).abs() < 0.02);
    assert_eq!(rigid.linvel(), Vec3::ZERO);
    assert_eq!(rigid.angvel(), Vec3::ZERO);
    let settled_center = rigid.translation();
    lab.advance(
        0.,
        Commands {
            pause: true,
            ..Default::default()
        },
    );
    let settled = physics_state(&lab);
    let hit = ball_probe(&lab, ball, landed_center + Vec3::Y * 2.);
    assert!((hit.distance - 1.42).abs() < 1e-4);
    lab.advance(600., Commands::default());
    assert_eq!(physics_state(&lab), settled);
    let frame = lab.advance(
        TICK,
        Commands {
            pause: true,
            pulse: true,
            ..Default::default()
        },
    );
    assert!(frame.pulse);
    let rigid = &lab.sandbox.physics.bodies[crate_handle];
    assert!(!rigid.is_sleeping());
    assert!(rigid.linvel().y > 0.1);
    assert!(rigid.translation().y > settled_center.y);
}

fn input_frame(lab: &mut Lab, input: &mut PlayerInput, elapsed: f64) -> (LabFrame, usize) {
    host_frame(elapsed as f32, 640, 480, || {
        input.sample(true);
        let mut reads = 0;
        let frame = lab.advance_with_input(elapsed, Commands::default(), || {
            reads += 1;
            input.take_tick(["pulse"])[0].pressed
        });
        (frame, reads)
    })
}

fn set_pulse_button(pad: &mut PadState, down: bool) {
    pad.buttons[4] = f32::from(down);
    assert!(gamepad_sample(0, *pad));
}

#[test]
fn real_controller_taps_coalesce_and_rejected_or_held_pulses_do_not_retry_after_recharge() {
    let mut input = PlayerInput::transient("journey200-173-pulse");
    input
        .action(
            "pulse",
            "Impulse pulse",
            ActionKind::Button,
            vec![Binding::digital(InputCode::PadButton(4))],
        )
        .unwrap();
    let mut pad = PadState {
        connected: true,
        ..Default::default()
    };
    assert!(gamepad_sample(0, pad));
    let mut lab = Lab::new();
    let mut reference = Lab::new();
    input_frame(&mut lab, &mut input, 0.);
    for _ in 0..2 {
        set_pulse_button(&mut pad, true);
        set_pulse_button(&mut pad, false);
        let (frame, reads) = input_frame(&mut lab, &mut input, 0.);
        assert_eq!(reads, 0);
        assert!(!frame.pulse);
        assert_eq!(physics_state(&lab), physics_state(&reference));
    }
    let (frame, reads) = input_frame(&mut lab, &mut input, 8. * TICK);
    assert!(frame.pulse);
    assert_eq!(reads, 8);
    reference.advance(
        TICK,
        Commands {
            pulse: true,
            ..Default::default()
        },
    );
    for _ in 1..8 {
        reference.advance(TICK, Commands::default());
    }
    assert_eq!(physics_state(&lab), physics_state(&reference));
    for _ in 0..8 {
        assert!(!input_frame(&mut lab, &mut input, 8. * TICK).0.pulse);
        reference.advance(8. * TICK, Commands::default());
    }
    assert_eq!(lab.session.tick(), 72);
    assert!(lab.sandbox.pulse_time > 0. && lab.sandbox.pulse_time < 8. / 120.);
    set_pulse_button(&mut pad, true);
    assert_eq!(input_frame(&mut lab, &mut input, 0.).1, 0);
    let (frame, reads) = input_frame(&mut lab, &mut input, 8. * TICK);
    reference.advance(8. * TICK, Commands::default());
    assert_eq!(reads, 8);
    assert!(
        !frame.pulse,
        "rejected press must not retry later in the same batch"
    );
    assert_eq!(lab.sandbox.pulse_time, 0.);
    assert_eq!(physics_state(&lab), physics_state(&reference));
    for _ in 0..10 {
        assert!(!input_frame(&mut lab, &mut input, 8. * TICK).0.pulse);
        reference.advance(8. * TICK, Commands::default());
    }
    assert_eq!(lab.session.tick(), 160);
    assert_eq!(physics_state(&lab), physics_state(&reference));
    set_pulse_button(&mut pad, false);
    input_frame(&mut lab, &mut input, 0.);
    set_pulse_button(&mut pad, true);
    set_pulse_button(&mut pad, false);
    let (frame, reads) = input_frame(&mut lab, &mut input, TICK);
    assert!(frame.pulse);
    assert_eq!(reads, 1);
    reference.advance(
        TICK,
        Commands {
            pulse: true,
            ..Default::default()
        },
    );
    assert_eq!(physics_state(&lab), physics_state(&reference));
    assert!(gamepad_sample(0, PadState::default()));
}

#[test]
fn capacity_rejection_is_atomic_and_reset_restores_drop_sequence_and_available_space() {
    let mut lab = Lab::new();
    let fresh = Lab::new();
    let initial_count = lab.sandbox.bodies.len();
    let initial_physics = physics_state(&lab);
    assert_alpha(&lab.advance(0.375 * TICK, Commands::default()), 0.375);
    lab.advance(
        0.,
        Commands {
            pause: true,
            ..Default::default()
        },
    );
    for i in initial_count..MAX_BODIES {
        lab.advance(
            0.,
            Commands {
                ball_drop: i % 2 == 0,
                crate_drop: i % 2 != 0,
                drop: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.sandbox.bodies.len(), i + 1);
    }
    let additions = MAX_BODIES - initial_count;
    let full = physics_state(&lab);
    assert_eq!(full.population.0, initial_physics.population.0 + additions);
    assert_eq!(full.population.1, initial_physics.population.1 + additions);
    assert_eq!(full.sequence as usize, additions);
    assert_eq!(
        full.sleep.awake_bodies + full.sleep.sleeping_bodies,
        MAX_BODIES
    );
    for _ in 0..3 {
        lab.advance(
            3600.,
            Commands {
                drop: true,
                pulse: true,
                ..Default::default()
            },
        );
        assert_eq!(physics_state(&lab), full);
        assert_eq!(lab.session.tick(), 0);
        assert!(!lab.pending_pulse);
    }
    lab.advance(
        0.,
        Commands {
            drop: true,
            ..Default::default()
        },
    );
    assert!(lab.feedback.starts_with("Body limit reached"));
    lab.advance(
        0.,
        Commands {
            step: true,
            ..Default::default()
        },
    );
    assert_eq!(lab.session.tick(), 1);
    lab.advance(
        0.,
        Commands {
            pause: true,
            pulse: true,
            ..Default::default()
        },
    );
    assert!(lab.pending_pulse);
    let frame = lab.advance(
        20.,
        Commands {
            reset: true,
            pause: true,
            step: true,
            ball_drop: true,
            pulse: true,
            ..Default::default()
        },
    );
    assert!(frame.reset && !frame.pulse && frame.impact.is_none());
    assert_eq!(lab.session.state(), SessionState::Playing);
    assert_eq!(lab.session.tick(), 0);
    assert_eq!(lab.shape, DropShape::Crate);
    assert_eq!(lab.dropped_seconds, 0.);
    assert!(!lab.pending_pulse);
    assert_eq!(physics_state(&lab), physics_state(&fresh));
    assert_snapped(&lab);

    let mut replayed = Lab::new();
    for ball in [true, false, true] {
        for yard in [&mut lab, &mut replayed] {
            yard.advance(
                0.,
                Commands {
                    ball_drop: ball,
                    crate_drop: !ball,
                    ..Default::default()
                },
            );
        }
        assert_eq!(physics_state(&lab), physics_state(&replayed));
    }
    assert_alpha(&lab.advance(0.5 * TICK, Commands::default()), 0.5);
    assert_eq!(lab.session.tick(), 0);
    assert!(!lab.advance(0.5 * TICK, Commands::default()).pulse);
    replayed.advance(TICK, Commands::default());
    assert_eq!(physics_state(&lab), physics_state(&replayed));
}
