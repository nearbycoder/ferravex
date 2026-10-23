//! Bounded polyline racing driver. Produces ordinary controls, never teleports or
//! awards progress. Hosts pass perceived nearby traffic, not hidden rival state.
//!
//! Call [`RacingDriver::step`] on fixed simulation ticks, inspect
//! [`RacingDriver::status`] for presentation, and [`RacingDriver::reset`] after a
//! teleport, restart or path replacement. Progress is a nearest-path observation,
//! not a lap counter or checkpoint authority. No history or path is allocated.
use ferravex_math::Vec2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RacingError {
    /// Invalid bounds, non-finite inputs, degenerate segments or arithmetic
    /// outside the finite range supported by the driver. State is unchanged.
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RacingControl {
    pub throttle: f32,
    pub steer: f32,
    pub target_speed: f32,
    pub lane: f32,
    pub recovering: bool,
}

/// Projection onto the supplied closed path, as demonstrated by
/// [`RacingDriver::status`]. Equal-distance segments choose the lowest index.
/// Intersections and teleports can change the projection discontinuously; it
/// does not establish that the driver visited any checkpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriverProgress {
    /// Start index of the closest segment, including the closing segment.
    pub segment: usize,
    /// Clamped fraction along that segment in `0..=1`.
    pub segment_fraction: f32,
    /// Distance along the path from its first vertex, in world units.
    pub distance_along: f32,
    /// Total length, including the closing segment, in world units.
    pub path_length: f32,
    /// Unsigned distance from the supplied position to its projection.
    pub distance_to_path: f32,
}

/// Why the most recent tick looks wrong-way relative to its nearest segment.
/// Facing means heading alignment below `-0.5`; moving means signed along-path
/// speed below `-cruise * 0.02`. These observations also apply during an intended
/// reverse: consult [`DriverStatus::recovery`] before showing a warning.
/// See [`RacingDriver::status`] for an example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrongWayReason {
    FacingBackward,
    MovingBackward,
    FacingAndMovingBackward,
}

/// Explanation of the controls returned on the most recent tick.
/// See [`RacingDriver::status`] for an example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverRecovery {
    /// Ordinary pursuit, including a deliberately zero target speed.
    Driving,
    /// Low speed is explained by a nearby rival's speed limit; no stuck time
    /// accumulates while that limit is at or below `cruise * 0.02`.
    WaitingForTraffic,
    /// Speed is below `cruise * 0.02` despite a higher requested speed.
    Stalled,
    /// A bounded reverse attempt after three simulated seconds stalled.
    Reversing,
}

/// Finite, allocation-free diagnostics for the last successful positive tick.
/// Timers are measured after that tick; `Reversing` describes the returned
/// controls even on the final reverse tick when the remaining time reaches zero.
/// See [`RacingDriver::status`] for an example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriverStatus {
    pub progress: DriverProgress,
    /// Normalized forward dotted with the nearest segment's direction, `-1..=1`.
    pub heading_alignment: f32,
    /// Velocity dotted with that direction, in world units per second.
    pub along_speed: f32,
    pub wrong_way: Option<WrongWayReason>,
    pub recovery: DriverRecovery,
    /// Consecutive low-speed seconds while ordinary controls demand movement.
    pub stuck_seconds: f32,
    /// Remaining time in the current one-second reverse attempt.
    pub reverse_seconds_remaining: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RacingDriver {
    stuck: f32,
    reverse: f32,
    reverse_side: f32,
    side: f32,
    status: Option<DriverStatus>,
}
impl RacingDriver {
    pub fn new(seed: u64) -> Self {
        Self {
            side: if seed & 1 == 0 { 1. } else { -1. },
            ..Self::default()
        }
    }

