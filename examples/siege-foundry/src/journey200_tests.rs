//! End-to-end headless scenarios for Journey200 task 169.
//!
//! Only normal lifecycle operations and tick inputs change the game. Snapshots
//! inspect real bodies, interpolation history and accepted events; no scenario
//! assigns ammunition, target flags, timers, poses or a result.

use super::*;

const TICK: f64 = 1.0 / 120.0;

#[derive(Debug, PartialEq)]
struct BodyState {
    position: [f32; 3],
    rotation: [f32; 4],
    velocity: [f32; 3],
    angular_velocity: [f32; 3],
    sleeping: bool,
}

#[derive(Debug, PartialEq)]
struct RangeState {
    phase: Phase,
    ammo: u8,
    aim: (f32, f32),
    timers: (f32, f32, f32),
    targets: Vec<bool>,
    bodies: Vec<BodyState>,
    previous_blocks: Vec<(Vec3, Quat)>,
    previous_shots: Vec<Vec3>,
    colliders: usize,
}

fn snapshot(foundry: &Foundry) -> RangeState {
    let game = &foundry.range;
    RangeState {
        phase: game.phase,
        ammo: game.ammo,
        aim: (game.yaw, game.elevation),
        timers: (game.elapsed, game.cooldown, game.since_shot),
        targets: game.blocks.iter().map(|block| block.down).collect(),
        bodies: game
            .physics
            .bodies
            .iter()
            .map(|(_, body)| BodyState {
                position: body.translation().to_array(),
                rotation: body.rotation().to_array(),
                velocity: body.linvel().to_array(),
                angular_velocity: body.angvel().to_array(),
                sleeping: body.is_sleeping(),
            })
            .collect(),
        previous_blocks: game
            .blocks
            .iter()
            .map(|block| (block.previous_position, block.previous_rotation))
            .collect(),
        previous_shots: game
            .shots
            .iter()
            .map(|shot| shot.previous_position)
            .collect(),
        colliders: game.physics.colliders.len(),
    }
}

fn feedback(foundry: &Foundry) -> (f32, f32, usize, f32, f32) {
    let f = &foundry.feedback;
    (f.shot, f.destruction, f.knocked_down, f.rebuilt, f.recoil)
}

#[derive(Debug, Default, PartialEq)]
struct Events {
    shots: usize,
    targets: usize,
    outcomes: Vec<Phase>,
}

impl Events {
    fn record(&mut self, events: FrameFeedback) {
        self.shots += usize::from(events.fired);
        self.targets += events.destroyed;
        self.outcomes.extend(events.outcome);
    }
}

#[derive(Debug, PartialEq)]
struct Checkpoint {
    executed_ticks: u64,
    range: RangeState,
    events: Events,
}

// Match the actual host's f32 Demo::dt -> f64 conversion. Each second has
// exactly fps supplied frames; no sleep or wall-clock measurement is involved.
fn frame_time(fps: u32) -> f64 {
    f64::from(1.0 / fps as f32)
}

fn autoplay_trace(foundry: &mut Foundry, fps: u32) -> Vec<Checkpoint> {
    let mut executed_ticks = 0;
    let mut checkpoints = Vec::new();
    for second in 1..=20 {
        let mut events = Events::default();
        for _ in 0..fps {
            events.record(foundry.advance_input(frame_time(fps), true, || {
                executed_ticks += 1;
                TickInput::default()
            }));
        }
        if foundry.session.is_playing() {
            assert_eq!(executed_ticks, second * 120);
        }
        checkpoints.push(Checkpoint {
            executed_ticks,
            range: snapshot(foundry),
            events,
        });
    }
    assert_eq!(foundry.session.state(), SessionState::Won);
    assert_eq!(foundry.range.score(), GOAL);
    let shots = checkpoints.iter().map(|c| c.events.shots).sum::<usize>();
    assert_eq!(shots, foundry.range.shots.len());
    assert_eq!(shots, usize::from(AMMO - foundry.range.ammo));
    assert!(shots > 0 && shots < usize::from(AMMO));
    assert_eq!(
        checkpoints.iter().map(|c| c.events.targets).sum::<usize>(),
        GOAL
    );
    assert_eq!(
        checkpoints
            .iter()
            .flat_map(|c| c.events.outcomes.iter().copied())
            .collect::<Vec<_>>(),
        [Phase::Won]
    );
    // Session::tick counts the entire scheduled batch. The game's callback
    // count records only executed ticks, including the terminal early exit.
    assert!(foundry.session.tick() >= executed_ticks);
    assert!(foundry.session.tick() - executed_ticks < 4);
    checkpoints
}

