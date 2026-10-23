use ferravex_play::{
    navigation::{Route3, RouteProjection},
    persistence::{RecordBook, RecordDelta, run_micros},
    racing::DriverRecovery,
};
use ferravex_render::prelude::*;

pub const ROAD_HALF: f32 = 33.0;
pub const LAPS: u32 = 3;
const SAMPLES: usize = 192;
const GATES: usize = 16;

pub struct Track {
    pub points: Vec<Vec2>,
    route: Route3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoadCue {
    Straight,
    Left,
    Right,
    WrongWay,
}
impl Track {
    pub fn new() -> Self {
        let knots = [
            vec2(200., 510.),
            vec2(120., 370.),
            vec2(160., 190.),
            vec2(340., 120.),
            vec2(550., 160.),
            vec2(720., 100.),
            vec2(930., 170.),
            vec2(980., 340.),
            vec2(850., 480.),
            vec2(650., 500.),
            vec2(520., 390.),
            vec2(350., 530.),
        ];
        let points: Vec<Vec2> = (0..SAMPLES)
            .map(|i| {
                let k = i / 16;
                let t = (i % 16) as f32 / 16.;
                let [a, b, c, d] = [
                    knots[(k + 11) % 12],
                    knots[k],
                    knots[(k + 1) % 12],
                    knots[(k + 2) % 12],
                ];
                0.5 * ((2. * b)
                    + (-a + c) * t
                    + (2. * a - 5. * b + 4. * c - d) * t * t
                    + (-a + 3. * b - 3. * c + d) * t * t * t)
            })
            .collect();
        let mut route = Route3::new(SAMPLES).expect("bounded rally circuit");
        route
            .set_closed(&points.iter().map(|p| p.extend(0.)).collect::<Vec<_>>())
            .expect("finite authored circuit");
        Self { points, route }
    }
    pub fn tangent(&self, index: usize) -> Vec2 {
        (self.points[(index + 1) % SAMPLES] - self.points[(index + SAMPLES - 1) % SAMPLES])
            .normalize()
    }
    pub fn project(&self, position: Vec2) -> RouteProjection {
        self.route
            .project(position.extend(0.))
            .expect("finite rally position")
            .expect("nonempty rally circuit")
    }
    pub fn nearest(&self, p: Vec2) -> (usize, f32) {
        let hit = self.project(p);
        (hit.segment, hit.distance_to_route as f32)
    }
    /// Geometry is guidance only; ordered gate crossings remain authoritative.
    pub fn road_cue(&self, car: &Car) -> RoadCue {
        let hit = self.project(car.pose.position);
        let forward = Vec2::from_angle(car.pose.angle);
        let tangent = hit.direction.truncate();
        if forward.dot(tangent) < -0.5 || car.velocity.dot(tangent) < -5. {
            return RoadCue::WrongWay;
        }
        let ahead = self
            .route
            .sample(hit.distance + f64::from((car.velocity.length() * 0.5).clamp(65., 125.)))
            .expect("bounded rally lookahead")
            .expect("nonempty rally circuit");
        let turn = tangent.perp_dot(ahead.direction.truncate());
        if turn > 0.25 {
            RoadCue::Right // The circuit uses Y-down display coordinates.
        } else if turn < -0.25 {
            RoadCue::Left
        } else {
            RoadCue::Straight
        }
    }
    pub fn gate(&self, gate: usize) -> (Vec2, Vec2) {
        let i = gate * SAMPLES / GATES;
        (self.points[i], self.tangent(i))
    }
    fn crossed(&self, gate: usize, from: Vec2, to: Vec2) -> bool {
        let (center, tangent) = self.gate(gate);
        let before = (from - center).dot(tangent);
        let after = (to - center).dot(tangent);
        if before >= 0. || after < 0. {
            return false;
        }
        let crossing = from.lerp(to, -before / (after - before));
        crossing.distance(center) <= ROAD_HALF + 4.
    }
}

#[derive(Default, Clone, Copy)]
pub struct Controls {
    pub throttle: f32,
    pub steer: f32,
    pub handbrake: bool,
}
#[derive(Clone, Copy)]
pub struct Pose {
    pub position: Vec2,
    pub angle: f32,
}
pub struct Car {
    pub pose: Pose,
    pub velocity: Vec2,
    pub offroad: bool,
    pub sliding: bool,
    steering: f32,
    tuning: ferravex_play::tuning::TuningProfile,
}
impl Car {
    fn new(track: &Track) -> Self {
        let heading = track.tangent(0);
        Self {
            pose: Pose {
                position: track.points[0],
                angle: heading.y.atan2(heading.x),
            },
            velocity: Vec2::ZERO,
            offroad: false,
            sliding: false,
            steering: 0.,
            tuning: ferravex_play::tuning::TuningProfile::rally(),
        }
    }
    pub fn set_tuning(
        &mut self,
        profile: ferravex_play::tuning::TuningProfile,
    ) -> Result<(), ferravex_play::persistence::SaveError> {
        self.tuning.replace(profile)
    }
    fn step(&mut self, track: &Track, control: Controls, dt: f32) {
        self.offroad = track.nearest(self.pose.position).1 > ROAD_HALF;
        let forward = Vec2::from_angle(self.pose.angle);
        let right = vec2(-forward.y, forward.x);
        let mut longitudinal = self.velocity.dot(forward);
        let lateral = self.velocity.dot(right);
        let throttle = control.throttle.clamp(-1., 1.);
        let force = if throttle < 0. && longitudinal > 5. {
            self.tuning.motion.braking
        } else {
            self.tuning.motion.acceleration
        };
        longitudinal += throttle * force * dt;
        let drag = if self.offroad {
            2.5
        } else {
            0.18 + longitudinal.abs() * 0.0015
        };
        longitudinal *= (-drag * dt).exp();
        if control.handbrake {
            longitudinal *= (-1.5 * dt).exp();
        }
        longitudinal = longitudinal.clamp(-65., self.tuning.motion.max_speed);
        let grip = if control.handbrake {
            1.5
        } else if self.offroad {
            5.
        } else {
            self.tuning.motion.traction
        };
        self.sliding = lateral.abs() > 30. || control.handbrake && longitudinal > 30.;
        self.velocity = forward * longitudinal + right * lateral * (-grip * dt).exp();
        let steer = if self.tuning.assists.steering > 0. {
            let nearest = track.nearest(self.pose.position).0;
            let desired_heading = track.tangent(nearest);
            control.steer
                + forward.perp_dot(desired_heading).clamp(-1., 1.) * self.tuning.assists.steering
        } else {
            control.steer
        };
        self.steering += (steer.clamp(-1., 1.) - self.steering)
            * ferravex_play::motion::response(self.tuning.motion.response, dt);
        let steering = self.steering
            * (if control.handbrake { 3.6 } else { 2.9 })
            * (longitudinal / 90.).clamp(-0.8, 1.0);
        self.pose.angle = (self.pose.angle + steering * dt).rem_euclid(std::f32::consts::TAU);
        self.pose.position += self.velocity * dt;
        let bounded = self.pose.position.clamp(vec2(28., 25.), vec2(1072., 625.));
        let correction = bounded - self.pose.position;
        if correction.length_squared() > 0. {
            self.velocity = ferravex_play::motion::impact_velocity(
                self.velocity.extend(0.),
                correction.extend(0.),
                0.15,
                0.94,
            )
            .expect("finite rally boundary")
            .truncate();
        }
        self.pose.position = bounded;
    }
}

pub struct Race {
    pub car: Car,
    pub next_gate: usize,
    pub laps: u32,
    pub lap_time: f32,
    pub total_time: f32,
    pub best_time: Option<f32>,
    pub last_lap: Option<f32>,
    pub last_comparison: Option<RecordDelta>,
    pub best_ghost: Vec<Pose>,
    recording: Vec<Pose>,
    ghost_track: Option<ferravex_play::replay::ghost::GhostTrack>,
    lap_ticks: u64,
    pub ticks: u64,
    driver: ferravex_play::racing::RacingDriver,
}
impl Race {
    pub fn new(track: &Track) -> Self {
        Self {
            car: Car::new(track),
            next_gate: 1,
            laps: 0,
            lap_time: 0.,
            total_time: 0.,
            best_time: None,
            last_lap: None,
            last_comparison: None,
            best_ghost: Vec::new(),
            recording: vec![Car::new(track).pose],
            ghost_track: None,
            lap_ticks: 0,
            ticks: 0,
            driver: ferravex_play::racing::RacingDriver::new(0),
        }
    }
    pub fn finished(&self) -> bool {
        self.laps >= LAPS
    }
    /// Gates cleared on the current lap, including the finish on a completed race.
    pub fn checkpoints_passed(&self) -> usize {
        if self.finished() {
            GATES
        } else if self.next_gate == 0 {
            GATES - 1
        } else {
            self.next_gate - 1
        }
    }
    pub fn reset(&mut self, track: &Track) {
        let tuning = self.car.tuning;
        let ghost = std::mem::take(&mut self.best_ghost);
        let best = self.best_time;
        let ghost_track = self.ghost_track.take();
        *self = Self::new(track);
        self.car.set_tuning(tuning).unwrap();
        self.best_ghost = ghost;
        self.best_time = best;
        self.ghost_track = ghost_track;
    }
    pub fn step(&mut self, track: &Track, controls: Controls, dt: f32) {
        if self.finished()
            || !dt.is_finite()
            || dt <= 0.
            || !controls.throttle.is_finite()
            || !controls.steer.is_finite()
        {
            return;
        }
        let dt = dt.min(1. / 30.);
        let previous = self.car.pose.position;
        self.car.step(track, controls, dt);
        self.total_time += dt;
        self.lap_time += dt;
        self.ticks += 1;
        self.lap_ticks += 1;
        if self.lap_ticks.is_multiple_of(6) && self.recording.len() < 6000 {
            self.recording.push(self.car.pose);
        }
        if track.crossed(self.next_gate, previous, self.car.pose.position) {
            if self.next_gate == 0 {
                self.laps += 1;
                self.last_lap = Some(self.lap_time);
                self.last_comparison = compare_lap(self.best_time, self.lap_time);
                if self.best_time.is_none_or(|best| self.lap_time < best) {
                    self.best_time = Some(self.lap_time);
                    self.best_ghost.clone_from(&self.recording);
                    self.rebuild_ghost();
                }
                self.recording.clear();
                self.recording.push(self.car.pose);
                self.lap_ticks = 0;
                self.lap_time = 0.;
            }
            self.next_gate = (self.next_gate + 1) % GATES;
        }
    }
    pub fn ghost(&self) -> Option<Pose> {
        if self.best_ghost.is_empty() || self.finished() {
            return None;
        }
        // Recorded keys are six simulation ticks apart. Integer division avoids
        // accumulated f32 lap-time drift at exact ghost keys and endpoints.
        let tick = self.lap_ticks / 6;
        let alpha = (self.lap_ticks % 6) as f32 / 6.;
        let sample = self
            .ghost_track
            .as_ref()?
            .sample_checked(tick, alpha)
            .ok()?
            .sample()?; // Hide missing/truncated history; never clamp a parked ghost.
        let heading = sample.rotation * Vec3::X;
        Some(Pose {
            position: vec2(sample.position.x, sample.position.y),
            angle: heading.y.atan2(heading.x),
        })
    }
    pub fn rebuild_ghost(&mut self) {
        use ferravex_play::replay::ghost::{GhostTrack, Sample};
        let mut track = GhostTrack::new(0, 20, self.best_ghost.len()).expect("bounded rally ghost");
        for (tick, p) in self.best_ghost.iter().enumerate() {
            if track
                .push(Sample {
                    tick: tick as u64,
                    position: vec3(p.position.x, p.position.y, 0.),
                    rotation: Quat::from_rotation_z(p.angle),
                    teleport: false,
                })
                .is_err()
            {
                self.ghost_track = None;
                return;
            }
        }
        self.ghost_track = Some(track);
    }
    pub fn driver_reversing(&self) -> bool {
        self.driver
            .status()
            .is_some_and(|status| status.recovery == DriverRecovery::Reversing)
    }
    /// Pure pursuit driver feeds the exact same controls and vehicle model as a human.
    pub fn autopilot(&mut self, track: &Track) -> Controls {
        use ferravex_play::racing::*;
        let control = self
            .driver
            .step(
                &track.points,
                DriverSettings::default(),
                DriverInput {
                    position: self.car.pose.position,
                    forward: Vec2::from_angle(self.car.pose.angle),
                    velocity: self.car.velocity,
                    rivals: &[],
                },
                1. / 120.,
            )
            .unwrap();
        Controls {
            throttle: control.throttle,
            steer: control.steer,
            handbrake: false,
        }
    }
}

/// Compare only an accepted lap with the best before that lap. The temporary
/// one-entry book supplies shared microsecond comparison semantics without
/// replacing the existing f32 record authority or changing the saved ghost.
fn compare_lap(previous: Option<f32>, completed: f32) -> Option<RecordDelta> {
    let mut book = RecordBook::default();
    if let Some(previous) = previous {
        book.record("lap", 0, 0, Some(run_micros(previous)?)).ok()?;
    }
    book.compare("lap", 0, 0, Some(run_micros(completed)?))
        .ok()?
        .time_us
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_projection_matches_the_authored_road_and_closes_the_seam() {
        let track = Track::new();
        // Compare with the original f32 road-distance calculation on/off road.
        for index in 0..SAMPLES {
            let a = track.points[index];
            let b = track.points[(index + 1) % SAMPLES];
            let tangent = (b - a).normalize();
            let normal = vec2(-tangent.y, tangent.x);
            for offset in [-60., -33.5, -32.5, 0., 32.5, 33.5, 60.] {
                let p = a.lerp(b, 0.4) + normal * offset;
                let legacy = (0..SAMPLES)
                    .map(|i| {
                        let start = track.points[i];
                        let ab = track.points[(i + 1) % SAMPLES] - start;
                        let t = ((p - start).dot(ab) / ab.length_squared()).clamp(0., 1.);
                        p.distance(start + ab * t)
                    })
                    .fold(f32::MAX, f32::min);
                let hit = track.project(p);
                assert!((hit.distance_to_route as f32 - legacy).abs() < 0.0002);
                assert_eq!(hit.distance_to_route as f32 > ROAD_HALF, legacy > ROAD_HALF);
                assert!(track.nearest(hit.position.truncate()).1 < 0.0002);
            }
        }
        assert_eq!(track.project(track.points[0]).progress, 0.);
        let closing = track.points[SAMPLES - 1].lerp(track.points[0], 0.5);
        let hit = track.project(closing);
        assert_eq!(hit.segment, SAMPLES - 1);
        assert!(hit.progress > 0.99 && hit.progress < 1.);
    }

    #[test]
    fn bend_and_wrong_way_guidance_observes_geometry_without_awarding_progress() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut left = false;
        let mut right = false;
        for index in 0..SAMPLES {
            let a = track.points[index];
            let b = track.points[(index + 1) % SAMPLES];
            let tangent = (b - a).normalize();
            race.car.pose = Pose {
                position: a.lerp(b, 0.5),
                angle: tangent.y.atan2(tangent.x),
            };
            race.car.velocity = tangent * 100.;
            let cue = track.road_cue(&race.car);
            left |= cue == RoadCue::Left;
            right |= cue == RoadCue::Right;
            assert_ne!(cue, RoadCue::WrongWay);
            race.car.pose.angle += std::f32::consts::PI;
            assert_eq!(track.road_cue(&race.car), RoadCue::WrongWay);
            race.car.pose.angle -= std::f32::consts::PI;
            race.car.velocity = -tangent * 30.;
            assert_eq!(track.road_cue(&race.car), RoadCue::WrongWay);
        }
        assert!(
            left && right,
            "the authored circuit contains both kinds of bends"
        );
        assert_eq!((race.laps, race.next_gate, race.ticks), (0, 1, 0));
        assert!(race.last_comparison.is_none());
    }

