use super::{CameraError, nonnegative, valid_dt, weight};
use ferravex_math::Vec2;
fn bounded(v: Vec2) -> bool {
    v.is_finite() && v.abs().max_element() <= 1e6
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits2 {
    pub min: Vec2,
    pub max: Vec2,
}
impl Limits2 {
    fn valid(self) -> bool {
        bounded(self.min)
            && bounded(self.max)
            && self.min.x <= self.max.x
            && self.min.y <= self.max.y
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Follow2Settings {
    pub offset: Vec2,
    /// Half-size around the view center in world units. Motion inside it does not pan.
    pub dead_zone: Vec2,
    pub half_life: f32,
    pub look_ahead_time: f32,
    pub max_look_ahead: f32,
    /// Bounds constrain the camera center; derive them from viewport half-extents.
    pub limits: Option<Limits2>,
    pub teleport_distance: Option<f32>,
}
impl Default for Follow2Settings {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            dead_zone: Vec2::ZERO,
            half_life: 0.1,
            look_ahead_time: 0.,
            max_look_ahead: 100.,
            limits: None,
            teleport_distance: Some(20.),
        }
    }
}
impl Follow2Settings {
    fn valid(self) -> bool {
        bounded(self.offset)
            && bounded(self.dead_zone)
            && self.dead_zone.min_element() >= 0.
            && nonnegative(self.half_life, 10.)
            && nonnegative(self.look_ahead_time, 2.)
            && nonnegative(self.max_look_ahead, 1e6)
            && self.limits.is_none_or(Limits2::valid)
            && self
                .teleport_distance
                .is_none_or(|v| v.is_finite() && (0.001..=1e6).contains(&v))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame2 {
    pub center: Vec2,
    pub cut: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Follow2 {
    settings: Follow2Settings,
    center: Vec2,
    previous_subject: Vec2,
    initialized: bool,
    cut_next: bool,
}
impl Default for Follow2 {
    fn default() -> Self {
        Self::new(Follow2Settings::default()).unwrap()
    }
}
impl Follow2 {
    pub fn new(settings: Follow2Settings) -> Result<Self, CameraError> {
        if !settings.valid() {
            return Err(CameraError::InvalidSettings);
        }
        Ok(Self {
            settings,
            center: Vec2::ZERO,
            previous_subject: Vec2::ZERO,
            initialized: false,
            cut_next: false,
        })
    }
    pub fn settings(&self) -> Follow2Settings {
        self.settings
    }
    pub fn set_settings(&mut self, settings: Follow2Settings) -> Result<(), CameraError> {
        if !settings.valid() {
            return Err(CameraError::InvalidSettings);
        }
        self.settings = settings;
        Ok(())
    }
    pub fn center(&self) -> Option<Vec2> {
        self.initialized.then_some(self.center)
    }
    pub fn cut(&mut self) {
        self.cut_next = true;
    }
    pub fn update(
        &mut self,
        dt: f32,
        subject: Vec2,
        velocity: Vec2,
    ) -> Result<Frame2, CameraError> {
        if !valid_dt(dt) || !bounded(subject) || !bounded(velocity) {
            return Err(CameraError::InvalidInput);
        }
        let cut = !self.initialized
            || self.cut_next
            || self
                .settings
                .teleport_distance
                .is_some_and(|v| subject.distance_squared(self.previous_subject) > v * v);
        let goal = subject
            + self.settings.offset
            + (velocity * self.settings.look_ahead_time)
                .clamp_length_max(self.settings.max_look_ahead);
        if !bounded(goal) {
            return Err(CameraError::InvalidInput);
        }
        let mut desired = if cut { goal } else { self.center };
        if !cut {
            for i in 0..2 {
                let delta = goal[i] - self.center[i];
                if delta.abs() > self.settings.dead_zone[i] {
                    desired[i] = goal[i] - delta.signum() * self.settings.dead_zone[i];
                }
            }
        }
        if let Some(limits) = self.settings.limits {
            desired = desired.clamp(limits.min, limits.max);
        }
        let mut center = if cut {
            desired
        } else {
            self.center
                .lerp(desired, weight(dt, self.settings.half_life))
        };
        if let Some(limits) = self.settings.limits {
            center = center.clamp(limits.min, limits.max);
        }
        if !bounded(center) {
            return Err(CameraError::InvalidInput);
        }
        self.center = center;
        self.previous_subject = subject;
        self.initialized = true;
        self.cut_next = false;
        Ok(Frame2 { center, cut })
    }
}