#[test]
fn full_ballistic_campaign_matches_bodies_and_events_at_30_60_144_hz() {
    let mut reference = None;
    for fps in [30, 60, 144] {
        let mut foundry = Foundry::new(false);
        assert!(foundry.session.start());
        let trace = autoplay_trace(&mut foundry, fps);
        // Compare actual physics and feedback totals at every supplied second,
        // including immutable post-win seconds, rather than just asserting Won.
        if let Some(reference) = &reference {
            assert_eq!(&trace, reference, "{fps} Hz campaign diverged");
        }
        reference = Some(trace);
    }
}

#[test]
fn pause_in_flight_resume_and_rebuild_reproduce_a_real_victory() {
    let mut uninterrupted = Foundry::new(false);
    let mut foundry = Foundry::new(false);
    let initial = snapshot(&foundry);
    foundry.queue_fire();
    let ready = foundry.advance_input(60.0, true, || panic!("Ready consumed input"));
    assert!(!ready.fired && ready.destroyed == 0 && ready.outcome.is_none());
    assert_eq!(snapshot(&foundry), initial);
    assert!(foundry.session.start());
    assert!(uninterrupted.session.start());
    let started = foundry.advance_input(0.0, false, || panic!("Start consumed input"));
    assert!(!started.fired);

    let mut paused_in_flight = false;
    let mut paused_after_collapse = false;
    let mut outcomes = 0;
    for _ in 0..20 * 60 {
        let expected = uninterrupted.advance_input(frame_time(60), true, TickInput::default);
        let actual = foundry.advance_input(frame_time(60), true, TickInput::default);
        assert_eq!(
            (actual.fired, actual.destroyed, actual.outcome),
            (expected.fired, expected.destroyed, expected.outcome)
        );
        outcomes += usize::from(actual.outcome.is_some());
        let in_flight = !paused_in_flight && actual.fired;
        let collapsed = !paused_after_collapse && actual.destroyed > 0;
        if in_flight || collapsed {
            assert_eq!(foundry.range.phase, Phase::Aiming);
            let before = snapshot(&foundry);
            let feedback_before = feedback(&foundry);
            let tick_before = foundry.session.tick();
            foundry.set_paused(true);
            assert_eq!(foundry.session.state(), SessionState::Paused);
            foundry.queue_fire();
            for elapsed in [0.0, 0.001, 0.1, 60.0] {
                let events =
                    foundry.advance_input(elapsed, true, || panic!("Pause consumed input"));
                assert!(!events.fired && events.destroyed == 0 && events.outcome.is_none());
                assert_eq!(events.alpha, 1.0);
                assert_eq!(snapshot(&foundry), before);
                assert_eq!(feedback(&foundry), feedback_before);
                assert_eq!(foundry.session.tick(), tick_before);
            }
            foundry.set_paused(false);
            paused_in_flight |= in_flight;
            paused_after_collapse |= collapsed;
        }
        assert_eq!(snapshot(&foundry), snapshot(&uninterrupted));
        assert_eq!(feedback(&foundry), feedback(&uninterrupted));
    }
    assert!(paused_in_flight && paused_after_collapse);
    assert_eq!(outcomes, 1);
    assert_eq!(foundry.session.state(), SessionState::Won);
    assert_eq!(foundry.range.score(), GOAL);

    foundry.rebuild();
    assert_eq!(snapshot(&foundry), initial);
    assert_eq!(foundry.session.state(), SessionState::Playing);
    assert_eq!(foundry.session.tick(), 0);
    assert!(!foundry.pending_fire);
    assert_eq!(feedback(&foundry), (0.0, 0.0, 0, 1.8, 0.0));
    assert!(
        !foundry
            .advance_input(0.0, false, || panic!("Retry consumed input"))
            .fired
    );
    let replay = autoplay_trace(&mut foundry, 60);
    let fresh = autoplay_trace(&mut Foundry::new(true), 60);
    assert_eq!(replay, fresh, "Retry must rebuild real collision state too");
}