    /// Clears recovery timers and the last observation, retaining the seeded
    /// passing/recovery side. Call after teleporting, restarting or replacing
    /// the path, before the next simulation tick. This never moves the vehicle.
    ///
    /// ```
    /// use ferravex_play::racing::RacingDriver;
    /// let mut driver = RacingDriver::new(7);
    /// // Reset the vehicle pose/velocity in its owning simulation, then:
    /// driver.reset();
    /// assert_eq!(driver.status(), None);
    /// ```
    pub fn reset(&mut self) {
        self.stuck = 0.;
        self.reverse = 0.;
        self.reverse_side = 0.;
        self.status = None;
    }

    /// Returns the last accepted positive tick's diagnostics, or `None` before
    /// that tick and after [`Self::reset`]. Invalid and zero-duration steps leave
    /// this snapshot unchanged. Reading it does not advance simulation time.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::racing::{
    ///     DriverInput, DriverProgress, DriverRecovery, DriverSettings,
    ///     DriverStatus, RacingDriver, WrongWayReason,
    /// };
    /// let path = [Vec2::ZERO, Vec2::X * 200., Vec2::splat(200.), Vec2::Y * 200.];
    /// let mut driver = RacingDriver::new(0);
    /// let input = DriverInput {
    ///     position: Vec2::X * 50., forward: -Vec2::X,
    ///     velocity: Vec2::ZERO, rivals: &[],
    /// };
    /// driver.step(&path, DriverSettings::default(), input, 0.25)?;
    /// let status: DriverStatus = driver.status().unwrap();
    /// let progress: DriverProgress = status.progress;
    /// assert_eq!(progress.segment, 0);
    /// assert_eq!(progress.distance_along, 50.);
    /// assert_eq!(progress.path_length, 800.);
    /// assert_eq!(status.wrong_way, Some(WrongWayReason::FacingBackward));
    /// assert_eq!(status.recovery, DriverRecovery::Stalled);
    /// assert_eq!(status.stuck_seconds, 0.25);
    /// # Ok::<(), ferravex_play::racing::RacingError>(())
    /// ```
    pub fn status(&self) -> Option<DriverStatus> {
        self.status
    }

