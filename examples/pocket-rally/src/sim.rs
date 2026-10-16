use ferravex_render::prelude::*;

pub const ROAD_HALF: f32 = 33.0;
pub const LAPS: u32 = 3;
const SAMPLES: usize = 192;
const GATES: usize = 16;

pub struct Track {
    pub points: Vec<Vec2>,
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
        let points = (0..SAMPLES)
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
        Self { points }
    }
    pub fn tangent(&self, index: usize) -> Vec2 {
        (self.points[(index + 1) % SAMPLES] - self.points[(index + SAMPLES - 1) % SAMPLES])
            .normalize()
    }
    pub fn nearest(&self, p: Vec2) -> (usize, f32) {
        let mut best = (0, f32::MAX);
        for i in 0..SAMPLES {
            let a = self.points[i];
            let ab = self.points[(i + 1) % SAMPLES] - a;
            let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0., 1.);
            let distance = p.distance(a + ab * t);
            if distance < best.1 {
                best = (i, distance);
            }
        }
        best
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
        }
    }
    fn step(&mut self, track: &Track, control: Controls, dt: f32) {
        self.offroad = track.nearest(self.pose.position).1 > ROAD_HALF;
        let forward = Vec2::from_angle(self.pose.angle);
        let right = vec2(-forward.y, forward.x);
        let mut longitudinal = self.velocity.dot(forward);
        let lateral = self.velocity.dot(right);
        let throttle = control.throttle.clamp(-1., 1.);
        let force = if throttle < 0. && longitudinal > 5. {
            260.
        } else {
            155.
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
        longitudinal = longitudinal.clamp(-65., 265.);
        let grip = if control.handbrake {
            1.5
        } else if self.offroad {
            5.
        } else {
            9.
        };
        self.sliding = lateral.abs() > 30. || control.handbrake && longitudinal > 30.;
        self.velocity = forward * longitudinal + right * lateral * (-grip * dt).exp();
        self.steering += (control.steer.clamp(-1., 1.) - self.steering)
            * ferravex_play::motion::response(14., dt);
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
    pub best_ghost: Vec<Pose>,
    recording: Vec<Pose>,
    ghost_track: Option<ferravex_play::replay::ghost::GhostTrack>,
    lap_ticks: u64,
    pub ticks: u64,
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
            best_ghost: Vec::new(),
            recording: vec![Car::new(track).pose],
            ghost_track: None,
            lap_ticks: 0,
            ticks: 0,
        }
    }
    pub fn finished(&self) -> bool {
        self.laps >= LAPS
    }
    pub fn reset(&mut self, track: &Track) {
        let ghost = std::mem::take(&mut self.best_ghost);
        let best = self.best_time;
        let ghost_track = self.ghost_track.take();
        *self = Self::new(track);
        self.best_ghost = ghost;
        self.best_time = best;
        self.ghost_track = ghost_track;
    }
    pub fn step(&mut self, track: &Track, controls: Controls, dt: f32) {
        if self.finished() || !dt.is_finite() || dt <= 0. {
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
        let time = self.lap_time * 20.;
        let sample = self
            .ghost_track
            .as_ref()?
            .sample(time.floor() as u64, time.fract())?;
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
    /// Pure pursuit driver feeds the exact same controls and vehicle model as a human.
    pub fn autopilot(&self, track: &Track) -> Controls {
        let (nearest, _) = track.nearest(self.car.pose.position);
        let speed = self.car.velocity.length();
        let lookahead = 42. + speed * 0.26;
        let mut target = nearest;
        let mut distance = 0.;
        while distance < lookahead {
            let next = (target + 1) % SAMPLES;
            distance += track.points[target].distance(track.points[next]);
            target = next;
        }
        let direction = track.points[target] - self.car.pose.position;
        let error = (direction.y.atan2(direction.x) - self.car.pose.angle + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let target_speed = 180. - error.abs().min(1.2) * 76.;
        Controls {
            throttle: ((target_speed - speed) / 20.).clamp(-1., 1.),
            steer: (error * 2.).clamp(-1., 1.),
            handbrake: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn real_controls_complete_three_laps_and_preserve_best() {
        let track = Track::new();
        let mut race = Race::new(&track);
        for _ in 0..24_000 {
            race.step(&track, race.autopilot(&track), 1. / 120.);
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