#[test]
fn manual_ammo_exhaustion_and_final_shot_settling_match_across_frame_rates() {
    let mut reference = None;
    for fps in [30, 60, 144] {
        let mut foundry = Foundry::new(false);
        assert!(foundry.session.start());
        let mut executed_ticks = 0_u64;
        let mut last_shot_tick = None;
        let mut last_shot_position = None;
        let mut checkpoints = Vec::new();
        let mut settling_frames = 0;
        for second in 1..=20 {
            let mut events = Events::default();
            for _ in 0..fps {
                let frame = foundry.advance_input(frame_time(fps), false, || {
                    executed_ticks += 1;
                    // Spend seven shots beside the towers, then aim back at
                    // the center tower and give the final ball real work.
                    let fire = (181..=901).contains(&executed_ticks)
                        && (executed_ticks - 181).is_multiple_of(120)
                        || executed_ticks == 1141;
                    if fire {
                        last_shot_tick = Some(executed_ticks);
                    }
                    TickInput {
                        fire,
                        aim: match executed_ticks {
                            1..=180 => Vec2::X,
                            902..=1081 => -Vec2::X,
                            _ => Vec2::ZERO,
                        },
                    }
                });
                if foundry.range.ammo == 0 && foundry.session.is_playing() {
                    assert!(frame.outcome.is_none());
                    assert!(foundry.range.since_shot <= SETTLE);
                    assert_eq!(foundry.range.phase, Phase::Aiming);
                    let shot = foundry.range.shots.last().unwrap();
                    let position = foundry.range.physics.bodies[shot.handle].translation();
                    if let Some((previous_tick, previous)) = last_shot_position
                        && executed_ticks > previous_tick
                        && foundry.range.since_shot < 0.5
                    {
                        assert_ne!(position, previous, "Final ball stopped before settling");
                    }
                    last_shot_position = Some((executed_ticks, position));
                    settling_frames += 1;
                }
                if frame.destroyed > 0 {
                    assert_eq!(foundry.range.ammo, 0, "Only the last shot aims at a tower");
                    assert!(foundry.range.since_shot < SETTLE);
                    assert!(frame.outcome.is_none(), "Two towers remain standing");
                }
                if frame.outcome.is_some() {
                    assert_eq!(frame.outcome, Some(Phase::Lost));
                    assert_eq!(frame.alpha, 1.0);
                    assert!(foundry.range.since_shot > SETTLE);
                    assert!(foundry.range.since_shot <= SETTLE + DT);
                    assert_eq!(executed_ticks - last_shot_tick.unwrap() + 1, 721);
                }
                events.record(frame);
            }
            if foundry.session.is_playing() {
                assert_eq!(executed_ticks, second * 120);
            }
            checkpoints.push(Checkpoint {
                executed_ticks,
                range: snapshot(&foundry),
                events,
            });
        }
        assert!(settling_frames > 5 * fps);
        assert_eq!(foundry.session.state(), SessionState::Lost);
        assert_eq!(foundry.range.score(), 2);
        assert_eq!(foundry.range.shots.len(), usize::from(AMMO));
        assert_eq!(foundry.range.ammo, 0);
        assert_eq!(
            checkpoints.iter().map(|c| c.events.shots).sum::<usize>(),
            usize::from(AMMO)
        );
        assert_eq!(
            checkpoints.iter().map(|c| c.events.targets).sum::<usize>(),
            2
        );
        assert_eq!(
            checkpoints
                .iter()
                .flat_map(|c| c.events.outcomes.iter().copied())
                .collect::<Vec<_>>(),
            [Phase::Lost]
        );
        if let Some(reference) = &reference {
            assert_eq!(&checkpoints, reference, "{fps} Hz loss diverged");
        }
        reference = Some(checkpoints);
        let terminal = snapshot(&foundry);
        foundry.queue_fire();
        foundry.set_paused(false);
        let events = foundry.advance_input(60.0, true, || panic!("Loss consumed input"));
        assert!(!events.fired && events.destroyed == 0 && events.outcome.is_none());
        assert_eq!(snapshot(&foundry), terminal);
        foundry.rebuild();
        assert_eq!(snapshot(&foundry), snapshot(&Foundry::new(true)));
    }
}

