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
    /// Seconds to halve the remaining distance to a stationary goal, in `0..=10`.
    /// Zero follows immediately on a positive timestep; cuts always snap.
    pub half_life: f32,
    /// Prediction horizon in seconds, finite and in `0..=2`. Zero disables prediction.
    pub look_ahead_time: f32,
    /// Maximum length of the predicted offset in world units, finite and in `0..=1e6`.
    /// Limits the goal prediction, not the distance from the subject to the damped center.
    pub max_look_ahead: f32,
    /// Bounds constrain the camera center; derive them from viewport half-extents.
    pub limits: Option<Limits2>,
    /// Cut when subject motion between successful updates exceeds this distance.
    /// `None` disables automatic teleport detection; explicit snaps still work.
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
    /// Discontinuity: discard any renderer interpolation from the previous frame.
    pub cut: bool,
}
/// A planar follow camera. The first valid update initializes directly at its
/// goal; subsequent updates apply the configured dead zone and half-life.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Follow2 {
    settings: Follow2Settings,
    look_ahead_dead_zone: Vec2,
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
            look_ahead_dead_zone: Vec2::ZERO,
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
    /// Per-axis velocity dead zone for prediction, in world units per second.
    /// Defaults to zero; see [`Self::set_look_ahead_dead_zone`] for tuning examples.
    pub fn look_ahead_dead_zone(&self) -> Vec2 {
        self.look_ahead_dead_zone
    }
    /// Suppress low-speed look-ahead without changing positional follow tuning.
    ///
    /// Each component must be finite and in `0..=1e6`, otherwise this returns
    /// [`CameraError::InvalidSettings`] without changing any camera state. Below
    /// or at the threshold the corresponding velocity component contributes no
    /// prediction. Above it, only the excess speed contributes, avoiding a jump
    /// at the threshold. The resulting prediction still uses the configured
    /// `look_ahead_time` and radial `max_look_ahead` limit. Zero restores the
    /// original velocity prediction on that axis.
    ///
    /// Changing this setting retains the center and any pending cut. It takes
    /// effect on the next update or snap, and is retained by reset and
    /// [`Self::set_settings`]. Prediction has no velocity history, so a stop or
    /// reversal immediately changes the goal; positional damping and the
    /// positional dead zone still apply and may retain some previous lead.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::camera::{Follow2, Follow2Settings};
    /// let mut camera = Follow2::new(Follow2Settings {
    ///     look_ahead_time: 0.5,
    ///     max_look_ahead: 3.,
    ///     half_life: 0.25,
    ///     ..Default::default()
    /// })?;
    /// camera.set_look_ahead_dead_zone(Vec2::new(2., 0.))?;
    /// assert_eq!(camera.look_ahead_dead_zone(), Vec2::new(2., 0.));
    /// assert_eq!(camera.snap(Vec2::ZERO, Vec2::X * 8.)?.center, Vec2::X * 3.);
    /// // Small residual velocity does not keep the prediction moving forward.
    /// let stopped = camera.update(0.25, Vec2::ZERO, Vec2::X)?;
    /// assert_eq!(stopped.center, Vec2::X * 1.5);
    /// assert!(!stopped.cut);
    /// # Ok::<(), ferravex_play::camera::CameraError>(())
    /// ```
    pub fn set_look_ahead_dead_zone(&mut self, dead_zone: Vec2) -> Result<(), CameraError> {
        if !bounded(dead_zone) || dead_zone.min_element() < 0. {
            return Err(CameraError::InvalidSettings);
        }
        self.look_ahead_dead_zone = dead_zone;
        Ok(())
    }
    /// Last successful center, or `None` before initialization or after [`Self::reset`].
    pub fn center(&self) -> Option<Vec2> {
        self.initialized.then_some(self.center)
    }
    /// Forget the current center and tracking history, retaining authored settings
    /// and the velocity look-ahead dead zone.
    /// The next valid update initializes at its goal, even with zero elapsed time.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::camera::Follow2;
    /// let mut camera = Follow2::default();
    /// camera.snap(Vec2::X * 30., Vec2::ZERO)?;
    /// camera.reset(); // Restart before the new player spawn is available.
    /// assert_eq!(camera.center(), None);
    /// let frame = camera.update(0., Vec2::X * 5., Vec2::ZERO)?;
    /// assert!(frame.cut);
    /// assert_eq!(frame.center, Vec2::X * 5.);
    /// # Ok::<(), ferravex_play::camera::CameraError>(())
    /// ```
    pub fn reset(&mut self) {
        self.center = Vec2::ZERO;
        self.previous_subject = Vec2::ZERO;
        self.initialized = false;
        self.cut_next = false;
    }
    /// Request a cut on the next valid update, retaining the current center until then.
    pub fn cut(&mut self) {
        self.cut_next = true;
    }
    /// Immediately place the camera for a spawn, teleport or restart.
    ///
    /// Applies offset, velocity look-ahead and limits without positional dead-zone lag or
    /// damping. The returned frame is a cut; subsequent updates smooth from this
    /// new center. Invalid input leaves all state, including pending cuts, intact.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::camera::{Follow2, Follow2Settings};
    /// let mut camera = Follow2::new(Follow2Settings {
    ///     offset: Vec2::X * 4.,
    ///     ..Default::default()
    /// })?;
    /// let spawn = Vec2::new(100., 20.);
    /// let frame = camera.snap(spawn, Vec2::ZERO)?;
    /// assert!(frame.cut);
    /// assert_eq!(frame.center, spawn + Vec2::X * 4.);
    /// # Ok::<(), ferravex_play::camera::CameraError>(())
    /// ```
    pub fn snap(&mut self, subject: Vec2, velocity: Vec2) -> Result<Frame2, CameraError> {
        let mut next = *self;
        next.cut();
        let frame = next.update(0., subject, velocity)?;
        *self = next;
        Ok(frame)
    }
    /// Follow a sampled subject using elapsed seconds in `0..=0.25`.
    ///
    /// Negative, non-finite or larger timesteps return [`CameraError::InvalidInput`]
    /// without changing state. For a long host suspension, explicitly [`Self::snap`]
    /// to the current subject or bound presentation time before calling this method.
    /// Zero time freezes damping; initialization, cuts and limits still apply.
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
        let prediction_velocity =
            velocity.signum() * (velocity.abs() - self.look_ahead_dead_zone).max(Vec2::ZERO);
        let goal = subject
            + self.settings.offset
            + (prediction_velocity * self.settings.look_ahead_time)
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

