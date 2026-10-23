//! Allocation-free presentation cameras. World Y is up. Shooter yaw zero faces
//! -Z; orbit eye offsets and vehicle-local chase forward use +Z. No simulation
//! or input state is changed by damping, obstruction recovery or shake.
//!
//! [`Follow2`] and [`Rig3`] initialize on their first successful update. Use
//! `snap` when the new spawn/teleport target is available, `reset` when restarting
//! before that target exists, or `cut` to defer a snap until the next update while
//! retaining the last pose. Honor the returned `cut` flag when interpolating
//! rendered frames. Reset simulation interpolation at the teleport too: provide
//! the new target directly so the camera does not follow a stale blended target.
//!
//! Smoothing half-lives are in seconds: after one half-life a stationary goal's
//! error is halved. Existing tuning is retained by resets and snaps. Update time
//! must be finite and in `0..=0.25`; larger gaps are rejected without mutation.
//! A host can deliberately snap on resume or bound only its presentation time.
//!
//! This chase-camera integration uses Dune Skimmer's position/target damping.
//! Call it after sampling the vehicle pose, then copy the returned pose into the
//! renderer's camera. A recovery flag must be raised even for a short teleport
//! below the automatic distance threshold.
//!
//! ```
//! use ferravex_math::Vec3;
//! use ferravex_play::camera::{
//!     CameraError, CollisionQuery, Frame3, Input3, Mode3, NoObstructions,
//!     Rig3, Settings3, Target3,
//! };
//!
//! fn vehicle_view(
//!     camera: &mut Rig3,
//!     vehicle: Target3,
//!     elapsed: f32,
//!     recovered: bool,
//!     blockers: &(impl CollisionQuery + ?Sized),
//! ) -> Result<Frame3, CameraError> {
//!     if !elapsed.is_finite() || elapsed < 0. {
//!         return Err(CameraError::InvalidInput);
//!     }
//!     let input = Input3 {
//!         target: vehicle,
//!         mode: Mode3::Chase {
//!             distance: 8., height: 3., look_ahead: 4., look_height: 1.,
//!             speed_distance: 0.05, speed_height: 0., prediction: 0.1,
//!             level: true,
//!         },
//!         fov_y: 1.,
//!     };
//!     if recovered || elapsed > 0.25 {
//!         camera.snap(input, blockers)
//!     } else {
//!         camera.update(elapsed, input, blockers)
//!     }
//! }
//!
//! let mut camera = Rig3::new(Settings3 {
//!     position_half_life: std::f32::consts::LN_2 / 5.5,
//!     target_half_life: std::f32::consts::LN_2 / 8.,
//!     ..Default::default()
//! })?;
//! let spawn = Target3::at(Vec3::new(100., 1., 20.));
//! let frame = vehicle_view(&mut camera, spawn, 1. / 60., true, &NoObstructions)?;
//! assert!(frame.cut);
//! assert_eq!(camera.pose(), Some(frame.pose));
//! # Ok::<(), CameraError>(())
//! ```
mod collision;
mod composition;
pub use composition::{
    CameraComposer, CameraZone, FramingBounds, FramingError, FramingLimit, FramingResult,
    FramingSettings, MAX_FRAMING_SUBJECTS,
};
mod follow;
pub mod rail;
mod shake;
#[cfg(test)]
mod tests;
pub mod transition;
pub use collision::{Blocker, CollisionQuery, MAX_BLOCKERS, NoObstructions, Sweep, SweepOutcome};
use ferravex_math::{Quat, Vec3};
pub use follow::{Follow2, Follow2Settings, Frame2, Limits2};
pub use shake::{CameraShake, ShakeEnvelope, ShakeImpulse, ShakeSample};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraError {
    InvalidInput,
    InvalidSettings,
    InvalidObstacle,
    InvalidQuery,
    QueryBudget,
    ShakeCapacity,
}
impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "camera: {self:?}")
    }
}
impl std::error::Error for CameraError {}
fn bounded(v: Vec3) -> bool {
    v.is_finite() && v.abs().max_element() <= 1e6
}
fn nonnegative(v: f32, max: f32) -> bool {
    v.is_finite() && (0. ..=max).contains(&v)
}
fn valid_dt(dt: f32) -> bool {
    nonnegative(dt, 0.25)
}
/// Exact exponential response to a stationary goal. Zero half-life snaps on a
/// positive timestep; zero time contributes no damping.
fn weight(dt: f32, half_life: f32) -> f32 {
    if dt == 0. {
        0.
    } else if half_life == 0. {
        1.
    } else {
        -(-std::f32::consts::LN_2 * dt / half_life).exp_m1()
    }
}
fn direction(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(sy * cp, sp, -cy * cp)
}
fn basis(forward: Vec3, up: Vec3) -> (Vec3, Vec3) {
    let right = forward
        .cross(up)
        .try_normalize()
        .unwrap_or_else(|| forward.any_orthonormal_vector());
    (right, right.cross(forward).normalize())
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target3 {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
}
impl Target3 {
    pub fn at(position: Vec3) -> Self {
        Self {
            position,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode3 {
    /// Offsets remain in world axes; useful for an isometric or overhead view.
    Follow { eye_offset: Vec3, look_offset: Vec3 },
    /// Eye offset is spherical around a target-local pivot. Positive pitch is up.
    Orbit {
        yaw: f32,
        pitch: f32,
        distance: f32,
        pivot: Vec3,
    },
    /// Immediate world-space look, yaw zero -Z, positive pitch up. No aim damping.
    FirstPerson {
        eye_offset: Vec3,
        yaw: f32,
        pitch: f32,
    },
    /// Vehicle-local +Z forward, optionally flattened to the world XZ plane.
    Chase {
        distance: f32,
        height: f32,
        look_ahead: f32,
        look_height: f32,
        speed_distance: f32,
        speed_height: f32,
        prediction: f32,
        level: bool,
    },
}
impl Mode3 {
    fn tag(self) -> u8 {
        match self {
            Self::Follow { .. } => 0,
            Self::Orbit { .. } => 1,
            Self::FirstPerson { .. } => 2,
            Self::Chase { .. } => 3,
        }
    }
    fn desired(self, t: Target3) -> Result<(Vec3, Vec3, Vec3), CameraError> {
        let angle = |v: f32| v.is_finite() && v.abs() <= 1e6;
        let pitch_valid = |v: f32| v.is_finite() && v.abs() <= std::f32::consts::FRAC_PI_2 - 0.001;
        let result = match self {
            Self::Follow {
                eye_offset,
                look_offset,
            } if bounded(eye_offset)
                && bounded(look_offset)
                && eye_offset.distance_squared(look_offset) > 1e-6 =>
            {
                (
                    t.position + eye_offset,
                    t.position + look_offset,
                    t.position,
                )
            }
            Self::Orbit {
                yaw,
                pitch,
                distance,
                pivot,
            } if angle(yaw)
                && pitch_valid(pitch)
                && distance.is_finite()
                && (0.01..=1e5).contains(&distance)
                && bounded(pivot) =>
            {
                let origin = t.position + t.rotation * pivot;
                let (s, c) = yaw.sin_cos();
                let (sp, cp) = pitch.sin_cos();
                (
                    origin + Vec3::new(s * cp, sp, c * cp) * distance,
                    origin,
                    origin,
                )
            }
            Self::FirstPerson {
                eye_offset,
                yaw,
                pitch,
            } if bounded(eye_offset) && angle(yaw) && pitch_valid(pitch) => {
                let eye = t.position + t.rotation * eye_offset;
                (eye, eye + direction(yaw, pitch), eye)
            }
            Self::Chase {
                distance,
                height,
                look_ahead,
                look_height,
                speed_distance,
                speed_height,
                prediction,
                level,
            } if distance.is_finite()
                && (0.01..=1e5).contains(&distance)
                && [height, look_ahead, look_height]
                    .into_iter()
                    .all(|v| v.is_finite() && v.abs() <= 1e5)
                && nonnegative(speed_distance, 100.)
                && nonnegative(speed_height, 100.)
                && nonnegative(prediction, 1.) =>
            {
                let mut forward = t.rotation * Vec3::Z;
                if level {
                    forward.y = 0.;
                    forward = forward.try_normalize().ok_or(CameraError::InvalidInput)?;
                }
                let speed = t.velocity.length();
                let center = t.position + t.velocity * prediction;
                (
                    center - forward * (distance + speed * speed_distance)
                        + Vec3::Y * (height + speed * speed_height),
                    center + forward * look_ahead + Vec3::Y * look_height,
                    t.position + Vec3::Y * look_height,
                )
            }
            _ => return Err(CameraError::InvalidInput),
        };
        if [result.0, result.1, result.2].into_iter().all(bounded) {
            Ok(result)
        } else {
            Err(CameraError::InvalidInput)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input3 {
    pub target: Target3,
    pub mode: Mode3,
    pub fov_y: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObstructionSettings {
    pub radius: f32,
    pub padding: f32,
    /// Retract immediately, ease back out after an obstacle clears.
    pub release_half_life: f32,
    pub sweep_motion: bool,
}
impl Default for ObstructionSettings {
    fn default() -> Self {
        Self {
            radius: 0.25,
            padding: 0.02,
            release_half_life: 0.15,
            sweep_motion: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings3 {
    /// Position error half-life in seconds, in `0..=10`; zero disables damping
    /// on positive timesteps. Orbit uses this for yaw, pitch and distance too.
    pub position_half_life: f32,
    /// Look-target error half-life in seconds. First-person aim is always immediate.
    pub target_half_life: f32,
    /// Vertical field-of-view error half-life in seconds.
    pub fov_half_life: f32,
    /// Cut when subject motion between successful updates exceeds this distance.
    /// `None` disables automatic teleport detection; explicit snaps still work.
    pub teleport_distance: Option<f32>,
    pub obstruction: Option<ObstructionSettings>,
    /// Set to zero for reduced-motion presentation.
    pub shake_strength: f32,
}
impl Default for Settings3 {
    fn default() -> Self {
        Self {
            position_half_life: 0.12,
            target_half_life: 0.08,
            fov_half_life: 0.14,
            teleport_distance: Some(20.),
            obstruction: None,
            shake_strength: 1.,
        }
    }
}
impl Settings3 {
    fn valid(self) -> bool {
        [
            self.position_half_life,
            self.target_half_life,
            self.fov_half_life,
        ]
        .into_iter()
        .all(|v| nonnegative(v, 10.))
            && self
                .teleport_distance
                .is_none_or(|v| v.is_finite() && (0.001..=1e6).contains(&v))
            && nonnegative(self.shake_strength, 1.)
            && self.obstruction.is_none_or(|o| {
                o.radius.is_finite()
                    && (0.001..=1000.).contains(&o.radius)
                    && nonnegative(o.padding, 10.)
                    && nonnegative(o.release_half_life, 10.)
            })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose3 {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
}
impl Default for Pose3 {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            target: -Vec3::Z,
            up: Vec3::Y,
            fov_y: std::f32::consts::FRAC_PI_3,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame3 {
    pub pose: Pose3,
    /// Discontinuity: discard any renderer interpolation from the previous frame.
    pub cut: bool,
    pub obstructed: bool,
    pub blocked_pivot: bool,
    pub recovered: bool,
    pub queries: u8,
}
/// Copyable complete presentation state; update errors preserve it unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rig3 {
    settings: Settings3,
    initialized: bool,
    cut_next: bool,
    last_mode: u8,
    previous_subject: Vec3,
    base: Pose3,
    rendered: Pose3,
    boom_length: f32,
    orbit: Vec3,
    pub shake: CameraShake,
}
impl Default for Rig3 {
    fn default() -> Self {
        Self::new(Settings3::default()).unwrap()
    }
}
impl Rig3 {
    pub fn new(settings: Settings3) -> Result<Self, CameraError> {
        if !settings.valid() {
            return Err(CameraError::InvalidSettings);
        }
        Ok(Self {
            settings,
            initialized: false,
            cut_next: false,
            last_mode: 0,
            previous_subject: Vec3::ZERO,
            base: Pose3::default(),
            rendered: Pose3::default(),
            boom_length: 0.,
            orbit: Vec3::ZERO,
            shake: CameraShake::new(0x4645525241564558),
        })
    }
    pub fn settings(&self) -> Settings3 {
        self.settings
    }
    pub fn set_settings(&mut self, settings: Settings3) -> Result<(), CameraError> {
        if !settings.valid() {
            return Err(CameraError::InvalidSettings);
        }
        self.settings = settings;
        Ok(())
    }
    /// Last successful pose, or `None` before initialization or after [`Self::reset`].
    pub fn pose(&self) -> Option<Pose3> {
        self.initialized.then_some(self.rendered)
    }
    /// Forget the pose, damping, orbit and obstruction history, retaining settings.
    ///
    /// Clears active shake without replacing its configured seed or sequence.
    /// The next valid update initializes directly, even with zero elapsed time.
    /// Shake queued after resetting belongs to the new scene and is retained.
    ///
    /// ```
    /// use ferravex_play::camera::Rig3;
    /// let mut camera = Rig3::default();
    /// let tuning = camera.settings();
    /// camera.reset(); // The restarted scene has not supplied its spawn yet.
    /// assert_eq!(camera.pose(), None);
    /// assert_eq!(camera.settings(), tuning);
    /// ```
    pub fn reset(&mut self) {
        self.initialized = false;
        self.cut_next = true;
        self.last_mode = 0;
        self.previous_subject = Vec3::ZERO;
        self.base = Pose3::default();
        self.rendered = Pose3::default();
        self.boom_length = 0.;
        self.orbit = Vec3::ZERO;
        self.shake.clear();
    }
    /// Cut on the next valid update and discard current presentation shake.
    /// The last pose remains available; shake queued after this call is retained.
    pub fn cut(&mut self) {
        self.cut_next = true;
        self.shake.clear();
    }
    /// Immediately initialize or relocate the camera, clearing current shake.
    ///
    /// Position, look target, FOV and orbit state snap to `input`; obstruction
    /// still constrains the boom, without sweeping from the old scene's pose.
    /// Subsequent updates smooth from the new state with the same settings.
    /// Input or collision errors preserve the complete previous camera state,
    /// including shake and pending cuts. Queue new-scene shake after this call.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::camera::{Input3, Mode3, NoObstructions, Rig3, Target3};
    /// let mut camera = Rig3::default();
    /// let frame = camera.snap(Input3 {
    ///     target: Target3::at(Vec3::X * 100.),
    ///     mode: Mode3::Follow {
    ///         eye_offset: Vec3::Z * 10., look_offset: Vec3::ZERO,
    ///     },
    ///     fov_y: 1.,
    /// }, &NoObstructions)?;
    /// assert!(frame.cut);
    /// assert_eq!(frame.pose.position, Vec3::new(100., 0., 10.));
    /// # Ok::<(), ferravex_play::camera::CameraError>(())
    /// ```
    pub fn snap(
        &mut self,
        input: Input3,
        collision: &(impl CollisionQuery + ?Sized),
    ) -> Result<Frame3, CameraError> {
        let mut next = *self;
        next.cut();
        let frame = next.update(0., input, collision)?;
        *self = next;
        Ok(frame)
    }
    /// Advance presentation with elapsed seconds in `0..=0.25`.
    ///
    /// Invalid or larger timesteps return [`CameraError::InvalidInput`] and leave
    /// all state unchanged. Use [`Self::snap`] after a long host suspension if an
    /// immediate view of the current target is desired. Zero time freezes damping
    /// and shake time; initialization, cuts, first-person aim and collision still
    /// apply. Automatic teleports and mode changes cut and clear old shake.
    pub fn update(
        &mut self,
        dt: f32,
        input: Input3,
        collision: &(impl CollisionQuery + ?Sized),
    ) -> Result<Frame3, CameraError> {
        if !valid_dt(dt)
            || !bounded(input.target.position)
            || !bounded(input.target.velocity)
            || !input.target.rotation.is_finite()
            || (input.target.rotation.length_squared() - 1.).abs() > 0.001
            || !input.fov_y.is_finite()
            || !(0.05..=3.1).contains(&input.fov_y)
        {
            return Err(CameraError::InvalidInput);
        }
        let (eye, look, origin) = input.mode.desired(input.target)?;
        let mut next = *self;
        let cut = !self.initialized
            || self.cut_next
            || self.last_mode != input.mode.tag()
            || self.settings.teleport_distance.is_some_and(|v| {
                input
                    .target
                    .position
                    .distance_squared(self.previous_subject)
                    > v * v
            });
        if cut {
            // Explicit cuts already clear old impulses. Preserve feedback that
            // the caller queued afterwards for the new scene/impact.
            if !self.cut_next {
                next.shake.clear();
            }
            next.base = Pose3 {
                position: eye,
                target: look,
                up: Vec3::Y,
                fov_y: input.fov_y,
            };
        } else {
            next.base.position = self
                .base
                .position
                .lerp(eye, weight(dt, self.settings.position_half_life));
            next.base.target = self
                .base
                .target
                .lerp(look, weight(dt, self.settings.target_half_life));
            next.base.fov_y = self.base.fov_y
                + (input.fov_y - self.base.fov_y) * weight(dt, self.settings.fov_half_life);
        }
        if let Mode3::Orbit {
            yaw,
            pitch,
            distance,
            pivot,
        } = input.mode
        {
            if cut {
                next.orbit = Vec3::new(yaw, pitch, distance);
            } else {
                let alpha = weight(dt, self.settings.position_half_life);
                let change = (yaw - self.orbit.x + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                next.orbit = Vec3::new(
                    self.orbit.x + change * alpha,
                    self.orbit.y + (pitch - self.orbit.y) * alpha,
                    self.orbit.z + (distance - self.orbit.z) * alpha,
                );
                let (eye, look, _) = Mode3::Orbit {
                    yaw: next.orbit.x,
                    pitch: next.orbit.y,
                    distance: next.orbit.z,
                    pivot,
                }
                .desired(input.target)?;
                next.base.position = next.base.target + (eye - look);
            }
        }
        // First-person orientation always follows the sampled aim exactly, even
        // when optional position damping follows a corrected/stepped eye.
        if let Mode3::FirstPerson { yaw, pitch, .. } = input.mode {
            next.base.target = next.base.position + direction(yaw, pitch);
        }
        let forward = (next.base.target - next.base.position)
            .try_normalize()
            .unwrap_or(-Vec3::Z);
        let (right, up) = basis(forward, Vec3::Y);
        let shake = next.shake.advance(dt)?;
        let translation = (right * shake.translation.x
            + up * shake.translation.y
            + forward * shake.translation.z)
            * self.settings.shake_strength;
        let rotation = Quat::from_scaled_axis(
            (right * shake.rotation.x + up * shake.rotation.y + forward * shake.rotation.z)
                * self.settings.shake_strength,
        );
        let mut pose = Pose3 {
            position: next.base.position + translation,
            target: next.base.position
                + translation
                + rotation * forward * (next.base.target - next.base.position).length().max(0.01),
            up: rotation * up,
            fov_y: next.base.fov_y,
        };
        let mut frame = Frame3 {
            pose,
            cut,
            obstructed: false,
            blocked_pivot: false,
            recovered: false,
            queries: 0,
        };
        if let Some(settings) = self.settings.obstruction {
            let boom = pose.position - origin;
            let length = boom.length();
            if length > 1e-6 {
                let allowed = limit(collision, origin, pose.position, settings, &mut frame)?;
                frame.blocked_pivot = allowed == 0.;
                next.boom_length = if cut || allowed < self.boom_length {
                    allowed
                } else {
                    self.boom_length
                        + (allowed - self.boom_length) * weight(dt, settings.release_half_life)
                };
                pose.position = origin + boom / length * next.boom_length.min(allowed);
            } else {
                next.boom_length = 0.;
            }
            if !cut
                && settings.sweep_motion
                && pose.position.distance_squared(self.rendered.position) > 1e-12
            {
                let length = pose.position.distance(self.rendered.position);
                let allowed = limit(
                    collision,
                    self.rendered.position,
                    pose.position,
                    settings,
                    &mut frame,
                )?;
                if allowed == 0. {
                    // A moving obstacle may now contain the previous camera.
                    // Use the already checked boom as a recovery cut.
                    frame.recovered = true;
                    frame.cut = true;
                } else if allowed < length {
                    pose.position = self.rendered.position
                        + (pose.position - self.rendered.position) * (allowed / length);
                    let length = pose.position.distance(origin);
                    if length > 1e-6 {
                        let safe = limit(collision, origin, pose.position, settings, &mut frame)?;
                        if safe < length {
                            frame.recovered = true;
                            frame.cut = true;
                        }
                        pose.position = origin + (pose.position - origin) * (safe / length);
                    }
                }
            }
        }
        if self.settings.obstruction.is_some() {
            next.boom_length = pose.position.distance(origin);
        }
        let forward = (pose.target - pose.position)
            .try_normalize()
            .unwrap_or(forward);
        let (_, up) = basis(forward, pose.up);
        pose.up = up;
        if pose.target.distance_squared(pose.position) < 1e-6 {
            pose.target = pose.position + forward;
        }
        if !bounded(pose.position) || !bounded(pose.target) || !pose.up.is_finite() {
            return Err(CameraError::InvalidInput);
        }
        frame.pose = pose;
        next.rendered = pose;
        next.previous_subject = input.target.position;
        next.last_mode = input.mode.tag();
        next.initialized = true;
        next.cut_next = false;
        *self = next;
        Ok(frame)
    }
}
fn limit(
    query: &(impl CollisionQuery + ?Sized),
    start: Vec3,
    end: Vec3,
    settings: ObstructionSettings,
    frame: &mut Frame3,
) -> Result<f32, CameraError> {
    frame.queries += 1;
    let length = start.distance(end);
    let hit = query.sweep(Sweep {
        start,
        end,
        radius: settings.radius,
    })?;
    match hit {
        Some(t) if t.is_finite() && (0. ..=1.).contains(&t) => {
            frame.obstructed = true;
            Ok((t * length - settings.padding).max(0.))
        }
        None => Ok(length),
        _ => Err(CameraError::InvalidQuery),
    }
}
