//! Local presentation owner; simulation never reads these effects or preferences.
use super::*;

pub(super) struct FlightPresentation {
    pub(super) wing_trails: [Trail; 2],
    pub(super) exhaust: ParticlePool,
    exhaust_emitter: Emitter,
    feedback: FeedbackController<16>,
    camera: Rig3,
    previous_effect_position: Vec3,
    pub(super) propeller_time: f32,
}

impl FlightPresentation {
    pub(super) fn new() -> Self {
        Self {
            wing_trails: std::array::from_fn(|_| {
                Trail::new(TrailConfig {
                    capacity: 96,
                    lifetime: 0.9,
                    spacing: 0.5,
                    teleport_distance: 35.,
                    ..Default::default()
                })
                .expect("bounded wing trail")
            }),
            exhaust: ParticlePool::new(128),
            exhaust_emitter: Emitter::new(presets::exhaust([0.68, 0.72, 0.69, 0.22]), 0x534b59)
                .expect("authored exhaust"),
            feedback: FeedbackController::new(
                FeedbackConfig {
                    translation_scale: 0.08,
                    rumble: 0.,
                    ..Default::default()
                },
                0x534b59,
            )
            .expect("bounded flight feedback"),
            camera: Rig3::new(Settings3 {
                position_half_life: 0.1,
                target_half_life: 0.06,
                teleport_distance: Some(35.),
                ..Default::default()
            })
            .expect("authored chase camera"),
            previous_effect_position: Flight::new().pos,
            propeller_time: 0.,
        }
    }

    pub(super) fn reset(&mut self, position: Vec3) {
        self.clear_particles();
        self.feedback.clear();
        self.camera.reset();
        self.previous_effect_position = position;
        self.propeller_time = 0.;
    }

    fn clear_particles(&mut self) {
        for trail in &mut self.wing_trails {
            trail.clear();
        }
        self.exhaust.clear();
        self.exhaust_emitter.reset(0x534b59);
    }