#[cfg(test)]
mod journey200_057 {
    use super::*;

    fn settings() -> Follow2Settings {
        Follow2Settings {
            half_life: 0.25,
            look_ahead_time: 0.5,
            max_look_ahead: 10.,
            teleport_distance: None,
            ..Default::default()
        }
    }

    #[test]
    fn zero_velocity_dead_zone_preserves_authored_prediction() {
        let settings = Follow2Settings {
            offset: Vec2::new(4., -2.),
            ..settings()
        };
        let mut camera = Follow2::new(settings).unwrap();
        assert_eq!(camera.settings(), settings);
        assert_eq!(camera.look_ahead_dead_zone(), Vec2::ZERO);
        let subject = Vec2::new(30., 50.);
        for velocity in [
            Vec2::ZERO,
            Vec2::new(-0., 0.),
            Vec2::new(3., -4.),
            Vec2::new(-600., 800.),
            Vec2::splat(1e6),
        ] {
            let expected = subject
                + settings.offset
                + (velocity * settings.look_ahead_time).clamp_length_max(settings.max_look_ahead);
            assert_eq!(camera.snap(subject, velocity).unwrap().center, expected);
        }
    }

    #[test]
    fn velocity_dead_zone_is_per_axis_continuous_and_radially_capped() {
        let mut camera = Follow2::new(settings()).unwrap();
        camera.set_look_ahead_dead_zone(Vec2::new(2., 4.)).unwrap();
        for (velocity, expected) in [
            (Vec2::ZERO, Vec2::ZERO),
            (Vec2::new(1., -3.), Vec2::ZERO),
            (Vec2::new(-2., 4.), Vec2::ZERO),
            (Vec2::new(2.125, -4.125), Vec2::new(0.0625, -0.0625)),
            (Vec2::new(-8., 12.), Vec2::new(-3., 4.)),
            (Vec2::new(62., -84.), Vec2::new(6., -8.)),
        ] {
            let frame = camera.snap(Vec2::ZERO, velocity).unwrap();
            assert!(frame.cut);
            assert!(frame.center.distance(expected) < 0.00001);
            assert!(frame.center.length() <= settings().max_look_ahead);
        }
        camera.set_look_ahead_dead_zone(Vec2::ZERO).unwrap();
        assert_eq!(
            camera.snap(Vec2::ZERO, Vec2::new(2., -4.)).unwrap().center,
            Vec2::new(1., -2.)
        );
    }

    #[test]
    fn invalid_velocity_dead_zone_preserves_state_and_pending_cut() {
        let mut camera = Follow2::new(settings()).unwrap();
        camera.set_look_ahead_dead_zone(Vec2::splat(2.)).unwrap();
        camera.snap(Vec2::X * 10., Vec2::X * 8.).unwrap();
        for pending_cut in [false, true] {
            if pending_cut {
                camera.cut();
            }
            let before = camera;
            for invalid in [
                -f32::EPSILON,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                1_000_001.,
            ] {
                for dead_zone in [Vec2::new(invalid, 0.), Vec2::new(0., invalid)] {
                    assert_eq!(
                        camera.set_look_ahead_dead_zone(dead_zone),
                        Err(CameraError::InvalidSettings)
                    );
                    assert_eq!(camera, before);
                }
            }
        }
        camera.set_look_ahead_dead_zone(Vec2::splat(1e6)).unwrap();
        assert!(camera.update(0., Vec2::ZERO, Vec2::splat(1e6)).unwrap().cut);
        assert_eq!(camera.center(), Some(Vec2::ZERO));
    }

