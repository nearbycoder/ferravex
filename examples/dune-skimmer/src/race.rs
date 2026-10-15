//! Hover dynamics and ordered race progress, independent of the renderer/event loop.
use ferravex_render::prelude::{Vec2, Vec3, vec2, vec3};
use std::f32::consts::{PI, TAU};

pub const GATES: usize = 14;
pub const LAPS: u32 = 3;
pub const HALF_WIDTH: f32 = 6.8;
pub const HOVER_HEIGHT: f32 = 1.45;
const PATH_SAMPLES: usize = 192;

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
    samples: [Vec2; PATH_SAMPLES],
}
impl Course {
    pub fn new() -> Self {
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
            samples: std::array::from_fn(|i| path(i as f32 / PATH_SAMPLES as f32 * TAU)),
        }
    }
    /// Nearest point on the finite closed track polyline, not an infinite roadway.
    pub fn nearest(&self, point: Vec2) -> (f32, f32) {
        let mut best = (f32::MAX, 0.0);
        for i in 0..PATH_SAMPLES {
            let a = self.samples[i];
            let b = self.samples[(i + 1) % PATH_SAMPLES];
            let fraction = ((point - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            let distance = point.distance_squared(a.lerp(b, fraction));
            if distance < best.0 {
                best = (distance, (i as f32 + fraction) / PATH_SAMPLES as f32 * TAU);
            }
        }
        (best.0.sqrt(), best.1)
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
    boost_exhausted: bool,
    impact_cooldown: f32,
    stranded_time: f32,
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
            boost_exhausted: false,
            impact_cooldown: 0.0,
            stranded_time: 0.0,
        }
    }
    pub fn forward(&self) -> Vec2 {
        vec2(self.yaw.sin(), self.yaw.cos())
    }
    pub fn speed(&self) -> f32 {
        self.velocity.length()
    }
    pub fn step(&mut self, course: &Course, input: Input, dt: f32) {
        self.previous_position = self.position;
        self.previous_yaw = self.yaw;
        if self.finished {
            self.boosting = false;
            return;
        }
        if self.countdown > 0.0 {
            self.countdown = (self.countdown - dt).max(0.0);
            return;
        }
        self.elapsed += dt;
        self.last_gate_timer = (self.last_gate_timer - dt).max(0.0);
        self.impact_cooldown = (self.impact_cooldown - dt).max(0.0);
        let (distance, _) = course.nearest(self.position);
        self.off_track = distance > HALF_WIDTH;
        let throttle = input.throttle.clamp(-1.0, 1.0);
        let steer = input.steer.clamp(-1.0, 1.0);
        if self.boost < 0.03 {
            self.boost_exhausted = true;
        }
        if self.boost > 0.25 {
            self.boost_exhausted = false;
        }
        self.boosting = input.boost && !self.boost_exhausted && throttle > 0.1 && !self.off_track;
        self.boost = (self.boost + dt * if self.boosting { -0.26 } else { 0.11 }).clamp(0.0, 1.0);
        // Turn rate stays useful at a standstill. Lateral damping gives the craft
        // controlled slip, while sand outside the racing line costs momentum.
        self.yaw =
            (self.yaw + steer * (1.15 + self.speed().min(32.0) * 0.018) * dt).rem_euclid(TAU);
        let forward = self.forward();
        let lateral = right(forward);
        let side_speed = self.velocity.dot(lateral);
        self.velocity -= lateral * side_speed * (1.0 - (-5.0 * dt).exp());
        let acceleration = if self.boosting { 33.0 } else { 19.0 };
        self.velocity += forward * throttle * acceleration * dt;
        self.velocity *= (-dt * if self.off_track { 2.4 } else { 0.62 }).exp();
        let cap = if self.boosting {
            44.0
        } else if self.off_track {
            12.0
        } else {
            30.0
        };
        self.velocity = self.velocity.clamp_length_max(cap);
        self.position += self.velocity * dt;
        self.bank +=
            (-steer * 0.30 * (self.speed() / 25.0).min(1.0) - self.bank) * (dt * 7.0).min(1.0);
        for &(center, radius) in &course.rocks {
            self.resolve_obstacle(center, radius);
        }
        for gate in course.gates {
            for side in [-1.0, 1.0] {
                self.resolve_obstacle(
                    gate.position + right(gate.direction) * (HALF_WIDTH + 0.4) * side,
                    0.65,
                );
            }
        }
        if course.gates[self.next_gate].crossed(
            world(self.previous_position, HOVER_HEIGHT),
            world(self.position, HOVER_HEIGHT),
        ) {
            if self.next_gate == 0 {
                self.laps += 1;
                self.finished = self.laps == LAPS;
                if self.finished {
                    self.velocity = Vec2::ZERO;
                    self.boosting = false;
                }
            }
            self.next_gate = (self.next_gate + 1) % GATES;
            self.last_gate_timer = 0.8;
        }
        // Recovery returns to the last earned gate, so it cannot skip checkpoints.
        // It costs race time, and keeps exploration/errors from stranding a player.
        self.stranded_time = if distance > 24.0 {
            self.stranded_time + dt
        } else {
            0.0
        };
        if self.stranded_time > 2.5 || self.position.length() > 210.0 {
            self.recover(course);
        }
    }
    fn resolve_obstacle(&mut self, center: Vec2, radius: f32) {
        let delta = self.position - center;
        let minimum = radius + 0.8;
        if delta.length_squared() < minimum * minimum {
            let normal = if delta.length_squared() > 0.0001 {
                delta.normalize()
            } else {
                Vec2::X
            };
            self.position = center + normal * minimum;
            let into = self.velocity.dot(normal);
            if into < 0.0 {
                self.velocity -= normal * into * 1.35;
                self.velocity *= 0.68;
                if self.impact_cooldown <= 0.0 {
                    self.impacts += 1;
                    self.impact_cooldown = 0.7;
                }
            }
        }
    }
    fn recover(&mut self, course: &Course) {
        let gate = course.gates[(self.next_gate + GATES - 1) % GATES];
        self.position = gate.position + gate.direction * 1.0;
        self.previous_position = self.position;
        self.yaw = heading(gate.direction);
        self.previous_yaw = self.yaw;
        self.velocity = Vec2::ZERO;
        self.elapsed += 3.0;
        self.recovery_count += 1;
        self.stranded_time = 0.0;
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
}
