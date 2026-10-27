//! Hover dynamics and ordered race progress, independent of the renderer/event loop.
use ferravex_play::navigation::Route3;
use ferravex_play::vehicle::{DriveInput, PlanarHandling, PlanarState, PlanarTelemetry};
use ferravex_render::prelude::{Vec2, Vec3, vec2, vec3};
use std::f32::consts::{PI, TAU};

pub const GATES: usize = 14;
pub const LAPS: u32 = 3;
pub const HALF_WIDTH: f32 = 6.8;
pub const HOVER_HEIGHT: f32 = 1.45;
const PATH_SAMPLES: usize = 192;
const BOOST_EMPTY: f32 = 0.03;
const BOOST_REARM: f32 = 0.25;
const RECOVERY_DELAY: f32 = 2.5;
pub const RECOVERY_PENALTY: f32 = 3.0;
/// A gate crossed this close to its center line is "threaded".
pub const THREAD_RADIUS: f32 = 2.2;
/// Threaded-gate refill grows with the streak up to this multiplier.
pub const MULTIPLIER_CAP: u32 = 5;
/// Clearance (beyond contact) that counts as a close scrape past a rock.
pub const SCRAPE_MARGIN: f32 = 1.6;
/// Minimum speed for a scrape to count, in meters per second.
pub const SCRAPE_SPEED: f32 = 20.0;
pub const SCRAPE_REFILL: f32 = 0.06;
/// Throttle first pressed at or below this countdown value earns a rocket start.
pub const ROCKET_WINDOW: f32 = 0.9;
pub const ROCKET_SPEED: f32 = 16.0;
/// Accepted events kept between host drains; older runs without a host stay bounded.
pub const MAX_EVENTS: usize = 32;

/// Accepted simulation events. Presentation and audio react only to these (or
/// to counters), so feedback never invents gameplay and replays read the same.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RaceEvent {
    /// The countdown expired. `rocket` is a well-timed launch; `eager` means the
    /// throttle was already held before the launch window.
    Go {
        rocket: bool,
        eager: bool,
    },
    Gate {
        gate: usize,
        threaded: bool,
        streak: u32,
        refill: f32,
    },
    /// A threaded streak of at least two ended (clipped gate, impact or recovery).
    StreakLost {
        streak: u32,
    },
    /// `lap` is 1-based; `time` includes any recovery penalties in that lap.
    Lap {
        lap: u32,
        time: f32,
        perfect: bool,
    },
    Finished,
    Scrape {
        at: Vec2,
        refill: f32,
    },
    Impact {
        at: Vec2,
        speed: f32,
    },
    BoostIgnited,
    Recovered,
}

