//! Arcade ground/hover and assisted-flight motors. Outputs are candidate motion;
//! hosts resolve collisions, terrain, boost resources and race/mission authority.
use ferravex_math::{Vec2, Vec3};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleError {
    Invalid,
}
fn valid(values: &[f32]) -> bool {
    values
        .iter()
        .all(|v| v.is_finite() && (0. ..=10000.).contains(v))
}
fn time(dt: f32) -> bool {
    dt.is_finite() && (0. ..=0.25).contains(&dt)
}
#[derive(Clone, Copy, Debug)]
pub struct PlanarHandling {
    pub acceleration: f32,
    pub braking: f32,
    pub drag: f32,
    pub grip: f32,
    pub max_speed: f32,
    pub base_turn: f32,
    pub speed_turn: f32,
    pub turn_speed_cap: f32,
    pub response: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanarState {
    pub position: Vec2,
    pub velocity: Vec2,
    pub yaw: f32,
    pub turn_rate: f32,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct DriveInput {
    pub throttle: f32,
    pub steer: f32,
    pub brake: f32,
}
impl PlanarState {
    pub fn step(
        &mut self,
        p: PlanarHandling,
        input: DriveInput,
        dt: f32,
    ) -> Result<(), VehicleError> {
        if !time(dt)
            || !valid(&[
                p.acceleration,
                p.braking,
                p.drag,
                p.grip,
                p.max_speed,
                p.base_turn,
                p.speed_turn,
                p.turn_speed_cap,
                p.response,
            ])
            || !self.position.is_finite()
            || !self.velocity.is_finite()
            || !self.yaw.is_finite()
            || !self.turn_rate.is_finite()
            || ![input.throttle, input.steer, input.brake]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err(VehicleError::Invalid);
        }
        let desired = input.steer.clamp(-1., 1.)
            * (p.base_turn + self.velocity.length().min(p.turn_speed_cap) * p.speed_turn);
        self.turn_rate += (desired - self.turn_rate) * crate::motion::response(p.response, dt);
        self.yaw = (self.yaw + self.turn_rate * dt).rem_euclid(std::f32::consts::TAU);
        let forward = Vec2::new(self.yaw.sin(), self.yaw.cos());
        let side = Vec2::new(forward.y, -forward.x);
        self.velocity -= side * self.velocity.dot(side) * crate::motion::response(p.grip, dt);
        self.velocity += forward * input.throttle.clamp(-1., 1.) * p.acceleration * dt;
        let speed = self.velocity.length();
        if speed > 0. {
            self.velocity *= ((speed - input.brake.clamp(0., 1.) * p.braking * dt).max(0.) / speed)
                * (-p.drag * dt).exp();
        }
        self.velocity = self.velocity.clamp_length_max(p.max_speed);
        self.position += self.velocity * dt;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FlightHandling {
    pub turn_rate: f32,
    pub turn_response: f32,
    pub pitch_limit: f32,
    pub pitch_response: f32,
    pub bank_limit: f32,
    pub bank_response: f32,
    pub speed_response: f32,
}
impl Default for FlightHandling {
    fn default() -> Self {
        Self {
            turn_rate: 1.15,
            turn_response: 9.,
            pitch_limit: 0.58,
            pitch_response: 4.2,
            bank_limit: 0.7,
            bank_response: 5.,
            speed_response: 2.5,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightState {
    pub yaw: f32,
    pub pitch: f32,
    pub bank: f32,
    pub turn_rate: f32,
    pub speed: f32,
}
impl FlightState {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }
    pub fn step(
        &mut self,
        p: FlightHandling,
        turn: f32,
        climb: f32,
        target_speed: f32,
        dt: f32,
    ) -> Result<(), VehicleError> {
        if !time(dt)
            || !valid(&[
                p.turn_rate,
                p.turn_response,
                p.pitch_limit,
                p.pitch_response,
                p.bank_limit,
                p.bank_response,
                p.speed_response,
                target_speed,
            ])
            || p.pitch_limit > 1.5
            || p.bank_limit > std::f32::consts::PI
            || ![
                self.yaw,
                self.pitch,
                self.bank,
                self.turn_rate,
                self.speed,
                turn,
                climb,
            ]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(VehicleError::Invalid);
        }
        let turn = turn.clamp(-1., 1.);
        self.turn_rate +=
            (turn * p.turn_rate - self.turn_rate) * crate::motion::response(p.turn_response, dt);
        self.yaw += self.turn_rate * dt;
        self.pitch += (climb.clamp(-1., 1.) * p.pitch_limit - self.pitch)
            * crate::motion::response(p.pitch_response, dt);
        self.bank +=
            (-turn * p.bank_limit - self.bank) * crate::motion::response(p.bank_response, dt);
        self.speed += (target_speed - self.speed) * crate::motion::response(p.speed_response, dt);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn brakes_stop_without_reverse_and_grip_removes_slip() {
        let mut s = PlanarState {
            position: Vec2::ZERO,
            velocity: Vec2::new(10., 10.),
            yaw: 0.,
            turn_rate: 0.,
        };
        let p = PlanarHandling {
            acceleration: 0.,
            braking: 100.,
            drag: 0.,
            grip: 10.,
            max_speed: 20.,
            base_turn: 1.,
            speed_turn: 0.,
            turn_speed_cap: 20.,
            response: 10.,
        };
        for _ in 0..60 {
            s.step(
                p,
                DriveInput {
                    brake: 1.,
                    ..Default::default()
                },
                1. / 60.,
            )
            .unwrap();
        }
        assert!(s.velocity.length() < 0.001);
        assert!(s.position.y >= 0.);
        let old = s;
        assert!(
            s.step(
                p,
                DriveInput {
                    steer: f32::NAN,
                    ..Default::default()
                },
                0.1
            )
            .is_err()
        );
        assert_eq!(s, old);
    }
    #[test]
    fn flight_limits_and_clock_partition() {
        let mut a = FlightState {
            yaw: 0.,
            pitch: 0.,
            bank: 0.,
            turn_rate: 0.,
            speed: 0.,
        };
        let mut b = a;
        for _ in 0..100 {
            a.step(FlightHandling::default(), 1., 1., 24., 0.01)
                .unwrap();
        }
        for _ in 0..50 {
            b.step(FlightHandling::default(), 1., 1., 24., 0.02)
                .unwrap();
        }
        assert!((a.speed - b.speed).abs() < 0.0001);
        assert!(a.pitch <= 0.58 && a.bank >= -0.7);
        assert!((a.forward().length() - 1.).abs() < 0.0001);
    }
}
