//! Local presentation owner; simulation never reads these effects or preferences.
//!
//! Juice is created only from accepted simulation events (gates, hidden rings,
//! impacts, outcomes) and aged by accepted simulation time, so pausing freezes
//! it. Reduced motion clears and blocks sparks, rising pops, FOV kicks, speed
//! lines, ring spin and the results orbit; reduced flash clears the gate glow
//! and impact edge. Text pops remain readable under both.
use super::*;
use ferravex_play::particles::{EmitterDesc, ParticleDesc, ParticleMotion, SpawnShape};

const POPS: usize = 6;
pub(super) const POP_LIFE: f32 = 1.1;
pub(super) const GOLD: Color = Color::new(1.0, 0.85, 0.45, 1.0);
pub(super) const MINT: Color = Color::new(0.56, 0.92, 0.74, 1.0);
const SPARK_SEED: u64 = 0x534b_5350;
const ROLL: f32 = 0.75;

/// A world-anchored rising tip label.
#[derive(Clone, Debug)]
pub(super) struct Pop {
    pub at: Vec3,
    pub text: String,
    pub color: Color,
    pub age: f32,
}

pub(super) struct FlightPresentation {
    pub(super) wing_trails: [Trail; 2],
    pub(super) exhaust: ParticlePool,
    exhaust_emitter: Emitter,
    feedback: FeedbackController<16>,
    camera: Rig3,
    previous_effect_position: Vec3,
    pub(super) propeller_time: f32,
    pub(super) sparks: ParticlePool,
    spark_emitter: Emitter,
    pub(super) pops: [Option<Pop>; POPS],
    /// Presentation clock advanced by accepted ticks while motion is allowed.
    pub(super) clock: f32,
    /// Smoothed speed-line strength, `0..=1`; zero under reduced motion.
    pub(super) speed_lines: f32,
    /// Gold edge glow after a bullseye; zero under reduced flash.
    pub(super) gate_glow: f32,
    fov_kick: f32,
    speed_fov: f32,
    /// Render seconds since a result, used only for the results orbit.
    celebrate: f32,
    /// Seconds left in a cosmetic victory roll.
    roll: f32,
    pub(super) motion: f32,
    flash: f32,
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
            sparks: ParticlePool::new(256),
            spark_emitter: Emitter::new(spark_desc(GOLD, AMBER, 10., 0.9, 0.9), SPARK_SEED)
                .expect("authored sparks"),
            pops: Default::default(),
            clock: 0.,
            speed_lines: 0.,
            gate_glow: 0.,
            fov_kick: 0.,
            speed_fov: 0.,
            celebrate: 0.,
            roll: 0.,
            motion: 1.,
            flash: 1.,
        }
    }

    pub(super) fn reset(&mut self, position: Vec3) {
        self.clear_particles();
        self.feedback.clear();
        self.camera.reset();
        self.previous_effect_position = position;
        self.propeller_time = 0.;
        self.pops = Default::default();
        self.clock = 0.;
        self.gate_glow = 0.;
        self.celebrate = 0.;
        self.roll = 0.;
    }

    fn clear_particles(&mut self) {
        for trail in &mut self.wing_trails {
            trail.clear();
        }
        self.exhaust.clear();
        self.exhaust_emitter.reset(0x534b59);
        self.sparks.clear();
        self.spark_emitter.reset(SPARK_SEED);
        self.speed_lines = 0.;
        self.fov_kick = 0.;
        self.speed_fov = 0.;
        self.roll = 0.;
    }

    fn burst(&mut self, desc: EmitterDesc, at: Vec3, count: u32) {
        if self.motion == 0. {
            return;
        }
        self.spark_emitter.set_config(desc).expect("authored burst");
        let budget = (count as f32 * self.motion).ceil() as u16;
        self.spark_emitter
            .burst_with_budget(
                count,
                budget,
                EmitterTransform {
                    position: at.to_array(),
                    rotation: Quat::IDENTITY.to_array(),
                },
                &mut self.sparks,
            )
            .expect("finite accepted event position");
    }

    fn pop(&mut self, at: Vec3, text: String, color: Color) {
        let slot = self
            .pops
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.pops
                    .iter()
                    .enumerate()
                    .max_by(|a, b| {
                        let age = |p: &Option<Pop>| p.as_ref().map_or(0., |p| p.age);
                        age(a.1).total_cmp(&age(b.1))
                    })
                    .map_or(0, |(i, _)| i)
            });
        self.pops[slot] = Some(Pop {
            at,
            text,
            color,
            age: 0.,
        });
    }

    /// Route accepted courier events into sparks, pops, glow and FOV kicks.
    fn courier_events(&mut self, flight: &Flight, frame: &FlightFrame, flash: f32) {
        let ledger = &flight.ledger;
        for (gate, &at) in ROUTE
            .iter()
            .enumerate()
            .take(flight.next)
            .skip(flight.next - frame.gates)
        {
            if let Some(pass) = ledger.gates.get(gate).copied().flatten() {
                let (count, kick, color) = match pass.line {
                    courier::Line::Bullseye => (44, 0.14, GOLD),
                    courier::Line::Clean => (28, 0.09, CREAM),
                    courier::Line::Scrape => (14, 0.05, CORAL),
                };
                self.burst(spark_desc(color, AMBER, 11., 0.8, 0.9), at, count);
                self.fov_kick = (self.fov_kick + kick * self.motion).min(0.2);
                if pass.line == courier::Line::Bullseye {
                    self.gate_glow = flash;
                }
                if quips::streak(pass.multiplier).is_some() && self.motion > 0. {
                    self.roll = ROLL;
                }
                self.pop(at + Vec3::Y * 11., format!("+${}", pass.tip), GOLD);
            } else if let Some(drop) = ledger.drop {
                for color in [CORAL, GOLD, MINT, CREAM] {
                    self.burst(spark_desc(color, color, 16., 1.6, 1.3), at, 30);
                }
                self.gate_glow = flash;
                self.pop(
                    at + Vec3::Y * 9.,
                    format!("+${}", drop.tip + drop.early),
                    GOLD,
                );
            }
        }
        for (index, ring) in courier::BONUS_RINGS.iter().enumerate() {
            if frame.bonus & (1 << index) == 0 {
                continue;
            }
            self.burst(spark_desc(GOLD, MINT, 14., 1.2, 0.8), *ring, 48);
            self.fov_kick = (self.fov_kick + 0.16 * self.motion).min(0.2);
            self.gate_glow = flash;
            if self.motion > 0. {
                self.roll = ROLL;
            }
            let tip = courier::BONUS_TIP
                + if ledger.scenic() {
                    courier::SCENIC_TIP
                } else {
                    0
                };
            self.pop(*ring + Vec3::Y * 8., format!("+${tip}"), MINT);
        }
    }

    /// Victory-roll angle in radians: one eased full turn, zero when reduced.
    pub(super) fn roll_angle(&self) -> f32 {
        if self.roll <= 0. || self.motion == 0. {
            return 0.;
        }
        let t = 1. - self.roll / ROLL;
        let eased = t * t * (3. - 2. * t);
        -std::f32::consts::TAU * eased
    }

    /// Blink the aircraft while recovery protection lasts, unless flashing
    /// effects are reduced; the shield countdown in the HUD remains.
    pub(super) fn aircraft_visible(&self, flight: &Flight) -> bool {
        flight.recovery <= 0. || self.flash == 0. || (flight.recovery * 4.).fract() > 0.35
    }

    /// Screen-space pops and the results orbit read this; zero when reduced.
    pub(super) fn pop_rise(&self, pop: &Pop) -> f32 {
        pop.age * 46. * self.motion
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
        let flash = if policy.reduced_flash() {
            0.
        } else {
            policy.intensity()
        };
        self.motion = motion;
        self.flash = flash;
        if motion == 0. {
            self.clear_particles();
            self.camera.shake.clear();
            self.propeller_time = 0.;
            self.celebrate = 0.;
            self.clock = 0.;
        } else {
            self.propeller_time += frame.seconds;
            self.clock += frame.seconds;
        }
        if flash == 0. {
            self.gate_glow = 0.;
        }

        let flight = &run.flight;
        self.courier_events(flight, frame, flash);
        // Pops and glow age on accepted time only, so pause freezes them.
        for slot in &mut self.pops {
            if let Some(pop) = slot {
                pop.age += frame.seconds;
                if pop.age >= POP_LIFE {
                    *slot = None;
                }
            }
        }
        self.gate_glow = (self.gate_glow - frame.seconds * 2.5).max(0.);
        self.roll = (self.roll - frame.seconds).max(0.);
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
        for (index, &ring) in courier::BONUS_RINGS.iter().enumerate() {
            if frame.bonus & (1 << index) != 0
                && let Some(event) = self
                    .feedback
                    .emit(
                        40 + index as u64,
                        FeedbackEvent::pickup(ring, 1.).expect("authored ring"),
                    )
                    .expect("three hidden rings fit in bounded feedback")
            {
                cue(event.cue, 0.9);
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
                self.sparks
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
        // Speed sensations follow accepted speed, smoothed on presentation time.
        let fast = ((flight.motor.speed - 26.) / 8.).clamp(0., 1.);
        let playing = flight.outcome == Outcome::Flying && run.session.is_playing();
        let ease = 1. - (-dt * 5.).exp();
        let lines = if playing { fast * motion } else { 0. };
        self.speed_lines += (lines - self.speed_lines) * ease;
        self.speed_fov += (fast * 0.09 * motion - self.speed_fov) * ease;
        self.fov_kick *= (-dt * 5.).exp();
        // Results: let confetti settle and slowly orbit, on render time only.
        let finished = matches!(run.session.state(), SessionState::Won | SessionState::Lost);
        if finished {
            self.speed_lines = 0.;
        }
        if finished && elapsed.is_finite() {
            let step = elapsed.clamp(0., 0.1);
            self.gate_glow = (self.gate_glow - step * 2.5).max(0.);
            for slot in &mut self.pops {
                if let Some(pop) = slot {
                    pop.age += step;
                    if pop.age >= POP_LIFE {
                        *slot = None;
                    }
                }
            }
            if motion > 0. {
                self.celebrate += step;
                self.sparks.update(step).expect("bounded celebration delta");
            }
        }
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
        view.fov_y += self.speed_fov + self.fov_kick;
        if self.celebrate > 0. {
            let blend = (self.celebrate / 1.5).min(1.);
            let blend = blend * blend * (3. - 2. * blend);
            // Orbit the aircraft, framed low so it stays visible under the card.
            let pivot = flight.pos;
            let arm = view.position - pivot;
            let angle = self.celebrate * 0.22 * blend;
            let orbit = Quat::from_rotation_y(angle) * arm;
            view.position = pivot + orbit + Vec3::Y * 5. * blend;
            view.target = view.target.lerp(pivot + Vec3::Y * 7., blend);
        }
        (view, feedback)
    }
}

fn spark_desc(start: Color, end: Color, speed: f32, life: f32, size: f32) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            acceleration: [0., -7., 0.],
            lifetime: life,
            start_color: [start.r, start.g, start.b, 0.95],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size,
            end_size: size * 0.15,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 1.6,
            angular_velocity: 5.,
            ..Default::default()
        },
        shape: SpawnShape::Disc { radius: 8.5 },
        velocity_jitter: [speed, speed, speed * 0.6],
        lifetime_jitter: 0.35,
        size_jitter: 0.4,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        max_per_update: 64,
        ..Default::default()
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

    fn to_first_gate(policy: FeedbackPolicy) -> (FlightRun, FlightPresentation, Pose3) {
        preferences::set_feedback_policy(policy);
        let mut run = FlightRun::new(true);
        let mut presentation = FlightPresentation::new();
        for _ in 0..600 {
            let frame = run.advance(1. / 60., Input::default(), true);
            let view = presentation.update(&run, &frame, 1. / 60., |_, _| {}).0;
            if frame.gates > 0 {
                return (run, presentation, view);
            }
        }
        panic!("autopilot reaches the first ring");
    }

    #[test]
    fn accepted_gates_pay_juice_that_freezes_in_pause_and_respects_policy() {
        let _restore = RestorePolicy::new();
        let (mut run, mut presentation, view) = to_first_gate(FeedbackPolicy::default());
        let pass = run.flight.ledger.gates[0].expect("accepted gate");
        assert!(!presentation.sparks.is_empty());
        assert!(view.fov_y > 55_f32.to_radians(), "a gate kicks the FOV");
        let pop = presentation.pops.iter().flatten().next().expect("tip pop");
        assert_eq!(pop.text, format!("+${}", pass.tip));
        assert!(presentation.gate_glow > 0., "autopilot threads a bullseye");
        // Pause freezes every juice timer, however long the render frame.
        run.session.set_paused(true);
        let ages: Vec<_> = presentation.pops.iter().flatten().map(|p| p.age).collect();
        let (glow, sparks) = (presentation.gate_glow, presentation.sparks.len());
        let frame = run.advance(5., Input::default(), true);
        presentation.update(&run, &frame, 5., |_, _| panic!("paused cue"));
        let after: Vec<_> = presentation.pops.iter().flatten().map(|p| p.age).collect();
        assert_eq!(after, ages);
        assert_eq!(
            (presentation.gate_glow, presentation.sparks.len()),
            (glow, sparks)
        );
        // Reduced motion clears moving effects but keeps the readable pop.
        preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        let (paused, _) = presentation.update(&run, &frame, 0., |_, _| {});
        assert!(presentation.sparks.is_empty());
        assert_eq!(presentation.speed_lines, 0.);
        assert_eq!(presentation.roll_angle(), 0.);
        assert_eq!(paused.fov_y, 55_f32.to_radians());
        let pop = presentation.pops.iter().flatten().next().unwrap();
        assert_eq!(presentation.pop_rise(pop), 0.);
        assert!(
            presentation.gate_glow > 0.,
            "flash is independent of motion"
        );
        preferences::set_feedback_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        presentation.update(&run, &frame, 0., |_, _| {});
        assert_eq!(presentation.gate_glow, 0.);
        preferences::set_feedback_policy(FeedbackPolicy::default());
        presentation.update(&run, &frame, 0., |_, _| {});
        assert!(presentation.sparks.is_empty() && presentation.gate_glow == 0.);
        // Retry discards everything.
        run.restart();
        presentation.reset(run.flight.pos);
        assert!(presentation.pops.iter().all(Option::is_none));
        assert_eq!(presentation.roll_angle(), 0.);
    }

    #[test]
    fn reduced_policies_change_no_accepted_tips_or_cues() {
        let _restore = RestorePolicy::new();
        let (full, _, _) = to_first_gate(FeedbackPolicy::default());
        let (reduced, presentation, _) =
            to_first_gate(FeedbackPolicy::new(true, true, 1.).unwrap());
        assert_eq!(full.flight.ledger, reduced.flight.ledger);
        assert!(presentation.sparks.is_empty() && presentation.gate_glow == 0.);
        assert!(presentation.pops.iter().flatten().count() == 1);
    }

    #[test]
    fn hidden_ring_pays_one_cue_sparkle_pop_and_roll() {
        let _restore = RestorePolicy::new();
        let mut run = FlightRun::new(true);
        let mut presentation = FlightPresentation::new();
        run.flight.pos = courier::BONUS_RINGS[1] - Vec3::Z * 3.;
        let frame = run.advance(1. / 120., Input::default(), false);
        assert_eq!(frame.bonus, 0b010);
        assert_eq!(run.flight.boost_energy, 1.);
        assert!(run.flight.bonus_notice > 0.);
        let mut cues = Vec::new();
        presentation.update(&run, &frame, 1. / 120., |cue, _| cues.push(cue));
        assert_eq!(cues, [Cue::Pickup]);
        assert!(!presentation.sparks.is_empty());
        assert!(presentation.roll_angle() < 0., "a victory roll begins");
        let frame = run.advance(1. / 120., Input::default(), false);
        assert_eq!(frame.bonus, 0, "collected once");
        presentation.update(&run, &frame, 1. / 120., |_, _| panic!("no repeat cue"));
        let pop = presentation.pops.iter().flatten().next().unwrap();
        assert_eq!(pop.text, format!("+${}", courier::BONUS_TIP));
    }
}