    #[test]
    fn stops_and_reversals_return_monotonically_without_passing_the_new_goal() {
        for positional_dead_zone in [0., 1.] {
            for direction in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                let mut camera = Follow2::new(Follow2Settings {
                    dead_zone: Vec2::splat(positional_dead_zone),
                    ..settings()
                })
                .unwrap();
                camera.set_look_ahead_dead_zone(Vec2::splat(2.)).unwrap();
                camera.snap(Vec2::ZERO, direction * 22.).unwrap();
                // Test both an exact stop and residual velocity on either side
                // of zero. None may extend the previous prediction.
                for velocity in [Vec2::ZERO, direction, -direction] {
                    let mut stopped = camera;
                    let mut previous = 10.;
                    for _ in 0..40 {
                        let frame = stopped.update(0.25, Vec2::ZERO, velocity).unwrap();
                        let lead = frame.center.dot(direction);
                        assert!(!frame.cut);
                        assert!(lead <= previous && lead >= positional_dead_zone);
                        previous = lead;
                    }
                    assert!((previous - positional_dead_zone).abs() < 0.00001);
                }
                let mut previous = 10.;
                let endpoint = -10. + positional_dead_zone;
                for _ in 0..40 {
                    let frame = camera.update(0.25, Vec2::ZERO, -direction * 22.).unwrap();
                    let lead = frame.center.dot(direction);
                    assert!(!frame.cut);
                    assert!(lead <= previous && lead >= endpoint);
                    previous = lead;
                }
                assert!((previous - endpoint).abs() < 0.00001);
            }
        }
    }

    #[test]
    fn tuning_retains_center_history_and_reset_snap_cut_semantics() {
        let settings = Follow2Settings {
            offset: Vec2::new(4., 2.),
            teleport_distance: Some(20.),
            ..settings()
        };
        let mut camera = Follow2::new(settings).unwrap();
        camera.snap(Vec2::X * 10., Vec2::X * 8.).unwrap();
        let center = camera.center();
        camera.set_look_ahead_dead_zone(Vec2::splat(2.)).unwrap();
        assert_eq!(camera.center(), center);
        let frame = camera.update(0., Vec2::X * 10., Vec2::X * 8.).unwrap();
        assert_eq!(Some(frame.center), center);
        assert!(!frame.cut);
        camera.cut();
        camera.set_look_ahead_dead_zone(Vec2::new(4., 2.)).unwrap();
        assert_eq!(camera.center(), center);
        let frame = camera.update(0., Vec2::X * 10., Vec2::X * 8.).unwrap();
        assert!(frame.cut);
        assert_eq!(frame.center, Vec2::new(16., 2.));

        // Updating ordinary settings must not silently clear the velocity filter.
        let settings = Follow2Settings {
            limits: Some(Limits2 {
                min: Vec2::ZERO,
                max: Vec2::new(100., 50.),
            }),
            ..settings
        };
        camera.set_settings(settings).unwrap();
        assert_eq!(camera.look_ahead_dead_zone(), Vec2::new(4., 2.));
        let frame = camera.update(0., Vec2::X * 200., Vec2::X * 8.).unwrap();
        assert!(frame.cut); // Existing automatic teleport detection still applies.
        assert_eq!(frame.center, Vec2::new(100., 2.));

        camera.reset();
        assert_eq!(camera.center(), None);
        assert_eq!(camera.settings(), settings);
        assert_eq!(camera.look_ahead_dead_zone(), Vec2::new(4., 2.));
        let frame = camera.update(0., Vec2::X * 10., Vec2::X * 8.).unwrap();
        assert!(frame.cut);
        assert_eq!(frame.center, Vec2::new(16., 2.));
        let frame = camera.snap(Vec2::ZERO, -Vec2::X * 8.).unwrap();
        assert!(frame.cut);
        assert_eq!(frame.center, Vec2::new(2., 2.));
    }

    #[test]
    fn invalid_updates_do_not_change_tuning_or_tracking_history() {
        let mut camera = Follow2::new(settings()).unwrap();
        camera.set_look_ahead_dead_zone(Vec2::splat(2.)).unwrap();
        camera.snap(Vec2::ZERO, Vec2::X * 22.).unwrap();
        camera.cut();
        let before = camera;
        for (dt, subject, velocity) in [
            (-1., Vec2::ZERO, Vec2::ZERO),
            (0.3, Vec2::ZERO, Vec2::ZERO),
            (f32::NAN, Vec2::ZERO, Vec2::ZERO),
            (0.1, Vec2::splat(f32::INFINITY), Vec2::ZERO),
            (0.1, Vec2::ZERO, Vec2::splat(f32::NAN)),
            (0.1, Vec2::ZERO, Vec2::splat(1_000_001.)),
            (0.1, Vec2::splat(1e6), Vec2::splat(1e6)),
        ] {
            assert_eq!(
                camera.update(dt, subject, velocity),
                Err(CameraError::InvalidInput)
            );
            assert_eq!(camera, before);
        }
        assert_eq!(
            camera.snap(Vec2::ZERO, Vec2::splat(f32::NAN)),
            Err(CameraError::InvalidInput)
        );
        assert_eq!(camera, before);
    }
}
