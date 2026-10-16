//! Bounded polyline racing driver. Produces ordinary controls, never teleports or
//! awards progress. Hosts pass perceived nearby traffic, not hidden rival state.
use ferravex_math::Vec2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RacingError {
    Invalid,
}
#[derive(Clone, Copy, Debug)]
pub struct DriverSettings {
    pub cruise: f32,
    pub minimum: f32,
    pub lookahead: f32,
    pub lookahead_seconds: f32,
    pub corner_braking: f32,
    pub lateral_acceleration: f32,
    pub steer_gain: f32,
    pub speed_response: f32,
    pub half_width: f32,
}
impl Default for DriverSettings {
    fn default() -> Self {
        Self {
            cruise: 180.,
            minimum: 60.,
            lookahead: 42.,
            lookahead_seconds: 0.26,
            corner_braking: 76.,
            lateral_acceleration: 300.,
            steer_gain: 2.,
            speed_response: 20.,
            half_width: 33.,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Rival {
    pub position: Vec2,
    pub velocity: Vec2,
}
#[derive(Clone, Copy, Debug)]
pub struct DriverInput<'a> {
    pub position: Vec2,
    pub forward: Vec2,
    pub velocity: Vec2,
    pub rivals: &'a [Rival],
}
#[derive(Clone, Copy, Debug)]
pub struct RacingControl {
    pub throttle: f32,
    pub steer: f32,
    pub target_speed: f32,
    pub lane: f32,
    pub recovering: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct RacingDriver {
    stuck: f32,
    reverse: f32,
    side: f32,
}
impl RacingDriver {
    pub fn new(seed: u64) -> Self {
        Self {
            side: if seed & 1 == 0 { 1. } else { -1. },
            ..Self::default()
        }
    }
    pub fn step(
        &mut self,
        path: &[Vec2],
        p: DriverSettings,
        input: DriverInput<'_>,
        dt: f32,
    ) -> Result<RacingControl, RacingError> {
        let DriverInput {
            position,
            forward,
            velocity,
            rivals,
        } = input;
        if !(3..=2048).contains(&path.len())
            || path.iter().any(|p| !p.is_finite())
            || rivals.len() > 32
            || rivals
                .iter()
                .any(|r| !r.position.is_finite() || !r.velocity.is_finite())
            || !position.is_finite()
            || !velocity.is_finite()
            || !forward.is_finite()
            || forward.length_squared() < 0.5
            || !dt.is_finite()
            || !(0. ..=0.25).contains(&dt)
            || ![
                p.cruise,
                p.minimum,
                p.lookahead,
                p.lookahead_seconds,
                p.corner_braking,
                p.lateral_acceleration,
                p.steer_gain,
                p.speed_response,
                p.half_width,
            ]
            .iter()
            .all(|v| v.is_finite() && (0. ..=10000.).contains(v))
            || p.cruise < p.minimum
            || p.speed_response <= 0.
            || p.lookahead <= 0.
        {
            return Err(RacingError::Invalid);
        }
        let mut nearest = 0;
        let mut progress = 0.;
        let mut best = f32::INFINITY;
        for (i, a) in path.iter().copied().enumerate() {
            let d = path[(i + 1) % path.len()] - a;
            let len = d.length_squared();
            if len < 1e-6 {
                return Err(RacingError::Invalid);
            }
            let distance =
                position.distance_squared(a + d * ((position - a).dot(d) / len).clamp(0., 1.));
            if distance < best {
                nearest = i;
                progress = ((position - a).dot(d) / len).clamp(0., 1.) * len.sqrt();
                best = distance;
            }
        }
        let speed = velocity.length();
        let mut distance = -progress;
        let mut target = nearest;
        for _ in 0..path.len() {
            if distance >= p.lookahead + speed * p.lookahead_seconds {
                break;
            }
            let next = (target + 1) % path.len();
            distance += path[target].distance(path[next]);
            target = next;
        }
        let forward = forward.normalize_or_zero();
        let side = Vec2::new(-forward.y, forward.x);
        let mut lane = 0.;
        let mut traffic_limit = p.cruise;
        for rival in rivals {
            let delta = rival.position - position;
            let along = delta.dot(forward);
            if along > 0. && along < p.lookahead && delta.dot(side).abs() < p.half_width * 0.4 {
                lane = self.side * p.half_width * 0.28;
                if along < p.lookahead * 0.4 {
                    traffic_limit = traffic_limit.min(rival.velocity.length() + along * 0.5);
                }
            }
        }
        let tangent = (path[(target + 1) % path.len()] - path[target]).normalize_or_zero();
        let desired = path[target] + Vec2::new(-tangent.y, tangent.x) * lane - position;
        let error = forward.perp_dot(desired).atan2(forward.dot(desired));
        let curvature = forward.perp_dot(tangent).abs() / distance.max(1.);
        let corner_speed = (p.lateral_acceleration / curvature.max(1e-6)).sqrt();
        let target_speed = (p.cruise - error.abs().min(1.2) * p.corner_braking)
            .min(corner_speed)
            .min(traffic_limit)
            .clamp(0., p.cruise);
        let target_speed = if rivals.is_empty() {
            target_speed.max(p.minimum)
        } else {
            target_speed
        };
        if speed < p.cruise * 0.02 {
            self.stuck += dt;
        } else {
            self.stuck = 0.;
        }
        if self.stuck >= 3. && self.reverse <= 0. {
            self.reverse = 1.;
            self.stuck = 0.;
        }
        let recovering = self.reverse > 0.;
        self.reverse = (self.reverse - dt).max(0.);
        Ok(RacingControl {
            throttle: if recovering {
                -0.6
            } else {
                ((target_speed - speed) / p.speed_response).clamp(-1., 1.)
            },
            steer: if recovering {
                (-error * p.steer_gain).clamp(-1., 1.)
            } else {
                (error * p.steer_gain).clamp(-1., 1.)
            },
            target_speed,
            lane,
            recovering,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> [Vec2; 4] {
        [
            Vec2::ZERO,
            Vec2::new(200., 0.),
            Vec2::new(200., 200.),
            Vec2::new(0., 200.),
        ]
    }
    #[test]
    fn traffic_and_corners_change_controls_without_teleporting() {
        let mut d = RacingDriver::new(1);
        let p = path();
        let input = DriverInput {
            position: Vec2::new(170., 0.),
            forward: Vec2::X,
            velocity: Vec2::X * 150.,
            rivals: &[],
        };
        let a = d.step(&p, DriverSettings::default(), input, 0.01).unwrap();
        assert!(a.target_speed < 180.);
        let rivals = [Rival {
            position: Vec2::new(180., 0.),
            velocity: Vec2::X * 20.,
        }];
        let b = d
            .step(
                &p,
                DriverSettings::default(),
                DriverInput {
                    rivals: &rivals,
                    ..input
                },
                0.01,
            )
            .unwrap();
        assert!(b.lane < 0. && b.target_speed < a.target_speed);
    }
    #[test]
    fn stationary_driver_attempts_bounded_reverse() {
        let mut d = RacingDriver::new(0);
        let input = DriverInput {
            position: Vec2::ZERO,
            forward: Vec2::X,
            velocity: Vec2::ZERO,
            rivals: &[],
        };
        let mut reversed = false;
        for _ in 0..200 {
            reversed |= d
                .step(&path(), DriverSettings::default(), input, 0.02)
                .unwrap()
                .recovering;
        }
        assert!(reversed);
        assert!(
            d.step(&[Vec2::ZERO; 3], DriverSettings::default(), input, 0.01)
                .is_err()
        );
    }
}