// The actual action journal receives press/release host samples. The reference
// receives the expected edge on one tick and always advances one tick per frame,
// so comparisons include collision consequences of zero/catch-up input timing.
struct TapScenario {
    foundry: Foundry,
    reference: Foundry,
    player: PlayerInput,
}

impl TapScenario {
    fn new() -> Self {
        use ferravex_render::input::{ActionKind, Binding, InputCode};
        let mut player = PlayerInput::transient("journey200-169-ballistics");
        player
            .action(
                "fire",
                "Fire",
                ActionKind::Button,
                vec![Binding::digital(InputCode::PadButton(7))],
            )
            .unwrap();
        Self {
            foundry: Foundry::new(true),
            reference: Foundry::new(true),
            player,
        }
    }

    fn set_paused(&mut self, paused: bool) {
        self.player.set_paused(paused);
        self.foundry.set_paused(paused);
        self.reference.set_paused(paused);
    }

    fn frame(&mut self, elapsed: f64, button: f32, expected_fire: bool) -> usize {
        use ferravex_render::input::{self, PadState};
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        pad.buttons[7] = button;
        assert!(input::gamepad_sample(0, pad));
        let mut calls = 0;
        let actual = input::host_frame(elapsed as f32, 320, 448, || {
            self.player.sample_play();
            self.foundry.advance_input(elapsed, false, || {
                calls += 1;
                TickInput {
                    fire: self.player.take_tick(["fire"])[0].pressed,
                    aim: Vec2::ZERO,
                }
            })
        });
        assert!(calls <= 12);
        let mut expected = Events::default();
        for tick in 0..calls {
            expected.record(self.reference.advance_input(TICK, false, || TickInput {
                fire: expected_fire && tick == 0,
                aim: Vec2::ZERO,
            }));
        }
        assert_eq!(actual.fired, expected_fire);
        let mut observed = Events::default();
        observed.record(actual);
        assert_eq!(observed, expected);
        assert_eq!(snapshot(&self.foundry), snapshot(&self.reference));
        assert_eq!(feedback(&self.foundry), feedback(&self.reference));
        calls
    }
}

impl Drop for TapScenario {
    fn drop(&mut self) {
        assert!(ferravex_render::input::gamepad_sample(
            0,
            Default::default()
        ));
    }
}

#[test]
fn quick_taps_and_pause_suppression_preserve_actual_collision_results() {
    let mut scenario = TapScenario::new();
    assert_eq!(scenario.frame(0.0, 0.0, false), 0); // Connect released.
    assert_eq!(scenario.frame(0.001, 1.0, false), 0);
    assert_eq!(scenario.frame(0.001, 0.0, false), 0);
    assert_eq!(scenario.frame(0.048, 0.0, true), 6);
    assert_eq!(scenario.foundry.range.ammo, AMMO - 1);

    // Follow the actual ball through contact and collapse. The single-tick
    // reference must have identical bodies and accepted destruction feedback.
    for _ in 0..30 {
        assert_eq!(scenario.frame(0.1, 0.0, false), 12);
    }
    assert_eq!(scenario.foundry.range.score(), 2);
    assert_eq!(scenario.foundry.range.shots.len(), 1);
    assert!(scenario.foundry.range.can_fire());

    // A new buffered press is cancelled by pause, even when held on resume.
    assert_eq!(scenario.frame(0.001, 1.0, false), 0);
    scenario.set_paused(true);
    assert_eq!(scenario.frame(60.0, 1.0, false), 0);
    scenario.set_paused(false);
    assert_eq!(scenario.frame(TICK, 1.0, false), 1);
    assert_eq!(scenario.foundry.range.shots.len(), 1);

    // Release rearms the action; a fresh quick tap fires on one catch-up tick.
    assert_eq!(scenario.frame(0.0, 0.0, false), 0);
    assert_eq!(scenario.frame(0.001, 1.0, false), 0);
    assert_eq!(scenario.frame(0.001, 0.0, false), 0);
    assert_eq!(scenario.frame(0.047, 0.0, true), 6);
    for _ in 0..20 {
        assert_eq!(scenario.frame(0.1, 0.0, false), 12);
    }
    assert_eq!(scenario.foundry.range.ammo, AMMO - 2);
    assert_eq!(scenario.foundry.range.shots.len(), 2);
    assert_eq!(scenario.foundry.range.score(), 2);
}
