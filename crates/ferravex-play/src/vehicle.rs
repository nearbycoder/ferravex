//! Arcade ground/hover and assisted-flight motors. Outputs are candidate motion;
//! hosts resolve collisions, terrain, boost resources and race/mission authority.
//!
//! Validate handling at setup with [`PlanarHandling::validate`] or
//! [`FlightHandling::validate`]. Telemetry is a read-only snapshot and never
//! advances the motor; feed `step` only the host's fixed simulation interval.
//! Reset helpers clear steering momentum without inspecting the previous state,
//! so they can also recover a motor whose public fields have become invalid.
//! Hosts still own collision-safe spawn selection, flight position, interpolation
//! history, camera history and gameplay resources.
use ferravex_math::{Vec2, Vec3};
/// A checked vehicle operation rejected its input or an unrepresentable result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleError {
    /// Nonfinite or out-of-range input. See the called method's documented limits.
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
impl PlanarHandling {
    /// Check that every coefficient is finite and in `0..=10000`.
    ///
    /// Zero is allowed, including a zero maximum speed. These are exactly the
    /// handling limits enforced by [`PlanarState::step`]; no tuning is changed.
    ///
    /// ```
    /// use ferravex_play::vehicle::{PlanarHandling, VehicleError};
    /// let mut handling = PlanarHandling {
    ///     acceleration: 19., braking: 30., drag: 0.62, grip: 5., max_speed: 30.,
    ///     base_turn: 1.15, speed_turn: 0.018, turn_speed_cap: 32., response: 12.,
    /// };
    /// assert_eq!(handling.validate(), Ok(()));
    /// handling.grip = -1.;
    /// assert_eq!(handling.validate(), Err(VehicleError::Invalid));
    /// ```
    pub fn validate(&self) -> Result<(), VehicleError> {
        if valid(&[
            self.acceleration,
            self.braking,
            self.drag,
            self.grip,
            self.max_speed,
            self.base_turn,
            self.speed_turn,
            self.turn_speed_cap,
            self.response,
        ]) {
            Ok(())
        } else {
            Err(VehicleError::Invalid)
        }
    }
}