    /// Computes controls for a closed path of 3 through 2048 vertices and at
    /// most 32 perceived rivals. Supply fixed simulation `dt` in `0..=0.25`.
    /// A zero-duration call previews controls without updating timers or status;
    /// invalid inputs also leave state unchanged. All successful outputs and
    /// observations are finite. Recovery does not count intentional stops or
    /// waiting behind slow traffic as stalled time. An initially near-zero
    /// reverse steering command uses the seeded side for the whole attempt so
    /// subsequent heading changes do not undo the escape turn. Zero steer gain
    /// still disables steering.
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
            || !forward.length_squared().is_finite()
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
        let mut segment_fraction = 0.;
        let mut distance_along = 0.;
        let mut path_length = 0.;
        let mut best = f32::INFINITY;
        for (i, a) in path.iter().copied().enumerate() {
            let d = path[(i + 1) % path.len()] - a;
            let len = d.length_squared();
            let projection = (position - a).dot(d) / len;
            if !len.is_finite() || len < 1e-6 || !projection.is_finite() {
                return Err(RacingError::Invalid);
            }
            let fraction = projection.clamp(0., 1.);
            let distance = position.distance_squared(a + d * fraction);
            if !distance.is_finite() {
                return Err(RacingError::Invalid);
            }
            if distance < best {
                nearest = i;
                progress = fraction * len.sqrt();
                segment_fraction = fraction;
                distance_along = path_length + progress;
                best = distance;
            }
            path_length += len.sqrt();
        }
        let speed = velocity.length();
        let lookahead = p.lookahead + speed * p.lookahead_seconds;
        if !speed.is_finite() || !lookahead.is_finite() || !path_length.is_finite() {
            return Err(RacingError::Invalid);
        }
        let mut distance = -progress;
        let mut target = nearest;
        for _ in 0..path.len() {
            if distance >= lookahead {
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
            let lateral = delta.dot(side);
            let rival_speed = rival.velocity.length();
            if !along.is_finite() || !lateral.is_finite() || !rival_speed.is_finite() {
                return Err(RacingError::Invalid);
            }
            if along > 0. && along < p.lookahead && lateral.abs() < p.half_width * 0.4 {
                lane = self.side * p.half_width * 0.28;
                if along < p.lookahead * 0.4 {
                    traffic_limit = traffic_limit.min(rival_speed + along * 0.5);
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
        let route_tangent = (path[(nearest + 1) % path.len()] - path[nearest]).normalize_or_zero();
        let heading_alignment = forward.dot(route_tangent).clamp(-1., 1.);
        let along_speed = velocity.dot(route_tangent);
        if !along_speed.is_finite() || !error.is_finite() {
            return Err(RacingError::Invalid);
        }
        let slow_speed = p.cruise * 0.02;
        let wrong_way = match (heading_alignment < -0.5, along_speed < -slow_speed) {
            (true, true) => Some(WrongWayReason::FacingAndMovingBackward),
            (true, false) => Some(WrongWayReason::FacingBackward),
            (false, true) => Some(WrongWayReason::MovingBackward),
            (false, false) => None,
        };
        let mut recovery = if speed < slow_speed && traffic_limit <= slow_speed {
            DriverRecovery::WaitingForTraffic
        } else if speed < slow_speed && target_speed > slow_speed {
            DriverRecovery::Stalled
        } else {
            DriverRecovery::Driving
        };
        let reverse_steer = (-error * p.steer_gain).clamp(-1., 1.);
        let mut stuck = self.stuck;
        let mut reverse = self.reverse;
        let mut reverse_side = self.reverse_side;
        if dt > 0. {
            stuck = if reverse <= 0. && recovery == DriverRecovery::Stalled {
                stuck + dt
            } else {
                0.
            };
            if stuck >= 3. && reverse <= 0. {
                reverse = 1.;
                stuck = 0.;
                reverse_side = if reverse_steer.abs() < 0.1 {
                    if self.side < 0. { -1. } else { 1. }
                } else {
                    0.
                };
            }
        }
        let recovering = reverse > 0.;
        if recovering {
            recovery = DriverRecovery::Reversing;
        }
        let control = RacingControl {
            throttle: if recovering {
                -0.6
            } else {
                ((target_speed - speed) / p.speed_response).clamp(-1., 1.)
            },
            steer: if recovering {
                if reverse_side != 0. {
                    reverse_side * p.steer_gain.min(1.)
                } else {
                    reverse_steer
                }
            } else {
                (error * p.steer_gain).clamp(-1., 1.)
            },
            target_speed,
            lane,
            recovering,
        };
        if dt > 0. {
            self.stuck = stuck;
            self.reverse = (reverse - dt).max(0.);
            self.reverse_side = if self.reverse > 0. { reverse_side } else { 0. };
            self.status = Some(DriverStatus {
                progress: DriverProgress {
                    segment: nearest,
                    segment_fraction,
                    distance_along,
                    path_length,
                    distance_to_path: best.sqrt(),
                },
                heading_alignment,
                along_speed,
                wrong_way,
                recovery,
                stuck_seconds: self.stuck,
                reverse_seconds_remaining: self.reverse,
            });
        }
        Ok(control)
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

    fn stationary() -> DriverInput<'static> {
        DriverInput {
            position: Vec2::new(50., 0.),
            forward: Vec2::X,
            velocity: Vec2::ZERO,
            rivals: &[],
        }
    }

    fn tick(driver: &mut RacingDriver, input: DriverInput<'_>) -> RacingControl {
        driver
            .step(&path(), DriverSettings::default(), input, 0.25)
            .unwrap()
    }

    #[test]
    fn progress_covers_closing_segment_seam_and_equal_distance_ties() {
        let mut driver = RacingDriver::new(0);
        for (position, segment, segment_fraction, distance_along, distance_to_path) in [
            (Vec2::new(50., -10.), 0, 0.25, 50., 10.),
            (Vec2::new(-10., 50.), 3, 0.75, 750., 10.),
            (Vec2::ZERO, 0, 0., 0., 0.),
            (Vec2::new(200., 0.), 0, 1., 200., 0.),
            (Vec2::splat(100.), 0, 0.5, 100., 100.),
        ] {
            tick(
                &mut driver,
                DriverInput {
                    position,
                    ..stationary()
                },
            );
            assert_eq!(
                driver.status().unwrap().progress,
                DriverProgress {
                    segment,
                    segment_fraction,
                    distance_along,
                    path_length: 800.,
                    distance_to_path,
                }
            );
        }
    }

    #[test]
    fn wrong_way_distinguishes_heading_motion_and_both_on_nearest_segment() {
        let mut driver = RacingDriver::new(0);
        for (forward, velocity, wrong_way) in [
            (Vec2::X, Vec2::X * 100., None),
            (-Vec2::X, Vec2::ZERO, Some(WrongWayReason::FacingBackward)),
            (
                Vec2::X,
                -Vec2::X * 100.,
                Some(WrongWayReason::MovingBackward),
            ),
            (
                -Vec2::X,
                -Vec2::X * 100.,
                Some(WrongWayReason::FacingAndMovingBackward),
            ),
            (Vec2::X, -Vec2::X, None),
            (Vec2::Y, Vec2::Y * 100., None),
        ] {
            tick(
                &mut driver,
                DriverInput {
                    // Lookahead crosses a corner; wrong-way uses the local
                    // segment rather than the future steering target's tangent.
                    position: Vec2::new(170., 0.),
                    forward: forward * 2.,
                    velocity,
                    ..stationary()
                },
            );
            let status = driver.status().unwrap();
            assert_eq!(status.wrong_way, wrong_way);
            assert_eq!(status.heading_alignment, forward.x);
            assert_eq!(status.along_speed, velocity.x);
        }
    }

    #[test]
    fn straight_obstruction_uses_seeded_reverse_steering_for_one_second() {
        for seed in [0, 1] {
            let mut driver = RacingDriver::new(seed);
            for _ in 0..11 {
                assert!(!tick(&mut driver, stationary()).recovering);
            }
            assert_eq!(driver.status().unwrap().stuck_seconds, 2.75);
            for remaining in [0.75, 0.5, 0.25, 0.] {
                let control = tick(&mut driver, stationary());
                assert!(control.recovering);
                assert_eq!(control.throttle, -0.6);
                assert_eq!(control.steer, if seed == 0 { 1. } else { -1. });
                let status = driver.status().unwrap();
                assert_eq!(status.recovery, DriverRecovery::Reversing);
                assert_eq!(status.stuck_seconds, 0.);
                assert_eq!(status.reverse_seconds_remaining, remaining);
            }
            assert!(!tick(&mut driver, stationary()).recovering);
            assert_eq!(driver.status().unwrap().recovery, DriverRecovery::Stalled);
            assert_eq!(driver.status().unwrap().stuck_seconds, 0.25);
        }
        // Default has no passing preference; recovery still chooses a turn.
        let mut driver = RacingDriver::default();
        for _ in 0..12 {
            tick(&mut driver, stationary());
        }
        assert_eq!(tick(&mut driver, stationary()).steer, 1.);
    }

    #[test]
    fn escape_turn_is_held_as_heading_changes_and_respects_zero_steer_gain() {
        for seed in [0, 1] {
            let mut driver = RacingDriver::new(seed);
            for _ in 0..12 {
                tick(&mut driver, stationary());
            }
            let side = if seed == 0 { 1. } else { -1. };
            // Reverse motion turns the nose away from the obstruction. Pursuit
            // would now steer the other way and undo that escape turn.
            let forward = Vec2::from_angle(-side * 0.2);
            let control = tick(
                &mut driver,
                DriverInput {
                    forward,
                    velocity: forward * -10.,
                    ..stationary()
                },
            );
            assert!(control.recovering);
            assert_eq!(control.steer, side);
            assert_eq!(
                driver.status().unwrap().wrong_way,
                Some(WrongWayReason::MovingBackward)
            );
            let control = driver
                .step(
                    &path(),
                    DriverSettings {
                        steer_gain: 0.,
                        ..DriverSettings::default()
                    },
                    stationary(),
                    0.25,
                )
                .unwrap();
            assert!(control.recovering);
            assert_eq!(control.steer, 0.);
        }
    }

    #[test]
    fn waiting_for_traffic_clears_stalled_time_and_does_not_trigger_reverse() {
        let mut driver = RacingDriver::new(1);
        for _ in 0..11 {
            tick(&mut driver, stationary());
        }
        let rivals = [Rival {
            position: Vec2::new(52., 0.),
            velocity: Vec2::ZERO,
        }];
        for _ in 0..24 {
            let control = tick(
                &mut driver,
                DriverInput {
                    rivals: &rivals,
                    ..stationary()
                },
            );
            assert!(!control.recovering);
            assert_eq!(control.target_speed, 1.);
            assert!(control.lane < 0.);
            let status = driver.status().unwrap();
            assert_eq!(status.recovery, DriverRecovery::WaitingForTraffic);
            assert_eq!(status.stuck_seconds, 0.);
        }
        for _ in 0..11 {
            assert!(!tick(&mut driver, stationary()).recovering);
        }
        assert!(tick(&mut driver, stationary()).recovering);
    }

    #[test]
    fn intentionally_zero_target_and_resumed_motion_clear_stalled_time() {
        let mut driver = RacingDriver::new(0);
        for _ in 0..11 {
            tick(&mut driver, stationary());
        }
        tick(
            &mut driver,
            DriverInput {
                velocity: Vec2::X * 100.,
                ..stationary()
            },
        );
        assert_eq!(driver.status().unwrap().stuck_seconds, 0.);
        for _ in 0..11 {
            assert!(!tick(&mut driver, stationary()).recovering);
        }
        let settings = DriverSettings {
            minimum: 0.,
            lateral_acceleration: 0.,
            ..DriverSettings::default()
        };
        for _ in 0..24 {
            let control = driver.step(&path(), settings, stationary(), 0.25).unwrap();
            assert_eq!(control.target_speed, 0.);
            assert!(!control.recovering);
            assert_eq!(driver.status().unwrap().recovery, DriverRecovery::Driving);
            assert_eq!(driver.status().unwrap().stuck_seconds, 0.);
        }
    }

    #[test]
    fn reset_after_teleport_discards_recovery_and_preserves_seeded_behavior() {
        for seed in [0, 1] {
            let mut driver = RacingDriver::new(seed);
            for _ in 0..12 {
                tick(&mut driver, stationary());
            }
            assert_eq!(driver.status().unwrap().recovery, DriverRecovery::Reversing);
            driver.reset();
            driver.reset();
            assert_eq!(driver.status(), None);
            let rivals = [Rival {
                position: Vec2::new(200., 62.),
                velocity: Vec2::Y * 30.,
            }];
            let teleported = DriverInput {
                position: Vec2::new(200., 50.),
                forward: Vec2::Y,
                velocity: Vec2::Y * 100.,
                rivals: &rivals,
            };
            let mut fresh = RacingDriver::new(seed);
            let control = tick(&mut driver, teleported);
            assert!(!control.recovering);
            assert_ne!(control.lane, 0.);
            assert_eq!(control, tick(&mut fresh, teleported));
            assert_eq!(driver.status(), fresh.status());
            assert_eq!(driver.status().unwrap().progress.distance_along, 250.);
            assert_eq!(driver.status().unwrap().stuck_seconds, 0.);
            assert_eq!(driver.status().unwrap().reverse_seconds_remaining, 0.);
        }
    }

    #[test]
    fn zero_duration_previews_do_not_consume_stall_or_reverse_time() {
        let mut driver = RacingDriver::new(0);
        driver
            .step(&path(), DriverSettings::default(), stationary(), 0.)
            .unwrap();
        assert_eq!(driver.status(), None);
        for _ in 0..11 {
            tick(&mut driver, stationary());
        }
        for recovering in [false, true] {
            let before = driver.status();
            let mut expected = driver;
            for _ in 0..20 {
                let control = driver
                    .step(
                        &path(),
                        DriverSettings::default(),
                        DriverInput {
                            position: Vec2::new(200., 50.),
                            forward: Vec2::Y,
                            ..stationary()
                        },
                        0.,
                    )
                    .unwrap();
                assert_eq!(control.recovering, recovering);
                assert_eq!(driver.status(), before);
            }
            assert_eq!(
                tick(&mut driver, stationary()),
                tick(&mut expected, stationary())
            );
            assert_eq!(driver.status(), expected.status());
        }
    }

    #[test]
    fn invalid_inputs_and_geometry_overflow_leave_status_and_recovery_unchanged() {
        let mut driver = RacingDriver::new(1);
        for _ in 0..12 {
            tick(&mut driver, stationary());
        }
        let before = driver.status();
        let mut expected = driver;
        let huge_path = [Vec2::ZERO, Vec2::new(f32::MAX, 0.), Vec2::new(0., f32::MAX)];
        for invalid_path in [&[Vec2::ZERO; 3][..], &huge_path[..]] {
            assert_eq!(
                driver.step(invalid_path, DriverSettings::default(), stationary(), 0.25),
                Err(RacingError::Invalid)
            );
        }
        for dt in [-0.1, 0.251, f32::NAN, f32::INFINITY] {
            assert_eq!(
                driver.step(&path(), DriverSettings::default(), stationary(), dt),
                Err(RacingError::Invalid)
            );
        }
        for input in [
            DriverInput {
                position: Vec2::splat(f32::MAX),
                ..stationary()
            },
            DriverInput {
                velocity: Vec2::splat(f32::MAX),
                ..stationary()
            },
            DriverInput {
                forward: Vec2::splat(f32::MAX),
                ..stationary()
            },
            DriverInput {
                rivals: &[Rival {
                    position: Vec2::ZERO,
                    velocity: Vec2::splat(f32::MAX),
                }],
                ..stationary()
            },
        ] {
            assert_eq!(
                driver.step(&path(), DriverSettings::default(), input, 0.25),
                Err(RacingError::Invalid)
            );
        }
        assert_eq!(driver.status(), before);
        assert_eq!(
            tick(&mut driver, stationary()),
            tick(&mut expected, stationary())
        );
        assert_eq!(driver.status(), expected.status());
    }

    #[test]
    fn normal_straight_corner_and_traffic_controls_keep_baseline_values() {
        let mut driver = RacingDriver::new(1);
        let straight = tick(
            &mut driver,
            DriverInput {
                velocity: Vec2::X * 100.,
                ..stationary()
            },
        );
        assert_eq!(
            straight,
            RacingControl {
                throttle: 1.,
                steer: 0.,
                target_speed: 180.,
                lane: 0.,
                recovering: false,
            }
        );
        let corner_input = DriverInput {
            position: Vec2::new(170., 0.),
            velocity: Vec2::X * 150.,
            ..stationary()
        };
        let corner = tick(&mut driver, corner_input);
        assert_eq!(corner.throttle, -1.);
        assert_eq!(corner.steer, 1.);
        assert!((corner.target_speed - 88.8).abs() < 1e-5);
        assert_eq!(corner.lane, 0.);
        assert!(!corner.recovering);
        let rivals = [Rival {
            position: Vec2::new(180., 0.),
            velocity: Vec2::X * 20.,
        }];
        let traffic = tick(
            &mut driver,
            DriverInput {
                rivals: &rivals,
                ..corner_input
            },
        );
        assert_eq!(traffic.throttle, -1.);
        assert_eq!(traffic.steer, 1.);
        assert_eq!(traffic.target_speed, 25.);
        assert_eq!(traffic.lane, -33. * 0.28);
        assert!(!traffic.recovering);
    }
}