    #[test]
    fn ghost_uses_recorded_tick_cadence_and_hides_after_the_last_key() {
        let track = Track::new();
        let mut race = Race::new(&track);
        race.best_ghost = vec![
            Pose {
                position: vec2(100., 100.),
                angle: 0.,
            },
            Pose {
                position: vec2(160., 100.),
                angle: 0.,
            },
        ];
        race.rebuild_ghost();
        // Even a mismatched presentation timer cannot skew a recorded tick.
        race.lap_time = 999.;
        for (ticks, x) in [(0, 100.), (3, 130.), (6, 160.)] {
            race.lap_ticks = ticks;
            assert!((race.ghost().unwrap().position.x - x).abs() < 0.0001);
        }
        race.lap_ticks = 7;
        assert!(race.ghost().is_none());
        race.reset(&track);
        assert_eq!(race.ghost().unwrap().position, vec2(100., 100.));
    }

    #[test]
    fn actual_laps_compare_against_the_previous_best_and_restart_clears_summary() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut accepted = 0;
        for _ in 0..24000 {
            let (lap, best) = (race.laps, race.best_time);
            let controls = race.autopilot(&track);
            race.step(&track, controls, 1. / 120.);
            if race.laps > lap {
                accepted += 1;
                let summary = race.last_comparison.unwrap();
                assert_eq!(summary.previous, best.and_then(run_micros));
                assert_eq!(summary.value, run_micros(race.last_lap.unwrap()).unwrap());
                assert_eq!(
                    summary.delta,
                    summary
                        .previous
                        .map(|prior| i128::from(summary.value) - i128::from(prior))
                );
                assert_eq!(summary.comparison.is_new_best(), race.best_time != best);
            }
            if race.finished() {
                break;
            }
        }
        assert_eq!(accepted, 3);
        let best = race.best_time;
        race.reset(&track);
        assert_eq!(race.best_time, best);
        assert!(race.last_comparison.is_none());
        assert!(!race.driver_reversing());
    }

    #[test]
    fn invalid_controls_do_not_poison_projection_or_advance_the_race() {
        let track = Track::new();
        let mut race = Race::new(&track);
        for controls in [
            Controls {
                throttle: f32::NAN,
                ..Default::default()
            },
            Controls {
                steer: f32::INFINITY,
                ..Default::default()
            },
        ] {
            race.step(&track, controls, 1. / 120.);
        }
        assert_eq!(race.ticks, 0);
        assert_eq!(race.car.pose.position, track.points[0]);
        assert!(race.car.velocity.is_finite());
        assert!(race.last_comparison.is_none());
        race.step(&track, Controls::default(), 1. / 120.);
        assert_eq!(race.ticks, 1);
    }

    #[test]
    fn handbrake_retains_slip_and_boundary_impacts_preserve_a_slide() {
        let track = Track::new();
        let mut road = Car::new(&track);
        let mut drift = Car::new(&track);
        let forward = Vec2::from_angle(road.pose.angle);
        let side = vec2(-forward.y, forward.x);
        road.velocity = forward * 150. + side * 80.;
        drift.velocity = road.velocity;
        road.step(&track, Controls::default(), 1. / 120.);
        drift.step(
            &track,
            Controls {
                handbrake: true,
                ..Default::default()
            },
            1. / 120.,
        );
        assert!(drift.velocity.dot(side) > road.velocity.dot(side));
        road.pose.position = vec2(1072., 300.);
        road.pose.angle = 0.;
        road.velocity = vec2(200., 30.);
        road.step(&track, Controls::default(), 1. / 120.);
        assert!(road.velocity.x < 0. && road.velocity.y > 20.);
        assert_eq!(road.pose.position.x, 1072.);
    }

    #[test]
    fn shortcut_and_reverse_cannot_score_laps() {
        let track = Track::new();
        let (p, tangent) = track.gate(0);
        assert!(track.crossed(0, p - tangent * 5., p + tangent * 5.));
        assert!(!track.crossed(0, p + tangent * 5., p - tangent * 5.));
        let normal = vec2(-tangent.y, tangent.x) * 100.;
        assert!(!track.crossed(0, p + normal - tangent * 5., p + normal + tangent * 5.));
        let mut race = Race::new(&track);
        race.car.pose.position = p - tangent;
        race.car.velocity = tangent * 240.;
        race.step(&track, Controls::default(), 1. / 120.);
        assert_eq!(race.laps, 0);
        assert_eq!(race.next_gate, 1);
    }
    #[test]
    fn checkpoints_must_be_crossed_in_order() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let (p, tangent) = track.gate(1);
        for expected in [2, 2] {
            race.car.pose.position = p - tangent;
            race.car.pose.angle = tangent.y.atan2(tangent.x);
            race.car.velocity = tangent * 240.;
            race.step(&track, Controls::default(), 1. / 120.);
            assert_eq!(race.next_gate, expected);
            assert_eq!(race.laps, 0);
        }
    }
    #[test]
    fn hud_progress_requires_every_gate_and_resets_only_after_a_valid_lap() {
        let track = Track::new();
        let mut race = Race::new(&track);
        assert_eq!(race.checkpoints_passed(), 0);
        for lap in 1..=LAPS {
            for gate in (1..GATES).chain(std::iter::once(0)) {
                let (p, tangent) = track.gate(gate);
                race.car.pose.position = p - tangent;
                race.car.pose.angle = tangent.y.atan2(tangent.x);
                race.car.velocity = tangent * 240.;
                race.step(&track, Controls::default(), 1. / 120.);
                if gate == 0 {
                    assert_eq!(race.laps, lap);
                    assert_eq!(
                        race.checkpoints_passed(),
                        if lap == LAPS { GATES } else { 0 }
                    );
                } else {
                    assert_eq!(race.checkpoints_passed(), gate);
                    assert_eq!(race.laps, lap - 1);
                }
            }
        }
    }
    #[test]
    fn real_controls_complete_three_laps_and_preserve_best() {
        let track = Track::new();
        let mut race = Race::new(&track);
        for _ in 0..24_000 {
            let controls = race.autopilot(&track);
            race.step(&track, controls, 1. / 120.);
            if race.finished() {
                break;
            }
        }
        assert!(
            race.finished(),
            "laps {}, next gate {}, position {:?}",
            race.laps,
            race.next_gate,
            race.car.pose.position
        );
        assert!(race.total_time > 25. && race.total_time < 120.);
        assert!(race.best_time.is_some());
        assert!(!race.best_ghost.is_empty());
        let time = race.total_time;
        race.step(
            &track,
            Controls {
                throttle: 1.,
                ..Controls::default()
            },
            1. / 120.,
        );
        assert_eq!(time, race.total_time);
        let best = race.best_time;
        race.reset(&track);
        assert_eq!(race.laps, 0);
        assert_eq!(race.best_time, best);
    }
    #[test]
    fn offroad_slows_car_and_inputs_remain_bounded() {
        let track = Track::new();
        let mut car = Car::new(&track);
        car.pose.position = vec2(500., 280.);
        car.velocity = Vec2::from_angle(car.pose.angle) * 200.;
        car.step(
            &track,
            Controls {
                throttle: 1.,
                steer: 10.,
                handbrake: false,
            },
            1. / 120.,
        );
        assert!(car.offroad);
        assert!(car.velocity.length() < 200.);
        for _ in 0..10_000 {
            car.step(
                &track,
                Controls {
                    throttle: 10.,
                    steer: -10.,
                    handbrake: false,
                },
                1. / 120.,
            );
        }
        assert!(car.pose.position.is_finite());
        assert!(car.velocity.length() < 300.);
    }
}

#[cfg(test)]
mod tuning_tests {
    use super::*;
    #[test]
    fn shared_profile_changes_real_car_acceleration_without_partial_edits() {
        let track = Track::new();
        let mut normal = Car::new(&track);
        let mut slow = Car::new(&track);
        let mut profile = ferravex_play::tuning::TuningProfile::rally();
        profile.motion.acceleration *= 0.5;
        slow.set_tuning(profile).unwrap();
        for _ in 0..60 {
            let input = Controls {
                throttle: 1.,
                ..Default::default()
            };
            normal.step(&track, input, 1. / 120.);
            slow.step(&track, input, 1. / 120.);
        }
        assert!(slow.velocity.length() < normal.velocity.length() * 0.7);
        let before = slow.tuning;
        assert!(
            slow.set_tuning(ferravex_play::tuning::TuningProfile::arena())
                .is_err()
        );
        assert_eq!(slow.tuning, before);
    }
}