/// A planar motor snapshot produced by [`PlanarState::telemetry`].
/// Distances use host world units; angles are radians and rates use seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanarTelemetry {
    /// Unit heading; yaw zero faces positive Y in the motor's 2D plane.
    pub forward: Vec2,
    /// Unit right direction; yaw zero has positive X on its right.
    pub right: Vec2,
    /// Nonnegative velocity magnitude in world units per second.
    pub speed: f32,
    /// Signed velocity along the heading; negative means reversing.
    pub forward_speed: f32,
    /// Signed velocity to the right; negative means sliding left.
    pub lateral_speed: f32,
    /// Velocity angle relative to forward, in `[-PI, PI]`, positive to the right.
    /// Exactly stationary motion reports zero; straight reverse reports +/- PI.
    pub slip_angle: f32,
    /// Current smoothed yaw rate in radians per second.
    pub turn_rate: f32,
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
    /// Read heading, speed and slip without advancing or modifying the motor.
    ///
    /// Returns [`VehicleError::Invalid`] if any state field is nonfinite or a
    /// derived speed cannot be represented as a finite `f32`.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::vehicle::PlanarState;
    /// let motor = PlanarState {
    ///     position: Vec2::ZERO, velocity: Vec2::new(3., 4.), yaw: 0., turn_rate: 0.,
    /// };
    /// let hud = motor.telemetry()?;
    /// assert_eq!((hud.speed, hud.forward_speed, hud.lateral_speed), (5., 4., 3.));
    /// assert!(hud.slip_angle > 0.); // Sliding to the right.
    /// # Ok::<(), ferravex_play::vehicle::VehicleError>(())
    /// ```
    pub fn telemetry(&self) -> Result<PlanarTelemetry, VehicleError> {
        if !self.position.is_finite()
            || !self.velocity.is_finite()
            || !self.yaw.is_finite()
            || !self.turn_rate.is_finite()
        {
            return Err(VehicleError::Invalid);
        }
        let forward = Vec2::new(self.yaw.sin(), self.yaw.cos());
        let right = Vec2::new(forward.y, -forward.x);
        let speed = self.velocity.x.hypot(self.velocity.y);
        let forward_speed = self.velocity.dot(forward);
        let lateral_speed = self.velocity.dot(right);
        if ![speed, forward_speed, lateral_speed]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(VehicleError::Invalid);
        }
        Ok(PlanarTelemetry {
            forward,
            right,
            speed,
            forward_speed,
            lateral_speed,
            slip_angle: if speed == 0. {
                0.
            } else {
                lateral_speed.atan2(forward_speed)
            },
            turn_rate: self.turn_rate,
        })
    }

    /// Replace pose and linear velocity, clearing the smoothed turn rate.
    ///
    /// Position, yaw and velocity must be finite, and velocity magnitude must be
    /// at most `10000` (the largest supported handling speed). Yaw is retained as
    /// supplied; the next step applies its usual wrapping and speed cap. Pass
    /// [`Vec2::ZERO`] to stop. Invalid targets leave every field unchanged, while
    /// a valid target can repair an invalid old state. No simulation tick elapses.
    /// Reset the host's previous pose too, to avoid interpolating across a teleport.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::vehicle::PlanarState;
    /// let mut motor = PlanarState {
    ///     position: Vec2::ZERO, velocity: Vec2::X * 8., yaw: 1., turn_rate: 2.,
    /// };
    /// motor.reset(Vec2::new(10., 20.), 0., Vec2::ZERO)?;
    /// assert_eq!(motor.position, Vec2::new(10., 20.));
    /// assert_eq!((motor.telemetry()?.speed, motor.turn_rate), (0., 0.));
    /// # Ok::<(), ferravex_play::vehicle::VehicleError>(())
    /// ```
    pub fn reset(&mut self, position: Vec2, yaw: f32, velocity: Vec2) -> Result<(), VehicleError> {
        if !position.is_finite()
            || !yaw.is_finite()
            || !velocity.is_finite()
            || velocity.x.hypot(velocity.y) > 10000.
        {
            return Err(VehicleError::Invalid);
        }
        *self = Self {
            position,
            velocity,
            yaw,
            turn_rate: 0.,
        };
        Ok(())
    }

    pub fn step(
        &mut self,
        p: PlanarHandling,
        input: DriveInput,
        dt: f32,
    ) -> Result<(), VehicleError> {
        if !time(dt)
            || p.validate().is_err()
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
impl FlightHandling {
    /// Check finite coefficients in `0..=10000`, pitch limit in `0..=1.5`
    /// radians and bank limit in `0..=PI` radians.
    ///
    /// These are exactly the handling limits enforced by [`FlightState::step`].
    /// Zero responses are valid and freeze their corresponding smoothing.
    ///
    /// ```
    /// use ferravex_play::vehicle::{FlightHandling, VehicleError};
    /// let mut handling = FlightHandling::default();
    /// assert_eq!(handling.validate(), Ok(()));
    /// handling.pitch_limit = 1.6;
    /// assert_eq!(handling.validate(), Err(VehicleError::Invalid));
    /// ```
    pub fn validate(&self) -> Result<(), VehicleError> {
        if valid(&[
            self.turn_rate,
            self.turn_response,
            self.pitch_limit,
            self.pitch_response,
            self.bank_limit,
            self.bank_response,
            self.speed_response,
        ]) && self.pitch_limit <= 1.5
            && self.bank_limit <= std::f32::consts::PI
        {
            Ok(())
        } else {
            Err(VehicleError::Invalid)
        }
    }
}

/// An assisted-flight snapshot produced by [`FlightState::telemetry`].
/// This motor has no position or terrain knowledge, so altitude and clearance
/// remain host measurements. Bank is visual and does not rotate the velocity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightTelemetry {
    /// Unit heading; yaw and pitch zero face positive Z, with positive Y up.
    pub forward: Vec3,
    /// Candidate world velocity in units per second, before host collisions.
    pub velocity: Vec3,
    /// Signed speed along forward, matching [`FlightState::speed`].
    pub speed: f32,
    /// Nonnegative velocity magnitude in the world XZ plane.
    pub horizontal_speed: f32,
    /// Signed world Y velocity; positive means climbing.
    pub vertical_speed: f32,
    /// Pitch in radians, positive above the horizontal plane.
    pub pitch: f32,
    /// Visual bank in radians, matching [`FlightState::bank`].
    pub bank: f32,
    /// Current smoothed yaw rate in radians per second.
    pub turn_rate: f32,
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

    /// Read flight velocity and climb information without advancing the motor.
    ///
    /// Returns [`VehicleError::Invalid`] for any nonfinite state field or derived
    /// velocity/speed. Finite signed speed is reported as stored, including a
    /// negative speed assigned directly to the public state.
    ///
    /// ```
    /// use ferravex_play::vehicle::FlightState;
    /// let motor = FlightState {
    ///     yaw: 0., pitch: 0.6_f32.asin(), bank: 0.4, turn_rate: 0., speed: 10.,
    /// };
    /// let hud = motor.telemetry()?;
    /// assert!((hud.vertical_speed - 6.).abs() < 0.0001);
    /// assert!((hud.horizontal_speed - 8.).abs() < 0.0001);
    /// # Ok::<(), ferravex_play::vehicle::VehicleError>(())
    /// ```
    pub fn telemetry(&self) -> Result<FlightTelemetry, VehicleError> {
        if ![self.yaw, self.pitch, self.bank, self.turn_rate, self.speed]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(VehicleError::Invalid);
        }
        let forward = self.forward();
        let velocity = forward * self.speed;
        let horizontal_speed = velocity.x.hypot(velocity.z);
        if !velocity.is_finite() || !horizontal_speed.is_finite() {
            return Err(VehicleError::Invalid);
        }
        Ok(FlightTelemetry {
            forward,
            velocity,
            speed: self.speed,
            horizontal_speed,
            vertical_speed: velocity.y,
            pitch: self.pitch,
            bank: self.bank,
            turn_rate: self.turn_rate,
        })
    }

    /// Replace flight heading, pitch and speed, clearing bank and turn rate.
    ///
    /// Yaw must be finite and is retained without wrapping. Pitch must be finite
    /// in `-1.5..=1.5` radians and speed in `0..=10000`; these are the motor's
    /// supported pitch/target-speed bounds. The next step approaches its supplied
    /// handling limits normally. Invalid targets leave every field unchanged;
    /// valid targets can repair invalid old state. Pass speed zero to stop.
    /// The host owns position and must reset its interpolation/camera history.
    ///
    /// ```
    /// use ferravex_play::vehicle::FlightState;
    /// let mut motor = FlightState {
    ///     yaw: 2., pitch: -0.5, bank: 0.7, turn_rate: 1., speed: 35.,
    /// };
    /// motor.reset(0., 0., 24.)?;
    /// assert_eq!((motor.bank, motor.turn_rate, motor.speed), (0., 0., 24.));
    /// # Ok::<(), ferravex_play::vehicle::VehicleError>(())
    /// ```
    pub fn reset(&mut self, yaw: f32, pitch: f32, speed: f32) -> Result<(), VehicleError> {
        if !yaw.is_finite() || !(-1.5..=1.5).contains(&pitch) || !valid(&[speed]) {
            return Err(VehicleError::Invalid);
        }
        *self = Self {
            yaw,
            pitch,
            bank: 0.,
            turn_rate: 0.,
            speed,
        };
        Ok(())
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
            || p.validate().is_err()
            || !valid(&[target_speed])
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

    fn skimmer_handling() -> PlanarHandling {
        PlanarHandling {
            acceleration: 19.,
            braking: 30.,
            drag: 0.62,
            grip: 5.,
            max_speed: 30.,
            base_turn: 1.15,
            speed_turn: 0.018,
            turn_speed_cap: 32.,
            response: 12.,
        }
    }

    fn planar() -> PlanarState {
        PlanarState {
            position: Vec2::new(12., 30.),
            velocity: Vec2::new(3., 4.),
            yaw: 0.,
            turn_rate: 0.7,
        }
    }

    fn flight() -> FlightState {
        FlightState {
            yaw: 0.,
            pitch: 0.6_f32.asin(),
            bank: -0.3,
            turn_rate: 0.7,
            speed: 10.,
        }
    }

    #[test]
    fn planar_validation_matches_step_for_every_coefficient() {
        assert_eq!(skimmer_handling().validate(), Ok(()));
        for field in 0..9 {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 10001.] {
                let mut p = skimmer_handling();
                let fields = [
                    &mut p.acceleration,
                    &mut p.braking,
                    &mut p.drag,
                    &mut p.grip,
                    &mut p.max_speed,
                    &mut p.base_turn,
                    &mut p.speed_turn,
                    &mut p.turn_speed_cap,
                    &mut p.response,
                ];
                *fields[field] = value;
                assert_eq!(p.validate(), Err(VehicleError::Invalid));
                let mut s = planar();
                assert_eq!(
                    s.step(p, DriveInput::default(), 0.01),
                    Err(VehicleError::Invalid)
                );
                assert_eq!(s, planar());
            }
        }
        for value in [0., 10000.] {
            let p = PlanarHandling {
                acceleration: value,
                braking: value,
                drag: value,
                grip: value,
                max_speed: value,
                base_turn: value,
                speed_turn: value,
                turn_speed_cap: value,
                response: value,
            };
            assert_eq!(p.validate(), Ok(()));
            assert_eq!(planar().step(p, DriveInput::default(), 0.01), Ok(()));
        }
    }

    #[test]
    fn flight_validation_matches_step_and_preserves_defaults() {
        let p = FlightHandling::default();
        assert_eq!(
            [
                p.turn_rate,
                p.turn_response,
                p.pitch_limit,
                p.pitch_response,
                p.bank_limit,
                p.bank_response,
                p.speed_response,
            ],
            [1.15, 9., 0.58, 4.2, 0.7, 5., 2.5]
        );
        assert_eq!(p.validate(), Ok(()));
        for field in 0..7 {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 10001.] {
                let mut p = FlightHandling::default();
                let fields = [
                    &mut p.turn_rate,
                    &mut p.turn_response,
                    &mut p.pitch_limit,
                    &mut p.pitch_response,
                    &mut p.bank_limit,
                    &mut p.bank_response,
                    &mut p.speed_response,
                ];
                *fields[field] = value;
                assert_eq!(p.validate(), Err(VehicleError::Invalid));
                let mut s = flight();
                assert_eq!(s.step(p, 0., 0., 24., 0.01), Err(VehicleError::Invalid));
                assert_eq!(s, flight());
            }
        }
        for (pitch_limit, bank_limit, valid) in [
            (0., 0., true),
            (1.5, std::f32::consts::PI, true),
            (1.5001, 0., false),
            (0., std::f32::consts::PI + 0.0001, false),
        ] {
            for value in [0., 10000.] {
                let p = FlightHandling {
                    turn_rate: value,
                    turn_response: value,
                    pitch_limit,
                    pitch_response: value,
                    bank_limit,
                    bank_response: value,
                    speed_response: value,
                };
                assert_eq!(p.validate().is_ok(), valid);
                assert_eq!(flight().step(p, 0., 0., 24., 0.01).is_ok(), valid);
            }
        }
    }

    #[test]
    fn planar_telemetry_reports_forward_reverse_sideways_and_stationary_motion() {
        let mut s = planar();
        let t = s.telemetry().unwrap();
        assert_eq!((t.forward, t.right), (Vec2::Y, Vec2::X));
        assert_eq!((t.speed, t.forward_speed, t.lateral_speed), (5., 4., 3.));
        assert_eq!(t.slip_angle, 3_f32.atan2(4.));
        assert_eq!(t.turn_rate, s.turn_rate);
        assert_eq!(s, planar());
        for (velocity, forward, lateral, angle) in [
            (Vec2::ZERO, 0., 0., 0.),
            (Vec2::Y * 8., 8., 0., 0.),
            (-Vec2::Y * 8., -8., 0., std::f32::consts::PI),
            (-Vec2::X * 8., 0., -8., -std::f32::consts::FRAC_PI_2),
        ] {
            s.velocity = velocity;
            let t = s.telemetry().unwrap();
            assert_eq!((t.forward_speed, t.lateral_speed), (forward, lateral));
            assert!((t.slip_angle - angle).abs() < 1e-6);
        }
        s.yaw = std::f32::consts::FRAC_PI_2;
        s.velocity = Vec2::new(4., 3.);
        let t = s.telemetry().unwrap();
        assert!(t.forward.abs_diff_eq(Vec2::X, 1e-6));
        assert!(t.right.abs_diff_eq(-Vec2::Y, 1e-6));
        assert!((t.forward_speed - 4.).abs() < 1e-6);
        assert!((t.lateral_speed + 3.).abs() < 1e-6);
    }

    #[test]
    fn flight_telemetry_reports_climb_descent_and_bank_independent_velocity() {
        let mut s = flight();
        let t = s.telemetry().unwrap();
        assert_eq!(s, flight());
        assert_eq!(
            (t.speed, t.pitch, t.bank, t.turn_rate),
            (10., s.pitch, -0.3, 0.7)
        );
        assert!((t.forward.length() - 1.).abs() < 1e-6);
        assert!(t.velocity.abs_diff_eq(Vec3::new(0., 6., 8.), 1e-6));
        assert!((t.horizontal_speed - 8.).abs() < 1e-6);
        assert!((t.vertical_speed - 6.).abs() < 1e-6);
        s.bank = 1.;
        assert_eq!(s.telemetry().unwrap().velocity, t.velocity);
        s.pitch = -s.pitch;
        s.yaw = std::f32::consts::FRAC_PI_2;
        let t = s.telemetry().unwrap();
        assert!(t.velocity.abs_diff_eq(Vec3::new(8., -6., 0.), 1e-6));
        assert!(t.vertical_speed < 0.);
        s.speed = -10.;
        let reversed = s.telemetry().unwrap();
        assert_eq!(reversed.speed, -10.);
        assert_eq!(reversed.velocity, -t.velocity);
        assert_eq!(reversed.horizontal_speed, t.horizontal_speed);
        s.speed = 0.;
        let stopped = s.telemetry().unwrap();
        assert_eq!(stopped.velocity, Vec3::ZERO);
        assert_eq!((stopped.horizontal_speed, stopped.vertical_speed), (0., 0.));
    }

    #[test]
    fn telemetry_rejects_nonfinite_state_and_unrepresentable_speed() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for field in 0..6 {
                let mut s = planar();
                let fields = [
                    &mut s.position.x,
                    &mut s.position.y,
                    &mut s.velocity.x,
                    &mut s.velocity.y,
                    &mut s.yaw,
                    &mut s.turn_rate,
                ];
                *fields[field] = value;
                assert_eq!(s.telemetry(), Err(VehicleError::Invalid));
            }
            for field in 0..5 {
                let mut s = flight();
                let fields = [
                    &mut s.yaw,
                    &mut s.pitch,
                    &mut s.bank,
                    &mut s.turn_rate,
                    &mut s.speed,
                ];
                *fields[field] = value;
                assert_eq!(s.telemetry(), Err(VehicleError::Invalid));
            }
        }
        let mut s = planar();
        s.velocity = Vec2::splat(f32::MAX);
        assert_eq!(s.telemetry(), Err(VehicleError::Invalid));
        s.velocity = Vec2::X * 1e30;
        assert_eq!(s.telemetry().unwrap().speed, 1e30);
        s.velocity = Vec2::X * f32::from_bits(1);
        assert_eq!(s.telemetry().unwrap().speed, f32::from_bits(1));
    }

    #[test]
    fn planar_reset_is_atomic_and_repairs_corrupt_state() {
        let mut s = planar();
        for (position, yaw, velocity) in [
            (Vec2::new(f32::NAN, 0.), 0., Vec2::ZERO),
            (Vec2::new(0., f32::INFINITY), 0., Vec2::ZERO),
            (Vec2::ZERO, f32::NAN, Vec2::ZERO),
            (Vec2::ZERO, f32::INFINITY, Vec2::ZERO),
            (Vec2::ZERO, 0., Vec2::new(0., f32::NAN)),
            (Vec2::ZERO, 0., Vec2::new(f32::INFINITY, 0.)),
            (Vec2::ZERO, 0., Vec2::splat(8000.)),
            (Vec2::ZERO, 0., Vec2::splat(f32::MAX)),
        ] {
            assert_eq!(s.reset(position, yaw, velocity), Err(VehicleError::Invalid));
            assert_eq!(s, planar());
        }
        s = PlanarState {
            position: Vec2::splat(f32::NAN),
            velocity: Vec2::splat(f32::INFINITY),
            yaw: f32::NAN,
            turn_rate: f32::NEG_INFINITY,
        };
        s.reset(Vec2::new(50., -20.), -2., Vec2::new(6000., 8000.))
            .unwrap();
        assert_eq!(s.position, Vec2::new(50., -20.));
        assert_eq!(s.velocity, Vec2::new(6000., 8000.));
        assert_eq!((s.yaw, s.turn_rate), (-2., 0.));
        assert_eq!(s.telemetry().unwrap().speed, 10000.);
        s.reset(Vec2::ZERO, 0., Vec2::ZERO).unwrap();
        assert_eq!(s.telemetry().unwrap().speed, 0.);
    }

    #[test]
    fn flight_reset_is_atomic_and_repairs_corrupt_state() {
        let mut s = flight();
        for (yaw, pitch, speed) in [
            (f32::NAN, 0., 24.),
            (f32::INFINITY, 0., 24.),
            (0., f32::NAN, 24.),
            (0., f32::INFINITY, 24.),
            (0., -1.5001, 24.),
            (0., 1.5001, 24.),
            (0., 0., f32::NAN),
            (0., 0., f32::INFINITY),
            (0., 0., -0.001),
            (0., 0., 10001.),
        ] {
            assert_eq!(s.reset(yaw, pitch, speed), Err(VehicleError::Invalid));
            assert_eq!(s, flight());
        }
        s = FlightState {
            yaw: f32::NAN,
            pitch: f32::NAN,
            bank: f32::NAN,
            turn_rate: f32::INFINITY,
            speed: f32::NEG_INFINITY,
        };
        for (pitch, speed) in [(-1.5, 0.), (1.5, 10000.)] {
            s.reset(-2., pitch, speed).unwrap();
            assert_eq!((s.yaw, s.pitch, s.speed), (-2., pitch, speed));
            assert_eq!((s.bank, s.turn_rate), (0., 0.));
            assert!(s.telemetry().is_ok());
        }
    }

    #[test]
    fn reset_and_telemetry_preserve_subsequent_fixed_tick_motion() {
        let mut observed_planar = planar();
        observed_planar
            .reset(Vec2::new(5., 2.), -1., Vec2::Y * 2.)
            .unwrap();
        let mut reference_planar = PlanarState {
            position: Vec2::new(5., 2.),
            velocity: Vec2::Y * 2.,
            yaw: -1.,
            turn_rate: 0.,
        };
        let mut observed_flight = flight();
        observed_flight.reset(-1., 0.1, 24.).unwrap();
        let mut reference_flight = FlightState {
            yaw: -1.,
            pitch: 0.1,
            bank: 0.,
            turn_rate: 0.,
            speed: 24.,
        };
        for tick in 0..120 {
            // HUDs may be sampled several times between simulation ticks.
            for _ in 0..3 {
                observed_planar.telemetry().unwrap();
                observed_flight.telemetry().unwrap();
            }
            let steer = if tick < 60 { 0.7 } else { -0.4 };
            let input = DriveInput {
                throttle: 1.,
                steer,
                brake: 0.2,
            };
            observed_planar
                .step(skimmer_handling(), input, 1. / 60.)
                .unwrap();
            reference_planar
                .step(skimmer_handling(), input, 1. / 60.)
                .unwrap();
            observed_flight
                .step(FlightHandling::default(), steer, 0.2, 35., 1. / 60.)
                .unwrap();
            reference_flight
                .step(FlightHandling::default(), steer, 0.2, 35., 1. / 60.)
                .unwrap();
            assert_eq!(observed_planar, reference_planar);
            assert_eq!(observed_flight, reference_flight);
        }
    }

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