    pub(super) fn update(
        &mut self,
        run: &FlightRun,
        frame: &FlightFrame,
        elapsed: f32,
        mut cue: impl FnMut(Cue, f32),
    ) -> (Pose3, FeedbackFrame) {
        // This is unconditional: menus, pause, results and zero-tick frames all
        // consume the host policy before emitting or sampling any feedback.
        preferences::sync_feedback(&mut self.feedback);
        let policy = preferences::feedback_policy();
        let motion = if policy.reduced_motion() {
            0.
        } else {
            policy.intensity()
        };
        if motion == 0. {
            self.clear_particles();
            self.camera.shake.clear();
            self.propeller_time = 0.;
        } else {
            self.propeller_time += frame.seconds;
        }

        let flight = &run.flight;
        if frame.damaged {
            // A recovery is discontinuous even when it moved less than 35 m.
            // Preserve the fading old segment, but never bridge it to respawn.
            for trail in &mut self.wing_trails {
                trail.break_segment();
            }
            self.camera.cut();
            if let Some(event) = self
                .feedback
                .emit(
                    16 + u64::from(3 - flight.hull),
                    FeedbackEvent::hurt(flight.pos, 1.).expect("finite impact"),
                )
                .expect("at most one impact per frame")
            {
                cue(event.cue, 0.7);
            }
        }
        for (gate, &position) in ROUTE
            .iter()
            .enumerate()
            .take(flight.next)
            .skip(flight.next - frame.gates)
        {
            if let Some(event) = self
                .feedback
                .emit(
                    gate as u64 + 1,
                    FeedbackEvent::pickup(position, 1.).expect("authored gate"),
                )
                .expect("six gates fit in bounded feedback")
            {
                cue(event.cue, event.gain);
            }
        }
        if let Some(outcome) = frame.outcome {
            cue(
                if outcome == Outcome::Delivered {
                    Cue::Success
                } else {
                    Cue::Failure
                },
                0.65,
            );
        }

        let rotation = Quat::from_rotation_y(flight.motor.yaw)
            * Quat::from_rotation_x(-flight.motor.pitch)
            * Quat::from_rotation_z(flight.motor.bank);
        let nozzle = EmitterTransform {
            position: (flight.pos + rotation * vec3(0.72, -0.35, 1.8)).to_array(),
            rotation: rotation.to_array(),
        };
        if motion > 0. {
            // Do not insert cosmetic samples on a render frame with no tick.
            if frame.seconds > 0. {
                for (side, trail) in [-1., 1.].into_iter().zip(&mut self.wing_trails) {
                    trail
                        .update(
                            frame.seconds,
                            (flight.outcome == Outcome::Flying && flight.motor.speed > 28.)
                                .then_some(TrailSample {
                                    position: (flight.pos
                                        + rotation * vec3(side * 5.65, 0.2, -0.4))
                                    .to_array(),
                                    color: [0.91, 0.93, 0.87, 0.2 * motion],
                                    width: 0.16,
                                }),
                        )
                        .expect("finite bounded wing samples");
                }
                self.exhaust
                    .update(frame.seconds)
                    .expect("bounded flight delta");
            }
            self.exhaust_emitter
                .set_rate(if flight.outcome == Outcome::Flying {
                    14. * motion
                } else {
                    0.
                })
                .expect("bounded exhaust rate");
            if frame.damaged || flight.pos.distance(self.previous_effect_position) > 35. {
                self.exhaust_emitter
                    .teleport(nozzle)
                    .expect("finite recovery nozzle");
            }
            self.exhaust_emitter
                .update(frame.seconds, nozzle, &mut self.exhaust)
                .expect("bounded flight exhaust");
        }
        self.previous_effect_position = flight.pos;

        // Camera damping uses presentation time, including between fixed ticks.
        // Local inactive sessions freeze it; simulation never receives this delta.
        let dt = if elapsed.is_finite() && (run.session.is_playing() || frame.seconds > 0.) {
            elapsed.clamp(0., 0.25)
        } else {
            0.
        };
        let feedback = self
            .feedback
            .advance(dt)
            .expect("bounded presentation delta");
        let telemetry = flight.motor.telemetry().expect("validated flight motor");
        let mut view = self
            .camera
            .update(
                dt,
                Input3 {
                    target: Target3 {
                        position: flight.pos,
                        rotation: Quat::from_rotation_y(flight.motor.yaw),
                        velocity: telemetry.velocity,
                    },
                    mode: Mode3::Chase {
                        distance: 24.,
                        height: 10.,
                        look_ahead: 20.,
                        look_height: 0.,
                        speed_distance: 0.,
                        speed_height: 0.,
                        prediction: 0.,
                        level: true,
                    },
                    fov_y: 55_f32.to_radians(),
                },
                &NoObstructions,
            )
            .expect("valid flight camera")
            .pose;
        view.position += feedback.shake.translation;
        view.target += feedback.shake.translation;
        (view, feedback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::feedback::FeedbackPolicy;

    struct RestorePolicy(FeedbackPolicy);
    impl RestorePolicy {
        fn new() -> Self {
            let old = preferences::feedback_policy();
            preferences::set_feedback_policy(FeedbackPolicy::default());
            Self(old)
        }
    }
    impl Drop for RestorePolicy {
        fn drop(&mut self) {
            preferences::set_feedback_policy(self.0);
        }
    }

    fn warm() -> (FlightRun, FlightPresentation) {
        let mut run = FlightRun::new(true);
        let mut presentation = FlightPresentation::new();
        for _ in 0..30 {
            let frame = run.advance(
                1. / 60.,
                Input {
                    boost: true,
                    ..Default::default()
                },
                false,
            );
            presentation.update(&run, &frame, 1. / 60., |_, _| {});
        }
        assert!(presentation.wing_trails.iter().all(|t| !t.is_empty()));
        assert!(!presentation.exhaust.is_empty());
        (run, presentation)
    }

    #[test]
    fn preferences_clear_existing_effects_in_every_inactive_state_without_revival() {
        let _restore = RestorePolicy::new();
        for state in [
            SessionState::Ready,
            SessionState::Paused,
            SessionState::Won,
            SessionState::Lost,
        ] {
            for (motion, flash) in [(true, false), (false, true), (true, true)] {
                preferences::set_feedback_policy(FeedbackPolicy::default());
                let (mut run, mut presentation) = warm();
                presentation
                    .feedback
                    .emit(30, FeedbackEvent::hurt(run.flight.pos, 1.).unwrap())
                    .unwrap();
                match state {
                    SessionState::Ready => run.session.reset(),
                    SessionState::Paused => {
                        run.session.set_paused(true);
                    }
                    SessionState::Won => {
                        run.session.finish(true);
                    }
                    SessionState::Lost => {
                        run.session.finish(false);
                    }
                    SessionState::Playing => unreachable!(),
                }
                let before = (
                    run.flight.pos,
                    run.flight.remaining,
                    run.session.tick(),
                    run.elapsed,
                );
                let particles = presentation.exhaust.len();
                preferences::set_feedback_policy(FeedbackPolicy::new(motion, flash, 1.).unwrap());
                let frame = run.advance(10., Input::default(), false);
                let (_, output) =
                    presentation.update(&run, &frame, 10., |_, _| panic!("inactive cue"));
                assert_eq!(output.flash == 0., flash);
                assert_eq!(presentation.feedback.impact().emphasis == 0., motion);
                if motion {
                    assert!(presentation.wing_trails.iter().all(Trail::is_empty));
                    assert!(presentation.exhaust.is_empty());
                    assert_eq!(presentation.propeller_time, 0.);
                } else {
                    assert_eq!(presentation.exhaust.len(), particles);
                }
                preferences::set_feedback_policy(FeedbackPolicy::default());
                let (_, restored) =
                    presentation.update(&run, &frame, 0., |_, _| panic!("replayed cue"));
                assert_eq!(restored.flash == 0., flash);
                assert_eq!(presentation.feedback.impact().emphasis == 0., motion);
                if motion {
                    assert!(presentation.exhaust.is_empty());
                    assert!(presentation.wing_trails.iter().all(Trail::is_empty));
                }
                assert_eq!(
                    (
                        run.flight.pos,
                        run.flight.remaining,
                        run.session.tick(),
                        run.elapsed
                    ),
                    before
                );
            }
        }
    }

    #[test]
    fn live_policy_controls_new_impacts_independently_and_preserves_audio_and_progress() {
        let _restore = RestorePolicy::new();
        let mut reference = None;
        for (motion, flash, intensity) in [
            (false, false, 1.),
            (true, false, 1.),
            (false, true, 1.),
            (true, true, 1.),
            (false, false, 0.),
        ] {
            preferences::set_feedback_policy(
                FeedbackPolicy::new(motion, flash, intensity).unwrap(),
            );
            let mut run = FlightRun::new(true);
            let mut presentation = FlightPresentation::new();
            run.flight.pos.y = -21.;
            let frame = run.advance(1. / 120., Input::default(), false);
            assert!(frame.damaged);
            let mut cues = 0;
            let (_, output) = presentation.update(&run, &frame, 1. / 120., |_, _| cues += 1);
            assert_eq!(cues, 1);
            assert_eq!(output.flash == 0., flash || intensity == 0.);
            assert_eq!(
                output.shake.translation == Vec3::ZERO,
                motion || intensity == 0.
            );
            let state = (
                run.flight.pos,
                run.flight.motor,
                run.flight.remaining,
                run.flight.hull,
                run.session.tick(),
            );
            assert_eq!(*reference.get_or_insert(state), state);
            let mut cues = 0;
            let zero = run.advance(0., Input::default(), false);
            presentation.update(&run, &zero, 1. / 240., |_, _| cues += 1);
            assert_eq!(cues, 0);
            // An immediate toggle on a playing frame with no tick clears old output.
            preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
            let (_, cleared) = presentation.update(&run, &zero, 0., |_, _| panic!("zero-tick cue"));
            assert_eq!(cleared.flash, 0.);
            assert_eq!(cleared.shake.translation, Vec3::ZERO);
        }
    }

    #[test]
    fn short_recovery_breaks_wing_segments_and_exhaust_motion_history() {
        let _restore = RestorePolicy::new();
        let (mut run, mut presentation) = warm();
        let previous = run.flight.pos;
        run.flight.damage();
        assert!(previous.distance(run.flight.pos) < 35.);
        // Force a post-recovery wing sample to exercise the explicit break,
        // independent of the normal short cruise-speed gap while boosting again.
        run.flight.motor.speed = 35.;
        let frame = FlightFrame {
            seconds: 1. / 60.,
            damaged: true,
            ..Default::default()
        };
        presentation.update(&run, &frame, 1. / 60., |_, _| {});
        for trail in &presentation.wing_trails {
            let points: Vec<_> = trail.iter().collect();
            assert!(points.len() > 2);
            let crossing = points
                .windows(2)
                .find(|p| {
                    Vec3::from_array(p[0].position).distance(Vec3::from_array(p[1].position)) > 5.
                })
                .expect("old trail and recovered head both survive");
            assert!(
                crossing[1].break_before,
                "no wing ribbon may cross the recovery"
            );
        }
        // Old exhaust may finish fading, but newly emitted particles must not
        // acquire teleport velocity and streak across the recovery distance.
        for _ in 0..6 {
            presentation.update(
                &run,
                &FlightFrame {
                    seconds: 1. / 60.,
                    ..Default::default()
                },
                1. / 60.,
                |_, _| {},
            );
        }
        assert!(
            presentation
                .exhaust
                .iter()
                .all(|p| Vec3::from_array(p.position).distance(run.flight.pos) < 35.)
        );
    }

    #[test]
    fn camera_smooths_between_ticks_but_freezes_in_pause_and_retry_clears_history() {
        let _restore = RestorePolicy::new();
        let mut run = FlightRun::new(true);
        let mut presentation = FlightPresentation::new();
        let initial = presentation
            .update(&run, &FlightFrame::default(), 0., |_, _| {})
            .0;
        let frame = run.advance(1. / 120., Input::default(), false);
        let first = presentation.update(&run, &frame, 1. / 240., |_, _| {}).0;
        let before = (run.flight.pos, run.session.tick());
        let zero = run.advance(0., Input::default(), false);
        let next = presentation.update(&run, &zero, 1. / 240., |_, _| {}).0;
        assert!(first.position.z > initial.position.z);
        assert!(next.position.z > first.position.z);
        assert_eq!((run.flight.pos, run.session.tick()), before);
        run.session.set_paused(true);
        let paused = presentation.update(&run, &zero, 2., |_, _| {}).0;
        assert_eq!(paused, next);
        run.restart();
        presentation.reset(run.flight.pos);
        assert!(presentation.wing_trails.iter().all(Trail::is_empty));
        assert!(presentation.exhaust.is_empty());
        assert_eq!(presentation.feedback.impact().flash, 0.);
        let reset = presentation
            .update(&run, &FlightFrame::default(), 0., |_, _| {})
            .0;
        assert_eq!(reset, initial);
    }

    #[test]
    fn autoplay_results_and_accepted_cues_match_with_all_visual_effects_disabled() {
        let _restore = RestorePolicy::new();
        let mut reference = None;
        for reduced in [false, true] {
            preferences::set_feedback_policy(FeedbackPolicy::new(reduced, reduced, 1.).unwrap());
            let mut run = FlightRun::new(true);
            let mut presentation = FlightPresentation::new();
            let mut pickups = 0;
            let mut wins = 0;
            for _ in 0..6_000 {
                let frame = run.advance(1. / 60., Input::default(), true);
                presentation.update(&run, &frame, 1. / 60., |cue, _| match cue {
                    Cue::Pickup => pickups += 1,
                    Cue::Success => wins += 1,
                    _ => panic!("unexpected autoplay cue"),
                });
                if !run.session.is_playing() {
                    break;
                }
            }
            assert_eq!((pickups, wins), (6, 1));
            assert_eq!(run.session.state(), SessionState::Won);
            let result = (
                run.flight.pos,
                run.flight.motor,
                run.flight.remaining,
                run.elapsed,
            );
            assert_eq!(*reference.get_or_insert(result), result);
            if reduced {
                assert!(presentation.wing_trails.iter().all(Trail::is_empty));
                assert!(presentation.exhaust.is_empty());
            }
        }
    }
}