/// The threaded-gate boost refill for a streak that includes this gate.
pub fn thread_refill(streak: u32) -> f32 {
    0.10 + 0.02 * (streak.clamp(1, MULTIPLIER_CAP) - 1) as f32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Traction {
    Track,
    Sliding,
    Reversing,
    Sand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoostStatus {
    Countdown,
    Ready,
    Active,
    Overheated,
    Sand,
    Finished,
}

pub fn path(t: f32) -> Vec2 {
    vec2(64.0 * t.cos() + 8.0 * (2.0 * t).sin(), 45.0 * t.sin())
}
pub fn tangent(t: f32) -> Vec2 {
    vec2(-64.0 * t.sin() + 16.0 * (2.0 * t).cos(), 45.0 * t.cos()).normalize()
}
pub fn right(dir: Vec2) -> Vec2 {
    vec2(dir.y, -dir.x)
}
pub fn world(p: Vec2, y: f32) -> Vec3 {
    vec3(p.x, y, p.y)
}
pub fn heading(dir: Vec2) -> f32 {
    dir.x.atan2(dir.y)
}
pub fn angle_difference(target: f32, current: f32) -> f32 {
    (target - current + PI).rem_euclid(TAU) - PI
}

#[derive(Clone, Copy)]
pub struct Gate {
    pub position: Vec2,
    pub direction: Vec2,
}
impl Gate {
    /// Test the swept vehicle center against the forward plane and finite aperture.
    /// A distant crossing of the plane or driving backwards never awards progress.
    pub fn crossed(self, previous: Vec3, current: Vec3) -> bool {
        let a = vec2(previous.x, previous.z) - self.position;
        let b = vec2(current.x, current.z) - self.position;
        let from = a.dot(self.direction);
        let to = b.dot(self.direction);
        if from >= 0.0 || to < 0.0 {
            return false;
        }
        let fraction = -from / (to - from);
        let intersection = a.lerp(b, fraction);
        let height = previous.y + (current.y - previous.y) * fraction;
        intersection.dot(right(self.direction)).abs() < HALF_WIDTH - 0.7
            && (0.5..5.4).contains(&height)
    }
}

pub struct Course {
    pub gates: [Gate; GATES],
    pub rocks: Vec<(Vec2, f32)>,
    route: Route3,
}
impl Course {
    pub fn new() -> Self {
        let mut route = Route3::new(PATH_SAMPLES).expect("bounded authored circuit");
        let samples: [_; PATH_SAMPLES] =
            std::array::from_fn(|i| world(path(i as f32 / PATH_SAMPLES as f32 * TAU), 0.));
        route.set_closed(&samples).expect("finite authored circuit");
        Self {
            gates: std::array::from_fn(|i| {
                let t = i as f32 / GATES as f32 * TAU;
                Gate {
                    position: path(t),
                    direction: tangent(t),
                }
            }),
            rocks: (0..18)
                .map(|i| {
                    let t = (i as f32 + 0.43) / 18.0 * TAU;
                    let side = if i % 2 == 0 { -1.0 } else { 1.0 };
                    (
                        path(t) + right(tangent(t)) * side * 6.8,
                        1.1 + (i % 3) as f32 * 0.24,
                    )
                })
                .collect(),
            route,
        }
    }
    /// Nearest point on the finite closed track polyline, not an infinite roadway.
    pub fn nearest(&self, point: Vec2) -> (f32, f32) {
        let projection = self
            .route
            .project(world(point, 0.))
            .expect("finite vehicle position")
            .expect("nonempty authored circuit");
        // Autoplay follows the authored curve parameter, not normalized arc length.
        // Projection remains advisory; only ordered forward gate crossings score.
        (
            projection.distance_to_route as f32,
            (projection.segment as f32 + projection.segment_fraction as f32) / PATH_SAMPLES as f32
                * TAU,
        )
    }
}

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub throttle: f32,
    pub steer: f32,
    pub boost: bool,
}

pub struct Race {
    pub position: Vec2,
    pub previous_position: Vec2,
    pub velocity: Vec2,
    pub yaw: f32,
    pub previous_yaw: f32,
    pub boost: f32,
    pub boosting: bool,
    pub next_gate: usize,
    pub laps: u32,
    pub elapsed: f32,
    pub countdown: f32,
    pub finished: bool,
    pub off_track: bool,
    pub impacts: u32,
    pub last_gate_timer: f32,
    pub bank: f32,
    pub recovery_count: u32,
    pub recovery_timer: f32,
    /// Consecutive threaded gates; clipping a gate, an impact or recovery ends it.
    pub streak: u32,
    pub best_streak: u32,
    /// Threaded gates across the whole race.
    pub threaded: u32,
    pub scrapes: u32,
    /// Laps in which all fourteen gates were threaded (the "mirage lap").
    pub perfect_laps: u32,
    /// Completed lap times; zero until that lap is complete.
    pub lap_times: [f32; LAPS as usize],
    pub rocket_start: bool,
    boost_exhausted: bool,
    impact_cooldown: f32,
    stranded_time: f32,
    turn_rate: f32,
    lap_start: f32,
    lap_threads: u32,
    /// Rock currently within scrape range, and whether that pass is still clean.
    scrape: Option<(usize, bool)>,
    /// Countdown value when the currently held throttle was first pressed.
    start_press: Option<f32>,
    events: Vec<RaceEvent>,
}
impl Race {
    pub fn new() -> Self {
        let start = path(0.0) + tangent(0.0) * 0.2;
        let yaw = heading(tangent(0.0));
        Self {
            position: start,
            previous_position: start,
            velocity: Vec2::ZERO,
            yaw,
            previous_yaw: yaw,
            boost: 1.0,
            boosting: false,
            next_gate: 1,
            laps: 0,
            elapsed: 0.0,
            countdown: 2.8,
            finished: false,
            off_track: false,
            impacts: 0,
            last_gate_timer: 0.0,
            bank: 0.0,
            recovery_count: 0,
            recovery_timer: 0.0,
            streak: 0,
            best_streak: 0,
            threaded: 0,
            scrapes: 0,
            perfect_laps: 0,
            lap_times: [0.; LAPS as usize],
            rocket_start: false,
            boost_exhausted: false,
            impact_cooldown: 0.0,
            stranded_time: 0.0,
            turn_rate: 0.,
            lap_start: 0.,
            lap_threads: 0,
            scrape: None,
            start_press: None,
            events: Vec::with_capacity(MAX_EVENTS),
        }
    }
    /// Take every accepted event since the previous drain, oldest first.
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, RaceEvent> {
        self.events.drain(..)
    }
    fn emit(&mut self, event: RaceEvent) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(event);
        }
    }
    fn break_streak(&mut self) {
        if self.streak >= 2 {
            self.emit(RaceEvent::StreakLost {
                streak: self.streak,
            });
        }
        self.streak = 0;
    }
    /// Elapsed time in the lap being driven (penalties included).
    pub fn lap_elapsed(&self) -> f32 {
        (self.elapsed - self.lap_start).max(0.)
    }
    /// Fastest completed lap, if any.
    pub fn best_lap(&self) -> Option<f32> {
        self.lap_times
            .iter()
            .copied()
            .filter(|&t| t > 0.)
            .min_by(f32::total_cmp)
    }
    /// Gates of the current lap that were threaded, as a bit per gate index.
    /// Bits are only meaningful for gates already earned this lap.
    pub fn lap_threads(&self) -> u32 {
        self.lap_threads
    }
    pub fn forward(&self) -> Vec2 {
        vec2(self.yaw.sin(), self.yaw.cos())
    }
    pub fn speed(&self) -> f32 {
        self.velocity.length()
    }
    fn motor(&self) -> PlanarState {
        PlanarState {
            position: self.position,
            velocity: self.velocity,
            yaw: self.yaw,
            turn_rate: self.turn_rate,
        }
    }
    fn apply_motor(&mut self, motor: PlanarState) {
        self.position = motor.position;
        self.velocity = motor.velocity;
        self.yaw = motor.yaw;
        self.turn_rate = motor.turn_rate;
    }
    pub fn telemetry(&self) -> PlanarTelemetry {
        self.motor().telemetry().expect("finite skimmer telemetry")
    }
    pub fn traction(&self) -> Traction {
        let motion = self.telemetry();
        if self.off_track {
            Traction::Sand
        } else if motion.forward_speed < -1.0 {
            Traction::Reversing
        } else if motion.speed > 3.0 && motion.slip_angle.abs() > 0.25 {
            Traction::Sliding
        } else {
            Traction::Track
        }
    }
    /// Total added recovery time, already included in `elapsed`.
    pub fn penalty_seconds(&self) -> f32 {
        self.recovery_count as f32 * RECOVERY_PENALTY
    }
    pub fn driving_seconds(&self) -> f32 {
        (self.elapsed - self.penalty_seconds()).max(0.)
    }
    /// Cooling estimate to the strict rearm threshold; boost still needs throttle.
    pub fn cooling_seconds(&self) -> f32 {
        ((BOOST_REARM - self.boost) / 0.11).max(0.)
    }
    /// Gates earned across the entire trial, including the finish gate of each lap.
    pub fn gates_completed(&self) -> usize {
        (self.laps as usize * GATES + (self.next_gate + GATES - 1) % GATES)
            .min(LAPS as usize * GATES)
    }
    /// Explain boost availability using the same hysteresis as the next vehicle tick.
    pub fn boost_status(&self) -> BoostStatus {
        if self.finished {
            BoostStatus::Finished
        } else if self.countdown > 0.0 {
            BoostStatus::Countdown
        } else if self.boost < BOOST_EMPTY || (self.boost_exhausted && self.boost <= BOOST_REARM) {
            BoostStatus::Overheated
        } else if self.off_track {
            BoostStatus::Sand
        } else if self.boosting {
            BoostStatus::Active
        } else {
            BoostStatus::Ready
        }
    }
    /// Remaining off-course grace period; `None` means no recovery is pending.
    pub fn recovery_in(&self) -> Option<f32> {
        (self.stranded_time > 0.0).then(|| (RECOVERY_DELAY - self.stranded_time).max(0.0))
    }
    pub fn step(&mut self, course: &Course, input: Input, dt: f32) {
        // Invalid/zero ticks must not award gates, spend energy or alter history.
        if !dt.is_finite() || dt <= 0.0 || dt > 0.25 {
            return;
        }
        let input = if input.throttle.is_finite() && input.steer.is_finite() {
            input
        } else {
            Input::default()
        };
        self.previous_position = self.position;
        self.previous_yaw = self.yaw;
        if self.finished {
            self.boosting = false;
            return;
        }
        if self.countdown > 0.0 {
            // Launch timing reads the held throttle on actual ticks only.
            if input.throttle > 0.5 {
                self.start_press.get_or_insert(self.countdown);
            } else {
                self.start_press = None;
            }
            self.countdown = (self.countdown - dt).max(0.0);
            if self.countdown == 0.0 {
                let rocket = self.start_press.is_some_and(|at| at <= ROCKET_WINDOW);
                let eager = self.start_press.is_some() && !rocket;
                if rocket {
                    self.velocity = self.forward() * ROCKET_SPEED;
                    self.rocket_start = true;
                }
                self.start_press = None;
                self.emit(RaceEvent::Go { rocket, eager });
            }
            return;
        }
        self.elapsed += dt;
        self.last_gate_timer = (self.last_gate_timer - dt).max(0.0);
        self.recovery_timer = (self.recovery_timer - dt).max(0.0);
        self.impact_cooldown = (self.impact_cooldown - dt).max(0.0);
        let (distance, _) = course.nearest(self.position);
        self.off_track = distance > HALF_WIDTH;
        let throttle = input.throttle.clamp(-1.0, 1.0);
        let steer = input.steer.clamp(-1.0, 1.0);
        if self.boost < BOOST_EMPTY {
            self.boost_exhausted = true;
        }
        if self.boost > BOOST_REARM {
            self.boost_exhausted = false;
        }
        let was_boosting = self.boosting;
        self.boosting = input.boost && !self.boost_exhausted && throttle > 0.1 && !self.off_track;
        if self.boosting && !was_boosting {
            self.emit(RaceEvent::BoostIgnited);
        }
        self.boost = (self.boost + dt * if self.boosting { -0.26 } else { 0.11 }).clamp(0.0, 1.0);
        // Turn rate stays useful at a standstill. Lateral damping gives the craft
        // controlled slip, while sand outside the racing line costs momentum.
        let mut motor = self.motor();
        motor
            .step(
                PlanarHandling {
                    acceleration: if self.boosting { 33. } else { 19. },
                    braking: 30.,
                    drag: if self.off_track { 2.4 } else { 0.62 },
                    grip: 5.,
                    max_speed: if self.boosting {
                        44.
                    } else if self.off_track {
                        12.
                    } else {
                        30.
                    },
                    base_turn: 1.15,
                    speed_turn: 0.018,
                    turn_speed_cap: 32.,
                    response: 12.,
                },
                DriveInput {
                    throttle,
                    steer,
                    brake: 0.,
                },
                dt,
            )
            .unwrap();
        self.apply_motor(motor);
        self.bank +=
            (-steer * 0.30 * (self.speed() / 25.0).min(1.0) - self.bank) * (dt * 7.0).min(1.0);
        let mut touched_rock = None;
        for (index, &(center, radius)) in course.rocks.iter().enumerate() {
            if self.resolve_obstacle(center, radius) {
                touched_rock = Some(index);
            }
        }
        for gate in course.gates {
            for side in [-1.0, 1.0] {
                self.resolve_obstacle(
                    gate.position + right(gate.direction) * (HALF_WIDTH + 0.4) * side,
                    0.65,
                );
            }
        }
        self.track_scrape(course, touched_rock);
        if course.gates[self.next_gate].crossed(
            world(self.previous_position, HOVER_HEIGHT),
            world(self.position, HOVER_HEIGHT),
        ) {
            self.earn_gate(course);
        }
        // Recovery returns to the last earned gate, so it cannot skip checkpoints.
        // It costs race time, and keeps exploration/errors from stranding a player.
        self.stranded_time = if distance > 24.0 {
            self.stranded_time + dt
        } else {
            0.0
        };
        if !self.finished && (self.stranded_time > RECOVERY_DELAY || self.position.length() > 210.0)
        {
            self.recover(course);
        }
    }
    fn earn_gate(&mut self, course: &Course) {
        let gate = self.next_gate;
        // A centered pass feeds the next straight; clipping the opening still
        // counts, but earns no extra boost and ends a threaded streak.
        let threaded = (self.position - course.gates[gate].position)
            .dot(right(course.gates[gate].direction))
            .abs()
            < THREAD_RADIUS;
        let mut refill = 0.;
        if threaded {
            self.streak += 1;
            self.best_streak = self.best_streak.max(self.streak);
            self.threaded += 1;
            self.lap_threads |= 1 << gate;
            refill = thread_refill(self.streak);
            self.boost = (self.boost + refill).min(1.);
        } else {
            self.break_streak();
        }
        self.emit(RaceEvent::Gate {
            gate,
            threaded,
            streak: self.streak,
            refill,
        });
        if gate == 0 {
            self.laps += 1;
            let time = self.elapsed - self.lap_start;
            self.lap_times[(self.laps - 1) as usize] = time;
            self.lap_start = self.elapsed;
            let perfect = self.lap_threads == (1 << GATES) - 1;
            self.lap_threads = 0;
            self.finished = self.laps == LAPS;
            if perfect {
                self.perfect_laps += 1;
                // The mirage lap bonus: a full tank for the next one.
                self.boost = 1.;
                self.boost_exhausted = false;
            }
            self.emit(RaceEvent::Lap {
                lap: self.laps,
                time,
                perfect,
            });
            if self.finished {
                self.velocity = Vec2::ZERO;
                self.boosting = false;
                self.emit(RaceEvent::Finished);
            }
        }
        self.next_gate = (gate + 1) % GATES;
        self.last_gate_timer = 0.8;
    }
    /// Award a scrape when the craft leaves a rock's near zone at speed without
    /// having touched it. Contact during the pass voids it.
    fn track_scrape(&mut self, course: &Course, touched: Option<usize>) {
        let near = course.rocks.iter().position(|&(center, radius)| {
            self.position.distance_squared(center) < (radius + 0.8 + SCRAPE_MARGIN).powi(2)
        });
        let previous = self.scrape;
        self.scrape = near.map(|index| {
            let still_clean = match previous {
                Some((old, clean)) if old == index => clean,
                _ => true,
            };
            (index, still_clean && touched != Some(index))
        });
        if let Some((index, true)) = previous
            && near != Some(index)
            && self.speed() >= SCRAPE_SPEED
            && !self.finished
        {
            self.scrapes += 1;
            self.boost = (self.boost + SCRAPE_REFILL).min(1.);
            self.emit(RaceEvent::Scrape {
                at: course.rocks[index].0,
                refill: SCRAPE_REFILL,
            });
        }
    }
    /// Push the craft out of a circular obstacle. Returns whether it touched.
    fn resolve_obstacle(&mut self, center: Vec2, radius: f32) -> bool {
        let delta = self.position - center;
        let minimum = radius + 0.8;
        let touched = delta.length_squared() < minimum * minimum;
        if touched {
            let normal = if delta.length_squared() > 0.0001 {
                delta.normalize()
            } else {
                Vec2::X
            };
            self.position = center + normal * minimum;
            let into = self.velocity.dot(normal);
            if into < 0.0 {
                self.velocity = ferravex_play::motion::impact_velocity(
                    self.velocity.extend(0.),
                    normal.extend(0.),
                    0.25,
                    0.93,
                )
                .expect("finite skimmer collision")
                .truncate();
                if self.impact_cooldown <= 0.0 {
                    self.impacts += 1;
                    self.impact_cooldown = 0.7;
                    self.emit(RaceEvent::Impact {
                        at: center + normal * radius,
                        speed: -into,
                    });
                    self.break_streak();
                }
            }
        }
        touched
    }
    fn recover(&mut self, course: &Course) {
        let gate = course.gates[(self.next_gate + GATES - 1) % GATES];
        let mut motor = self.motor();
        motor
            .reset(
                gate.position + gate.direction,
                heading(gate.direction),
                Vec2::ZERO,
            )
            .expect("finite authored recovery anchor");
        self.apply_motor(motor);
        self.previous_position = self.position;
        self.previous_yaw = self.yaw;
        self.elapsed += RECOVERY_PENALTY;
        self.recovery_count += 1;
        self.recovery_timer = 2.4;
        self.off_track = false;
        self.boosting = false;
        self.last_gate_timer = 0.0;
        self.impact_cooldown = 0.0;
        self.stranded_time = 0.0;
        self.bank = 0.;
        self.scrape = None;
        self.break_streak();
        self.emit(RaceEvent::Recovered);
    }
    /// Pure pursuit drives exactly the same throttle/steer/boost path as a player.
    pub fn autoplay(&self, course: &Course) -> Input {
        let (_, t) = course.nearest(self.position);
        let target = path(t + 0.13 + self.speed() * 0.0012);
        let angle = angle_difference(
            heading((target - self.position).normalize_or_zero()),
            self.yaw,
        );
        Input {
            throttle: if angle.abs() > 1.1 { 0.1 } else { 1.0 },
            steer: (angle * 2.8).clamp(-1.0, 1.0),
            boost: angle.abs() < 0.20
                && (self.boost > 0.60 || (self.boosting && self.boost > 0.10)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_route_preserves_authored_projection_including_closing_segment() {
        let course = Course::new();
        let samples: [_; PATH_SAMPLES] =
            std::array::from_fn(|i| path(i as f32 / PATH_SAMPLES as f32 * TAU));
        // Independent baseline projection guards the authored autoplay parameter:
        // substituting normalized arc progress would silently retune pursuit.
        for i in 0..97 {
            let t = i as f32 / 97. * TAU;
            for offset in [-40., -HALF_WIDTH, 0., HALF_WIDTH, 40.] {
                let point = path(t) + right(tangent(t)) * offset;
                let mut expected = (f32::MAX, 0.);
                for j in 0..PATH_SAMPLES {
                    let a = samples[j];
                    let b = samples[(j + 1) % PATH_SAMPLES];
                    let fraction =
                        ((point - a).dot(b - a) / (b - a).length_squared()).clamp(0., 1.);
                    let distance = point.distance_squared(a.lerp(b, fraction));
                    if distance < expected.0 {
                        expected = (distance, (j as f32 + fraction) / PATH_SAMPLES as f32 * TAU);
                    }
                }
                let actual = course.nearest(point);
                assert!((actual.0 - expected.0.sqrt()).abs() < 0.0001);
                assert!(angle_difference(actual.1, expected.1).abs() < 0.0001);
            }
        }
    }

    #[test]
    fn traction_uses_signed_vehicle_telemetry_and_sand_has_priority() {
        let mut race = Race::new();
        race.yaw = 0.;
        assert_eq!(race.traction(), Traction::Track);
        race.velocity = vec2(8., 10.);
        assert_eq!(race.traction(), Traction::Sliding);
        assert_eq!(race.telemetry().lateral_speed, 8.);
        race.velocity = vec2(0., -10.);
        assert_eq!(race.traction(), Traction::Reversing);
        race.off_track = true;
        assert_eq!(race.traction(), Traction::Sand);
        race.off_track = false;
        race.velocity = vec2(0.2, 0.);
        assert_eq!(race.traction(), Traction::Track);
    }

    #[test]
    fn every_recovery_anchor_is_clear_and_preserves_earned_progress() {
        let course = Course::new();
        for next_gate in 0..GATES {
            let mut race = Race::new();
            race.next_gate = next_gate;
            race.laps = 2;
            race.velocity = vec2(12., -5.);
            race.turn_rate = 0.7;
            race.bank = 0.2;
            let progress = race.gates_completed();
            race.recover(&course);
            assert_eq!(race.gates_completed(), progress);
            assert_eq!(race.telemetry().turn_rate, 0.);
            assert_eq!(race.telemetry().speed, 0.);
            assert_eq!(race.previous_position, race.position);
            assert_eq!(race.previous_yaw, race.yaw);
            assert_eq!(race.bank, 0.);
            assert!(course.nearest(race.position).0 < HALF_WIDTH);
            for &(center, radius) in &course.rocks {
                assert!(race.position.distance(center) > radius + 0.8);
            }
            for gate in course.gates {
                for side in [-1., 1.] {
                    let post = gate.position + right(gate.direction) * (HALF_WIDTH + 0.4) * side;
                    assert!(race.position.distance(post) > 0.65 + 0.8);
                }
            }
        }
    }

    #[test]
    fn recovery_penalties_are_included_once_and_impacts_only_cost_speed() {
        let course = Course::new();
        let mut race = Race::new();
        race.elapsed = 10.;
        race.position = Vec2::X;
        race.velocity = -Vec2::X * 20.;
        race.resolve_obstacle(Vec2::ZERO, 1.);
        assert_eq!(race.impacts, 1);
        assert_eq!(race.elapsed, 10.);
        assert_eq!(race.penalty_seconds(), 0.);
        for recovery in 1..=2 {
            race.recover(&course);
            assert_eq!(race.penalty_seconds(), recovery as f32 * 3.);
            assert_eq!(race.driving_seconds(), 10.);
            assert_eq!(
                race.elapsed,
                race.driving_seconds() + race.penalty_seconds()
            );
        }
        race.finished = true;
        race.step(&course, Input::default(), 1. / 120.);
        assert_eq!(race.elapsed, 16.);
        assert_eq!(race.penalty_seconds(), 6.);
    }

    #[test]
    fn rejected_ticks_preserve_history_and_invalid_controls_coast_safely() {
        let course = Course::new();
        let mut race = Race::new();
        race.countdown = 0.;
        race.previous_position = race.position - Vec2::X;
        let previous = race.previous_position;
        let telemetry = race.telemetry();
        for dt in [0., -0.1, 0.251, f32::INFINITY, f32::NAN] {
            race.step(
                &course,
                Input {
                    throttle: 1.,
                    steer: 1.,
                    boost: true,
                },
                dt,
            );
            assert_eq!(race.previous_position, previous);
            assert_eq!(race.telemetry(), telemetry);
            assert_eq!(race.elapsed, 0.);
            assert_eq!(race.boost, 1.);
        }
        for (throttle, steer) in [(f32::NAN, 1.), (1., f32::INFINITY)] {
            race.step(
                &course,
                Input {
                    throttle,
                    steer,
                    boost: true,
                },
                1. / 120.,
            );
            assert_eq!(race.telemetry(), telemetry);
            assert!(!race.boosting);
        }
        assert!(race.elapsed > 0.);
    }

    #[test]
    fn boost_feedback_matches_overheat_lockout_and_rearm_threshold() {
        let course = Course::new();
        let mut race = Race::new();
        assert_eq!(race.boost_status(), BoostStatus::Countdown);
        race.countdown = 0.0;
        assert_eq!(race.boost_status(), BoostStatus::Ready);
        race.boost = 0.0;
        let input = Input {
            throttle: 1.0,
            boost: true,
            ..Default::default()
        };
        for _ in 0..120 {
            // Keep this a straight-line energy test, clear of course obstacles.
            race.position = Race::new().position;
            race.velocity = Vec2::ZERO;
            race.step(&course, input, 1.0 / 120.0);
            assert!(!race.boosting);
            assert_eq!(race.boost_status(), BoostStatus::Overheated);
        }
        race.boost = BOOST_REARM;
        assert_eq!(race.boost_status(), BoostStatus::Overheated);
        race.step(&course, input, 1.0 / 120.0);
        assert!(!race.boosting);
        assert_eq!(race.boost_status(), BoostStatus::Ready);
        race.step(&course, input, 1.0 / 120.0);
        assert!(race.boosting);
        assert_eq!(race.boost_status(), BoostStatus::Active);
        race.position = Vec2::ZERO;
        race.step(&course, input, 1.0 / 120.0);
        assert!(!race.boosting);
        assert_eq!(race.boost_status(), BoostStatus::Sand);
        race.finished = true;
        assert_eq!(race.boost_status(), BoostStatus::Finished);
    }

    #[test]
    fn progress_counts_finish_gate_and_remains_monotonic_across_laps() {
        let course = Course::new();
        let mut race = Race::new();
        let mut completed = 0;
        assert_eq!(race.gates_completed(), 0);
        for _ in 0..120 * 120 {
            race.step(&course, race.autoplay(&course), 1.0 / 120.0);
            let next = race.gates_completed();
            assert!(next == completed || next == completed + 1);
            assert!(next <= GATES * LAPS as usize);
            completed = next;
            if race.finished {
                break;
            }
        }
        assert_eq!(completed, GATES * LAPS as usize);
    }

    #[test]
    fn recovery_has_grace_period_penalty_and_clears_transient_vehicle_state() {
        let course = Course::new();
        let mut race = Race::new();
        race.countdown = 0.0;
        race.next_gate = 5;
        race.position = Vec2::ZERO;
        for _ in 0..120 {
            race.step(&course, Input::default(), 1.0 / 120.0);
        }
        assert_eq!(race.recovery_count, 0);
        assert!((race.recovery_in().unwrap() - 1.5).abs() < 0.001);
        race.boosting = true;
        race.bank = 0.2;
        race.last_gate_timer = 0.5;
        let elapsed = race.elapsed;
        race.recover(&course);
        assert!((race.elapsed - elapsed - 3.0).abs() < 0.0001);
        assert_eq!(race.recovery_count, 1);
        assert_eq!(race.recovery_in(), None);
        assert!(race.recovery_timer > 2.0);
        assert!(!race.off_track && !race.boosting);
        assert_eq!(race.bank, 0.0);
        assert_eq!(race.last_gate_timer, 0.0);
        assert_eq!(race.next_gate, 5);
        assert_eq!(race.laps, 0);
        assert_eq!(race.position, race.previous_position);
        assert_eq!(race.yaw, race.previous_yaw);
        assert!(race.position.distance(course.gates[4].position) < 1.01);
        for _ in 0..300 {
            race.step(&course, Input::default(), 1.0 / 120.0);
        }
        assert_eq!(race.recovery_timer, 0.0);
        assert_eq!(race.recovery_count, 1);
    }

    #[test]
    fn glancing_impacts_keep_tangential_speed_and_turning_has_inertia() {
        let mut race = Race::new();
        race.position = Vec2::X;
        race.velocity = vec2(-10., 20.);
        race.resolve_obstacle(Vec2::ZERO, 1.);
        assert!(race.velocity.x > 0. && race.velocity.y > 18.);
        let course = Course::new();
        race = Race::new();
        race.countdown = 0.;
        race.step(
            &course,
            Input {
                steer: 1.,
                ..Default::default()
            },
            1. / 120.,
        );
        assert!(race.turn_rate > 0. && race.turn_rate < 0.3);
    }

    #[test]
    fn autoplay_completes_three_laps_using_real_controls() {
        let course = Course::new();
        let mut race = Race::new();
        let mut gates = 0;
        for _ in 0..120 * 120 {
            let old = race.next_gate;
            race.step(&course, race.autoplay(&course), 1.0 / 120.0);
            gates += u32::from(old != race.next_gate);
            assert!(race.position.is_finite() && race.velocity.is_finite());
            assert!((0.0..=1.0).contains(&race.boost));
            if race.finished {
                break;
            }
        }
        assert!(
            race.finished,
            "lap={} gate={} time={} position={:?}",
            race.laps, race.next_gate, race.elapsed, race.position
        );
        assert_eq!(race.laps, LAPS);
        assert_eq!(gates, GATES as u32 * LAPS);
        assert_eq!(race.recovery_count, 0);
    }
    #[test]
    fn gates_require_forward_crossing_inside_three_dimensional_aperture() {
        let gate = Gate {
            position: Vec2::ZERO,
            direction: Vec2::Y,
        };
        assert!(gate.crossed(vec3(0.0, 1.5, -2.0), vec3(0.0, 1.5, 2.0)));
        assert!(!gate.crossed(vec3(0.0, 1.5, 2.0), vec3(0.0, 1.5, -2.0)));
        assert!(!gate.crossed(vec3(10.0, 1.5, -2.0), vec3(10.0, 1.5, 2.0)));
        assert!(!gate.crossed(vec3(0.0, 8.0, -2.0), vec3(0.0, 8.0, 2.0)));
    }
    #[test]
    fn skipped_gates_and_recovery_never_award_a_lap() {
        let course = Course::new();
        let mut race = Race::new();
        race.countdown = 0.0;
        let gate = course.gates[0];
        race.position = gate.position - gate.direction * 0.05;
        race.velocity = gate.direction * 30.0;
        race.yaw = heading(gate.direction);
        race.step(&course, Input::default(), 1.0 / 120.0);
        assert_eq!(race.laps, 0);
        assert_eq!(race.next_gate, 1);
        race.position = vec2(500.0, 500.0);
        race.step(&course, Input::default(), 1.0 / 120.0);
        assert_eq!(race.recovery_count, 1);
        assert_eq!(race.next_gate, 1);
        assert!(race.position.distance(course.gates[0].position) < 2.0);
    }
    #[test]
    fn rocks_and_gate_posts_repel_the_vehicle_and_cost_speed() {
        let course = Course::new();
        let gate = course.gates[2];
        let post = gate.position + right(gate.direction) * (HALF_WIDTH + 0.4);
        for (center, radius) in [course.rocks[0], (post, 0.65)] {
            let mut race = Race::new();
            race.countdown = 0.0;
            race.position = center + Vec2::X * (radius + 0.85);
            race.velocity = -Vec2::X * 20.0;
            race.yaw = heading(-Vec2::X);
            race.step(&course, Input::default(), 1.0 / 120.0);
            assert!(race.position.distance(center) >= radius + 0.79);
            assert!(race.speed() < 15.0);
            assert_eq!(race.impacts, 1);
        }
    }

    #[test]
    fn sand_slows_vehicle_and_boost_recharges() {
        let course = Course::new();
        let mut race = Race::new();
        race.countdown = 0.0;
        race.position = Vec2::ZERO;
        race.velocity = Vec2::X * 30.0;
        race.boost = 0.0;
        for _ in 0..120 {
            race.step(&course, Input::default(), 1.0 / 120.0);
        }
        assert!(race.off_track);
        assert!(race.speed() < 3.0);
        assert!(race.boost > 0.10);
    }

    fn drive_gate(race: &mut Race, course: &Course, offset: f32) {
        // Cross the next gate at a lateral offset with ordinary ticks.
        let gate = course.gates[race.next_gate];
        race.position = gate.position - gate.direction * 0.05 + right(gate.direction) * offset;
        race.velocity = gate.direction * 25.0;
        race.yaw = heading(gate.direction);
        race.step(course, Input::default(), 1.0 / 120.0);
    }

    #[test]
    fn threaded_streak_scales_refill_and_clipping_or_impact_breaks_it() {
        let course = Course::new();
        let mut race = Race::new();
        race.countdown = 0.;
        race.boost = 0.;
        let mut refills = Vec::new();
        for _ in 0..6 {
            let before = race.boost;
            drive_gate(&mut race, &course, 0.5);
            refills.push(race.boost - before);
        }
        assert_eq!(race.streak, 6);
        assert_eq!(race.threaded, 6);
        for (i, refill) in refills.iter().enumerate() {
            let streak = i as u32 + 1;
            assert!(
                (refill - thread_refill(streak)).abs() < 0.001,
                "{i}: {refill}"
            );
        }
        assert!((thread_refill(5) - 0.18).abs() < 1e-6);
        assert_eq!(thread_refill(9), thread_refill(MULTIPLIER_CAP));
        let events: Vec<_> = race.drain_events().collect();
        assert!(matches!(
            events.last(),
            Some(RaceEvent::Gate {
                threaded: true,
                streak: 6,
                ..
            })
        ));
        // Clipping the opening still counts but ends the streak.
        let before = race.boost;
        drive_gate(&mut race, &course, 4.0);
        assert_eq!(race.gates_completed(), 7);
        assert_eq!((race.streak, race.best_streak), (0, 6));
        assert!(
            race.boost - before < 0.002,
            "only the ordinary recharge tick"
        );
        let events: Vec<_> = race.drain_events().collect();
        assert!(events.contains(&RaceEvent::StreakLost { streak: 6 }));
        // An impact also ends a running streak.
        drive_gate(&mut race, &course, 0.);
        drive_gate(&mut race, &course, 0.);
        race.drain_events().for_each(drop);
        race.position = Vec2::X * 300.;
        race.velocity = -Vec2::X * 20.;
        assert!(race.resolve_obstacle(Vec2::X * 299., 1.));
        assert_eq!(race.streak, 0);
        let events: Vec<_> = race.drain_events().collect();
        assert!(matches!(events[0], RaceEvent::Impact { .. }));
        assert_eq!(events[1], RaceEvent::StreakLost { streak: 2 });
    }

    #[test]
    fn a_fully_threaded_lap_is_perfect_refills_boost_and_records_the_lap_time() {
        let course = Course::new();
        let mut race = Race::new();
        race.countdown = 0.;
        for _ in 0..GATES {
            race.boost = 0.2;
            race.elapsed += 0.5;
            drive_gate(&mut race, &course, -1.0);
        }
        assert_eq!(race.laps, 1);
        assert_eq!(race.perfect_laps, 1);
        assert_eq!(race.boost, 1.);
        assert!((race.lap_times[0] - (GATES as f32 * 0.5 + GATES as f32 / 120.)).abs() < 0.001);
        assert_eq!(race.best_lap(), Some(race.lap_times[0]));
        assert!(race.drain_events().any(|e| matches!(
            e,
            RaceEvent::Lap {
                lap: 1,
                perfect: true,
                ..
            }
        )));
        // One clipped gate spoils the next lap's mirage.
        for i in 0..GATES {
            drive_gate(&mut race, &course, if i == 3 { 4.5 } else { 0. });
        }
        assert_eq!((race.laps, race.perfect_laps), (2, 1));
        assert!(race.drain_events().any(|e| matches!(
            e,
            RaceEvent::Lap {
                lap: 2,
                perfect: false,
                ..
            }
        )));
        assert!(race.lap_elapsed() < 0.001);
    }

    #[test]
    fn clean_close_pass_at_speed_scrapes_once_but_contact_or_crawling_does_not() {
        let course = Course::new();
        let (center, radius) = course.rocks[4];
        let pass = |clearance: f32, speed: f32| {
            let mut race = Race::new();
            race.countdown = 0.;
            race.boost = 0.5;
            // Fly a straight chord past the rock along the local tangent.
            let t = (4.43) / 18.0 * TAU;
            let along = tangent(t);
            let side = (center - path(t)).normalize();
            let offset = center - side * (radius + 0.8 + clearance);
            for step in -40..40 {
                race.previous_position = race.position;
                race.position = offset + along * (step as f32 * 0.25);
                race.velocity = along * speed;
                let touched = race.resolve_obstacle(center, radius).then_some(4);
                race.track_scrape(&course, touched);
            }
            race
        };
        let close = pass(0.8, 26.);
        assert_eq!(close.scrapes, 1);
        assert!((close.boost - 0.5 - SCRAPE_REFILL).abs() < 1e-6);
        assert_eq!(pass(0.8, 10.).scrapes, 0, "too slow");
        assert_eq!(pass(SCRAPE_MARGIN + 0.5, 26.).scrapes, 0, "too far");
        assert_eq!(pass(-0.3, 26.).scrapes, 0, "contact voids the pass");
    }

    #[test]
    fn launch_timing_rewards_a_late_press_and_ignores_an_eager_hold() {
        let course = Course::new();
        let run = |press_below: f32| {
            let mut race = Race::new();
            let mut events = Vec::new();
            while race.countdown > 0. {
                let throttle = if race.countdown <= press_below {
                    1.
                } else {
                    0.
                };
                race.step(
                    &course,
                    Input {
                        throttle,
                        ..Default::default()
                    },
                    1. / 120.,
                );
                events.extend(race.drain_events());
            }
            (race, events)
        };
        let (rocket, events) = run(0.4);
        assert!(rocket.rocket_start);
        assert!((rocket.speed() - ROCKET_SPEED).abs() < 1e-4);
        assert_eq!(
            rocket.position,
            Race::new().position,
            "launch never moves during the countdown"
        );
        assert_eq!(
            events,
            [RaceEvent::Go {
                rocket: true,
                eager: false
            }]
        );
        let (eager, events) = run(10.);
        assert!(!eager.rocket_start && eager.speed() == 0.);
        assert_eq!(
            events,
            [RaceEvent::Go {
                rocket: false,
                eager: true
            }]
        );
        let (idle, events) = run(-1.);
        assert_eq!(idle.speed(), 0.);
        assert_eq!(
            events,
            [RaceEvent::Go {
                rocket: false,
                eager: false
            }]
        );
    }

    #[test]
    fn events_are_bounded_without_a_host_and_drain_in_order() {
        let course = Course::new();
        let mut race = Race::new();
        race.countdown = 0.;
        for _ in 0..(MAX_EVENTS * 2) {
            drive_gate(&mut race, &course, 0.);
        }
        assert_eq!(race.drain_events().count(), MAX_EVENTS);
        assert_eq!(race.drain_events().count(), 0);
    }

    #[test]
    fn autoplay_reports_consistent_lap_times_and_threading_stats() {
        let course = Course::new();
        let mut race = Race::new();
        let mut gates = 0;
        let mut threaded = 0;
        for _ in 0..120 * 120 {
            race.step(&course, race.autoplay(&course), 1.0 / 120.0);
            for event in race.drain_events() {
                if let RaceEvent::Gate { threaded: t, .. } = event {
                    gates += 1;
                    threaded += u32::from(t);
                }
            }
            if race.finished {
                break;
            }
        }
        assert!(race.finished);
        assert_eq!(gates, GATES * LAPS as usize);
        assert_eq!(threaded, race.threaded);
        let total: f32 = race.lap_times.iter().sum();
        assert!((total - race.elapsed).abs() < 0.01);
        assert!(race.lap_times.iter().all(|&t| t > 0.));
        // The HUD quotes Dusty (autoplay) as the time to beat; keep it truthful.
        assert!(
            (race.elapsed - crate::quips::DUSTY).abs() < 0.01,
            "{}",
            race.elapsed
        );
        eprintln!(
            "autoplay: time={} laps={:?} threaded={} best_streak={} perfect={} scrapes={}",
            race.elapsed,
            race.lap_times,
            race.threaded,
            race.best_streak,
            race.perfect_laps,
            race.scrapes
        );
    }
}
