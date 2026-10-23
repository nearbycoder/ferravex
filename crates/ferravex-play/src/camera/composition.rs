//! Authored world-space composition zones. Presentation offsets never alter the
//! tracked body or shooter aim; first-person input is passed through unchanged.
//!
//! Perspective group framing is a separate, stateless calculation: it fits
//! authored subject bounds before camera damping, obstruction and shake. Those
//! later effects can move a subject outside the requested screen margins.
use super::*;
use ferravex_math::Vec2;

/// Maximum number of subject boxes examined by one framing calculation.
pub const MAX_FRAMING_SUBJECTS: usize = 256;

/// An axis-aligned world-space subject box. Equal endpoints on any axis are
/// allowed, including a point subject. Each coordinate must be within ±1e6.
/// Bounds are checked by [`FramingSettings::frame`], including public-field edits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FramingBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl FramingBounds {
    fn valid(self) -> bool {
        bounded(self.min)
            && bounded(self.max)
            && (0..3).all(|axis| self.min[axis] <= self.max[axis])
    }

    fn corners(self) -> impl Iterator<Item = Vec3> {
        (0..8).map(move |bits| {
            Vec3::new(
                if bits & 1 == 0 {
                    self.min.x
                } else {
                    self.max.x
                },
                if bits & 2 == 0 {
                    self.min.y
                } else {
                    self.max.y
                },
                if bits & 4 == 0 {
                    self.min.z
                } else {
                    self.max.z
                },
            )
        })
    }
}

/// A rejected framing request. No camera or composer state is changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramingError {
    /// Invalid aspect, field of view, margins, near plane or distance limits.
    InvalidSettings,
    /// The view quaternion is non-finite or not unit length (tolerance 0.001).
    InvalidRotation,
    EmptyGroup,
    /// More than [`MAX_FRAMING_SUBJECTS`] boxes were supplied; none are ignored.
    TooManySubjects,
    /// The first non-finite, inverted or out-of-range box, in input order.
    InvalidBounds {
        index: usize,
    },
    /// The required distance overflows f32, the eye exceeds ±1e6, or rounding
    /// the computed pose loses its separation/framing at the supplied world scale.
    Unrepresentable,
}

impl std::fmt::Display for FramingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "camera framing: {self:?}")
    }
}

impl std::error::Error for FramingError {}

/// How authored distance limits affected a framing result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramingLimit {
    /// The computed fit distance lies inside the configured inclusive limits.
    None,
    /// A small/degenerate group uses the configured minimum distance.
    MinimumDistance,
    /// The camera cannot back away far enough to satisfy all framing constraints.
    MaximumDistance,
}

/// A perspective framing solution, before damping, obstruction and shake.
/// The host must use the same aspect and vertical FOV, a near clip plane no
/// larger than the requested `near`, and a sufficiently distant far clip plane.
/// Floating-point projection retains ordinary f32 rounding error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FramingResult {
    /// Looks at the center of the group's world-space enclosing box.
    pub pose: Pose3,
    /// Selected boom length after applying the configured distance limits.
    pub distance: f32,
    /// Unclamped distance needed for the margins and near plane, rounded upward
    /// to f32. It may exceed `distance` when the maximum prevents a complete fit.
    pub required_distance: f32,
    pub limit: FramingLimit,
}

/// Checked perspective framing constraints. All fields are revalidated on use.
///
/// A box's eight corners are projected independently, including their depth;
/// framing does not approximate a subject by a point or use vertical FOV for
/// both axes. The fit holds the orientation and group center fixed. It is not
/// an optimization over alternative orientations or target positions.
///
/// ```
/// use ferravex_math::{Quat, Vec2, Vec3};
/// use ferravex_play::camera::{FramingBounds, FramingLimit, FramingSettings};
/// let subjects = [FramingBounds { min: -Vec3::ONE, max: Vec3::ONE }];
/// let settings = FramingSettings {
///     aspect_ratio: 9. / 16., // width / height of a portrait viewport
///     margin: Vec2::splat(0.1), // reserve 10% along each screen edge
///     ..Default::default()
/// };
/// let frame = settings.frame(&subjects, Quat::IDENTITY)?;
/// assert_ne!(frame.limit, FramingLimit::MaximumDistance);
/// assert_eq!(frame.pose.target, Vec3::ZERO);
/// // Copy frame.pose into the presentation camera using the same aspect ratio.
/// # Ok::<(), ferravex_play::camera::FramingError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FramingSettings {
    /// Finite positive viewport width / height.
    pub aspect_ratio: f32,
    /// Vertical field of view in radians, in `0.05..=3.1`, matching [`Input3`].
    pub fov_y: f32,
    /// Symmetric per-edge fractions of the full viewport width and height.
    /// Each component must be finite and in `0..0.5`. For example, `x = 0.1`
    /// leaves 80% of the width for subjects, independent of the aspect ratio.
    pub margin: Vec2,
    /// Minimum view-space depth of every corner, in `0.001..=1e5` world units.
    pub near: f32,
    /// Positive boom floor, in `0.01..=1e5`; prevents point subjects collapsing
    /// the eye onto the target. Must not exceed `max_distance`.
    pub min_distance: f32,
    /// Boom ceiling in `min_distance..=1e5`. A constrained fit is returned with
    /// [`FramingLimit::MaximumDistance`], rather than silently claiming success.
    pub max_distance: f32,
}

impl Default for FramingSettings {
    fn default() -> Self {
        Self {
            aspect_ratio: 16. / 9.,
            fov_y: std::f32::consts::FRAC_PI_3,
            margin: Vec2::splat(0.05),
            near: 0.1,
            min_distance: 0.5,
            max_distance: 1e5,
        }
    }
}

impl FramingSettings {
    fn valid(self) -> bool {
        self.aspect_ratio.is_finite()
            && self.aspect_ratio > 0.
            && self.fov_y.is_finite()
            && (0.05..=3.1).contains(&self.fov_y)
            && self.margin.is_finite()
            && (0. ..0.5).contains(&self.margin.x)
            && (0. ..0.5).contains(&self.margin.y)
            && self.near.is_finite()
            && (0.001..=1e5).contains(&self.near)
            && self.min_distance.is_finite()
            && (0.01..=1e5).contains(&self.min_distance)
            && self.max_distance.is_finite()
            && (self.min_distance..=1e5).contains(&self.max_distance)
    }

    /// Frame one subject or a group without allocations or camera mutation.
    ///
    /// `view_rotation` maps camera-local -Z to the viewing direction and +Y to
    /// up; identity places the eye on the group's +Z side. Accepted quaternions
    /// are normalized before use. Each call examines at most 256 boxes, with
    /// two passes over their 2048 corners and constant storage. Duplicate boxes
    /// do not change the fit.
    ///
    /// Errors are checked in this order: settings, rotation, group length,
    /// individual bounds, then the representability of the result. Empty groups
    /// return an error so the caller can retain its last presentation pose.
    /// A maximum-distance result may violate both margins and near clearance;
    /// callers must inspect `limit` before relying on the fit.
    ///
    /// ```
    /// use ferravex_math::{Quat, Vec3};
    /// use ferravex_play::camera::{FramingBounds, FramingError, FramingLimit, FramingSettings};
    /// let point = FramingBounds { min: Vec3::ZERO, max: Vec3::ZERO };
    /// let settings = FramingSettings::default();
    /// let frame = settings.frame(&[point], Quat::IDENTITY)?;
    /// assert_eq!(frame.limit, FramingLimit::MinimumDistance);
    /// assert_eq!(frame.distance, settings.min_distance);
    /// assert_eq!(settings.frame(&[], Quat::IDENTITY), Err(FramingError::EmptyGroup));
    /// # Ok::<(), FramingError>(())
    /// ```
    pub fn frame(
        self,
        subjects: &[FramingBounds],
        view_rotation: Quat,
    ) -> Result<FramingResult, FramingError> {
        if !self.valid() {
            return Err(FramingError::InvalidSettings);
        }
        if !view_rotation.is_finite() || (view_rotation.length_squared() - 1.).abs() > 0.001 {
            return Err(FramingError::InvalidRotation);
        }
        if subjects.is_empty() {
            return Err(FramingError::EmptyGroup);
        }
        if subjects.len() > MAX_FRAMING_SUBJECTS {
            return Err(FramingError::TooManySubjects);
        }
        let mut min = subjects[0].min;
        let mut max = subjects[0].max;
        for (index, subject) in subjects.iter().enumerate() {
            if !subject.valid() {
                return Err(FramingError::InvalidBounds { index });
            }
            min = min.min(subject.min);
            max = max.max(subject.max);
        }
        let center = (min + max) * 0.5;
        let rotation = view_rotation.normalize();
        let back = (rotation * Vec3::Z).normalize();
        let (right, up) = basis(-back, rotation * Vec3::Y);
        let tangent = (f64::from(self.fov_y) * 0.5).tan();
        let horizontal =
            tangent * f64::from(self.aspect_ratio) * (1. - 2. * f64::from(self.margin.x));
        let vertical = tangent * (1. - 2. * f64::from(self.margin.y));
        let mut required = 0_f64;
        for point in subjects.iter().flat_map(|subject| subject.corners()) {
            // Subtract in f64 so tiny extents at a large world position do not
            // disappear in intermediate math, and extreme valid aspects do not
            // overflow before the final explicit representability check.
            let project = |axis: Vec3| -> f64 {
                (0..3)
                    .map(|i| (f64::from(point[i]) - f64::from(center[i])) * f64::from(axis[i]))
                    .sum()
            };
            let z = project(back);
            required = required
                .max(z + project(right).abs() / horizontal)
                .max(z + project(up).abs() / vertical)
                .max(z + f64::from(self.near));
        }
        let mut required_distance = required as f32;
        if f64::from(required_distance) < required {
            required_distance = required_distance.next_up();
        }
        if !required_distance.is_finite() {
            return Err(FramingError::Unrepresentable);
        }
        let limit = if required_distance > self.max_distance {
            FramingLimit::MaximumDistance
        } else if required_distance < self.min_distance {
            FramingLimit::MinimumDistance
        } else {
            FramingLimit::None
        };
        let distance = required_distance.clamp(self.min_distance, self.max_distance);
        let position = center + back * distance;
        if !bounded(position) || position.distance_squared(center) <= 1e-6 {
            return Err(FramingError::Unrepresentable);
        }
        // Rounding a small boom at a large world coordinate can lose a sizable
        // part of the near clearance or change the view direction. Do not report
        // a complete fit when the actual stored pose cannot retain it. Allow
        // ordinary f32 basis rounding, but not scale-dependent clipping.
        if limit != FramingLimit::MaximumDistance {
            let actual_back = (position - center).normalize();
            let (actual_right, actual_up) = basis(-actual_back, up);
            let tolerance = 32. * f64::from(f32::EPSILON);
            for point in subjects.iter().flat_map(|subject| subject.corners()) {
                let project = |axis: Vec3| -> f64 {
                    (0..3)
                        .map(|i| {
                            (f64::from(point[i]) - f64::from(position[i])) * f64::from(axis[i])
                        })
                        .sum()
                };
                let depth = -project(actual_back);
                if depth < f64::from(self.near) * (1. - tolerance)
                    || project(actual_right).abs() > depth * horizontal * (1. + tolerance)
                    || project(actual_up).abs() > depth * vertical * (1. + tolerance)
                {
                    return Err(FramingError::Unrepresentable);
                }
            }
        }
        Ok(FramingResult {
            pose: Pose3 {
                position,
                target: center,
                up,
                fov_y: self.fov_y,
            },
            distance,
            required_distance,
            limit,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraZone {
    pub min: Vec3,
    pub max: Vec3,
    pub feather: f32,
    pub priority: i16,
    pub focus_offset: Vec3,
    pub boom_scale: f32,
    pub fov_scale: f32,
}
impl CameraZone {
    fn valid(self) -> bool {
        bounded(self.min)
            && bounded(self.max)
            && (self.max - self.min).min_element() > 0.
            && nonnegative(self.feather, 10000.)
            && bounded(self.focus_offset)
            && self.boom_scale.is_finite()
            && (0.5..=2.).contains(&self.boom_scale)
            && self.fov_scale.is_finite()
            && (0.5..=1.5).contains(&self.fov_scale)
    }
    fn weight(self, p: Vec3) -> f32 {
        let margin = (p - self.min).min(self.max - p).min_element();
        if margin < 0. {
            0.
        } else if self.feather == 0. {
            1.
        } else {
            (margin / self.feather).clamp(0., 1.)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraComposer<const N: usize> {
    zones: [CameraZone; N],
    anticipation: f32,
    max_lead: f32,
    half_life: f32,
    offset: Vec3,
    boom: f32,
    fov: f32,
}
impl<const N: usize> CameraComposer<N> {
    pub fn new(
        zones: [CameraZone; N],
        anticipation: f32,
        max_lead: f32,
        half_life: f32,
    ) -> Result<Self, CameraError> {
        if N > 32
            || zones.iter().any(|z| !z.valid())
            || !nonnegative(anticipation, 1.)
            || !nonnegative(max_lead, 100.)
            || !nonnegative(half_life, 10.)
        {
            return Err(CameraError::InvalidSettings);
        }
        Ok(Self {
            zones,
            anticipation,
            max_lead,
            half_life,
            offset: Vec3::ZERO,
            boom: 1.,
            fov: 1.,
        })
    }
    pub fn clear(&mut self) {
        self.offset = Vec3::ZERO;
        self.boom = 1.;
        self.fov = 1.;
    }
    pub fn update(
        &mut self,
        mut input: Input3,
        acceleration: Vec3,
        dt: f32,
    ) -> Result<Input3, CameraError> {
        if !valid_dt(dt)
            || !bounded(input.target.position)
            || !bounded(input.target.velocity)
            || !bounded(acceleration)
            || !input.target.rotation.is_finite()
            || (input.target.rotation.length_squared() - 1.).abs() > 0.001
            || !input.fov_y.is_finite()
            || !(0.05..=3.1).contains(&input.fov_y)
        {
            return Err(CameraError::InvalidInput);
        }
        input.mode.desired(input.target)?;
        if matches!(input.mode, Mode3::FirstPerson { .. }) {
            return Ok(input);
        }
        let mut selected = None;
        let mut priority = i16::MIN;
        for zone in self.zones {
            let weight = zone.weight(input.target.position);
            if weight > 0. && (selected.is_none() || zone.priority > priority) {
                selected = Some((zone, weight));
                priority = zone.priority;
            }
        }
        let (focus, boom, fov) = selected.map_or((Vec3::ZERO, 1., 1.), |(z, w)| {
            (
                z.focus_offset * w,
                1. + (z.boom_scale - 1.) * w,
                1. + (z.fov_scale - 1.) * w,
            )
        });
        let lead = (input.target.velocity * self.anticipation
            + acceleration * (0.5 * self.anticipation * self.anticipation))
            .clamp_length_max(self.max_lead);
        let alpha = if self.half_life == 0. {
            1.
        } else {
            1. - (-std::f32::consts::LN_2 * dt / self.half_life).exp()
        };
        let mut next = *self;
        next.offset = next.offset.lerp(focus + lead, alpha);
        next.boom += (boom - next.boom) * alpha;
        next.fov += (fov - next.fov) * alpha;
        input.target.position += next.offset;
        input.fov_y = (input.fov_y * next.fov).clamp(0.05, 3.1);
        match &mut input.mode {
            Mode3::Follow {
                eye_offset,
                look_offset,
            } => *eye_offset = *look_offset + (*eye_offset - *look_offset) * next.boom,
            Mode3::Orbit { distance, .. } => *distance *= next.boom,
            Mode3::Chase {
                distance, height, ..
            } => {
                *distance *= next.boom;
                *height *= next.boom;
            }
            Mode3::FirstPerson { .. } => {}
        }
        input.mode.desired(input.target)?;
        if !bounded(input.target.position) {
            return Err(CameraError::InvalidInput);
        }
        *self = next;
        Ok(input)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_math::Mat4;

    fn box_at(center: Vec3, half: Vec3) -> FramingBounds {
        FramingBounds {
            min: center - half,
            max: center + half,
        }
    }

    fn assert_framed(settings: FramingSettings, subjects: &[FramingBounds], frame: FramingResult) {
        assert_ne!(frame.limit, FramingLimit::MaximumDistance);
        assert!(frame.distance >= settings.min_distance);
        assert!(frame.distance <= settings.max_distance);
        assert!(frame.distance >= frame.required_distance);
        assert_eq!(frame.pose.fov_y, settings.fov_y);
        let view = Mat4::look_at_rh(frame.pose.position, frame.pose.target, frame.pose.up);
        let projection =
            Mat4::perspective_rh_gl(settings.fov_y, settings.aspect_ratio, settings.near, 1e7);
        assert!(view.is_finite());
        for point in subjects.iter().flat_map(|subject| subject.corners()) {
            let camera = view.transform_point3(point);
            assert!(-camera.z >= settings.near - 1e-5, "depth: {camera:?}");
            let screen = projection.project_point3(camera);
            assert!(
                screen.x.abs() <= 1. - 2. * settings.margin.x + 1e-5,
                "x: {screen:?}"
            );
            assert!(
                screen.y.abs() <= 1. - 2. * settings.margin.y + 1e-5,
                "y: {screen:?}"
            );
        }
    }

    #[test]
    fn portrait_and_ultrawide_use_both_fov_axes() {
        let subjects = [box_at(Vec3::ZERO, Vec3::new(4., 2., 1.))];
        let mut distances = [0.; 2];
        for (index, aspect_ratio) in [9. / 16., 32. / 9.].into_iter().enumerate() {
            let settings = FramingSettings {
                aspect_ratio,
                fov_y: std::f32::consts::FRAC_PI_2,
                margin: Vec2::ZERO,
                ..Default::default()
            };
            let frame = settings.frame(&subjects, Quat::IDENTITY).unwrap();
            assert_framed(settings, &subjects, frame);
            let expected = 1. + (4. / aspect_ratio).max(2.);
            assert!((frame.distance - expected).abs() < 1e-5);
            assert_eq!(frame.limit, FramingLimit::None);
            distances[index] = frame.distance;
        }
        assert!(distances[0] > distances[1] * 2.);
    }

    #[test]
    fn margins_are_per_edge_and_independent_of_aspect() {
        let subjects = [box_at(Vec3::ZERO, Vec3::new(2., 1., 0.))];
        for aspect_ratio in [9. / 16., 1., 32. / 9.] {
            let plain = FramingSettings {
                aspect_ratio,
                margin: Vec2::ZERO,
                ..Default::default()
            };
            let padded = FramingSettings {
                margin: Vec2::splat(0.25),
                ..plain
            };
            let small = plain.frame(&subjects, Quat::IDENTITY).unwrap();
            let large = padded.frame(&subjects, Quat::IDENTITY).unwrap();
            assert!((large.distance - small.distance * 2.).abs() < 1e-5);
            assert_framed(padded, &subjects, large);
            let unequal = FramingSettings {
                margin: Vec2::new(0.1, 0.35),
                ..plain
            };
            assert_framed(
                unequal,
                &subjects,
                unequal.frame(&subjects, Quat::IDENTITY).unwrap(),
            );
        }
    }

    #[test]
    fn rotated_group_includes_each_subjects_depth_and_bounds() {
        let subjects = [
            box_at(Vec3::new(-3., 2., -6.), Vec3::new(1., 2., 0.5)),
            box_at(Vec3::new(5., -1., 3.), Vec3::new(2., 1., 2.)),
            box_at(Vec3::new(0., 3., 2.), Vec3::ZERO),
        ];
        for aspect_ratio in [9. / 16., 32. / 9.] {
            for rotation in [
                Quat::IDENTITY,
                Quat::from_rotation_y(0.8) * Quat::from_rotation_x(-0.4),
                Quat::from_rotation_z(0.6) * Quat::from_rotation_x(1.5),
            ] {
                let settings = FramingSettings {
                    aspect_ratio,
                    ..Default::default()
                };
                let frame = settings.frame(&subjects, rotation).unwrap();
                assert_eq!(frame.pose.target, Vec3::new(1.5, 1., -0.75));
                assert_framed(settings, &subjects, frame);
            }
        }
    }

    #[test]
    fn points_lines_and_near_zero_bounds_have_a_usable_boom() {
        let settings = FramingSettings::default();
        for half in [
            Vec3::ZERO,
            Vec3::splat(1e-30),
            Vec3::splat(1e-8),
            Vec3::Y * 1e-8,
        ] {
            let subjects = [box_at(Vec3::ZERO, half)];
            let frame = settings.frame(&subjects, Quat::IDENTITY).unwrap();
            assert_eq!(frame.distance, settings.min_distance);
            assert_eq!(frame.limit, FramingLimit::MinimumDistance);
            assert_framed(settings, &subjects, frame);
        }
        let line = [box_at(Vec3::ZERO, Vec3::Z * 10.)];
        let frame = settings.frame(&line, Quat::IDENTITY).unwrap();
        assert!((frame.distance - 10.1).abs() < 1e-5);
        assert_framed(settings, &line, frame);
    }

    #[test]
    fn near_plane_is_checked_even_for_a_point_or_very_wide_fov() {
        let settings = FramingSettings {
            fov_y: 3.1,
            near: 5.,
            ..Default::default()
        };
        for half in [Vec3::ZERO, Vec3::ONE] {
            let subjects = [box_at(Vec3::ZERO, half)];
            let frame = settings.frame(&subjects, Quat::IDENTITY).unwrap();
            assert_eq!(frame.distance, 5. + half.z);
            assert_framed(settings, &subjects, frame);
        }
    }

    #[test]
    fn maximum_distance_reports_unmet_screen_and_near_constraints() {
        for (half, near) in [(Vec3::splat(10.), 0.1), (Vec3::ZERO, 5.)] {
            let settings = FramingSettings {
                max_distance: 2.,
                near,
                ..Default::default()
            };
            let frame = settings
                .frame(&[box_at(Vec3::ZERO, half)], Quat::IDENTITY)
                .unwrap();
            assert_eq!(frame.limit, FramingLimit::MaximumDistance);
            assert_eq!(frame.distance, 2.);
            assert!(frame.required_distance > frame.distance);
        }
        let settings = FramingSettings {
            min_distance: 0.5,
            max_distance: 0.5,
            near: 0.5,
            ..Default::default()
        };
        let point = [box_at(Vec3::ZERO, Vec3::ZERO)];
        let frame = settings.frame(&point, Quat::IDENTITY).unwrap();
        assert_eq!(frame.limit, FramingLimit::None);
        assert_framed(settings, &point, frame);
    }

    #[test]
    fn group_order_duplicates_and_capacity_do_not_change_the_fit() {
        let a = box_at(Vec3::X * -4., Vec3::ONE);
        let b = box_at(Vec3::X * 2., Vec3::ONE);
        let settings = FramingSettings::default();
        let frame = settings.frame(&[a, b], Quat::IDENTITY).unwrap();
        assert_eq!(settings.frame(&[b, a, b], Quat::IDENTITY), Ok(frame));
        let mut maximum = [a; MAX_FRAMING_SUBJECTS];
        maximum[MAX_FRAMING_SUBJECTS - 1] = b;
        assert_eq!(settings.frame(&maximum, Quat::IDENTITY), Ok(frame));
        assert_eq!(
            settings.frame(&[], Quat::IDENTITY),
            Err(FramingError::EmptyGroup)
        );
        assert_eq!(
            settings.frame(&[a; MAX_FRAMING_SUBJECTS + 1], Quat::IDENTITY),
            Err(FramingError::TooManySubjects)
        );
    }

    #[test]
    fn invalid_bounds_report_the_first_index_instead_of_dropping_subjects() {
        let valid = box_at(Vec3::ZERO, Vec3::ONE);
        for bad in [
            FramingBounds {
                min: Vec3::ONE,
                max: Vec3::ZERO,
            },
            FramingBounds {
                min: Vec3::splat(f32::NAN),
                max: Vec3::ZERO,
            },
            FramingBounds {
                min: Vec3::ZERO,
                max: Vec3::splat(f32::INFINITY),
            },
            FramingBounds {
                min: Vec3::ZERO,
                max: Vec3::splat(1e6 + 1.),
            },
        ] {
            assert_eq!(
                FramingSettings::default().frame(&[valid, bad, bad], Quat::IDENTITY),
                Err(FramingError::InvalidBounds { index: 1 })
            );
        }
    }

    #[test]
    fn invalid_settings_are_rejected_on_every_call() {
        let base = FramingSettings::default();
        let point = [box_at(Vec3::ZERO, Vec3::ZERO)];
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1., 0.] {
            for settings in [
                FramingSettings {
                    aspect_ratio: invalid,
                    ..base
                },
                FramingSettings {
                    fov_y: invalid,
                    ..base
                },
                FramingSettings {
                    near: invalid,
                    ..base
                },
                FramingSettings {
                    min_distance: invalid,
                    ..base
                },
                FramingSettings {
                    max_distance: invalid,
                    ..base
                },
            ] {
                assert_eq!(
                    settings.frame(&point, Quat::IDENTITY),
                    Err(FramingError::InvalidSettings)
                );
            }
        }
        for settings in [
            FramingSettings {
                margin: Vec2::new(-0.01, 0.),
                ..base
            },
            FramingSettings {
                margin: Vec2::new(0., 0.5),
                ..base
            },
            FramingSettings {
                margin: Vec2::splat(f32::NAN),
                ..base
            },
            FramingSettings {
                margin: Vec2::splat(f32::INFINITY),
                ..base
            },
            FramingSettings {
                fov_y: 0.049,
                ..base
            },
            FramingSettings {
                fov_y: 3.11,
                ..base
            },
            FramingSettings {
                min_distance: 0.001,
                ..base
            },
            FramingSettings {
                near: 0.0001,
                ..base
            },
            FramingSettings {
                min_distance: 2.,
                max_distance: 1.,
                ..base
            },
            FramingSettings {
                max_distance: 1e6,
                ..base
            },
        ] {
            assert_eq!(
                settings.frame(&point, Quat::IDENTITY),
                Err(FramingError::InvalidSettings)
            );
        }
        // The same value remains usable after an error; nothing is cached/mutated.
        assert_framed(base, &point, base.frame(&point, Quat::IDENTITY).unwrap());
    }

    #[test]
    fn invalid_rotations_and_unrepresentable_results_are_errors() {
        let settings = FramingSettings::default();
        let point = [box_at(Vec3::ZERO, Vec3::ZERO)];
        for rotation in [
            Quat::from_xyzw(0., 0., 0., 0.),
            Quat::from_xyzw(0., 0., 0., 2.),
            Quat::from_xyzw(f32::NAN, 0., 0., 1.),
            Quat::from_xyzw(0., f32::INFINITY, 0., 1.),
        ] {
            assert_eq!(
                settings.frame(&point, rotation),
                Err(FramingError::InvalidRotation)
            );
        }
        let extreme_aspect = FramingSettings {
            aspect_ratio: f32::from_bits(1),
            ..settings
        };
        assert_eq!(
            extreme_aspect.frame(&[box_at(Vec3::ZERO, Vec3::ONE)], Quat::IDENTITY),
            Err(FramingError::Unrepresentable)
        );
        let boundary = [box_at(Vec3::splat(1e6), Vec3::ZERO)];
        assert_eq!(
            settings.frame(&boundary, Quat::IDENTITY),
            Err(FramingError::Unrepresentable)
        );
        let tiny_boom = FramingSettings {
            min_distance: 0.01,
            near: 0.001,
            ..settings
        };
        assert_eq!(
            tiny_boom.frame(&[box_at(Vec3::splat(9e5), Vec3::ZERO)], Quat::IDENTITY),
            Err(FramingError::Unrepresentable)
        );
        // The eye remains distinct, but 900000 + 0.2 rounds to 900000.1875,
        // placing the target inside the requested near plane.
        let rounded_near = FramingSettings {
            min_distance: 0.2,
            near: 0.2,
            ..settings
        };
        assert_eq!(
            rounded_near.frame(&[box_at(Vec3::splat(9e5), Vec3::ZERO)], Quat::IDENTITY),
            Err(FramingError::Unrepresentable)
        );
    }

    #[test]
    fn fitting_a_rotated_pose_is_compatible_with_the_existing_rig() {
        let settings = FramingSettings::default();
        let subjects = [box_at(Vec3::new(1., 2., 3.), Vec3::new(2., 1., 0.5))];
        let frame = settings
            .frame(&subjects, Quat::from_rotation_y(0.6))
            .unwrap();
        let input = Input3 {
            target: Target3::at(frame.pose.target),
            mode: Mode3::Follow {
                eye_offset: frame.pose.position - frame.pose.target,
                look_offset: Vec3::ZERO,
            },
            fov_y: frame.pose.fov_y,
        };
        let rig_frame = Rig3::default().snap(input, &NoObstructions).unwrap();
        assert!(rig_frame.pose.position.distance(frame.pose.position) < 1e-5);
        assert!(rig_frame.pose.target.distance(frame.pose.target) < 1e-5);
        assert_eq!(rig_frame.pose.fov_y, frame.pose.fov_y);
        assert_framed(settings, &subjects, frame);
    }

    fn input() -> Input3 {
        Input3 {
            target: Target3 {
                velocity: Vec3::X * 20.,
                ..Target3::at(Vec3::ZERO)
            },
            mode: Mode3::Follow {
                eye_offset: Vec3::Y * 10.,
                look_offset: Vec3::ZERO,
            },
            fov_y: 1.,
        }
    }
    fn zone() -> CameraZone {
        CameraZone {
            min: Vec3::splat(-10.),
            max: Vec3::splat(10.),
            feather: 2.,
            priority: 1,
            focus_offset: Vec3::Z,
            boom_scale: 1.,
            fov_scale: 1.,
        }
    }
    #[test]
    fn anticipation_is_bounded_and_does_not_zoom() {
        let mut c = CameraComposer::new([zone()], 0.5, 2., 0.).unwrap();
        let i = c.update(input(), Vec3::ZERO, 0.1).unwrap();
        assert_eq!(i.target.position, Vec3::X * 2. + Vec3::Z);
        if let Mode3::Follow {
            eye_offset,
            look_offset,
        } = i.mode
        {
            assert_eq!(eye_offset.distance(look_offset), 10.);
        }
        let old = c;
        assert!(c.update(input(), Vec3::splat(f32::NAN), 0.1).is_err());
        assert_eq!(c, old);
    }
    #[test]
    fn priority_feather_and_first_person_are_explicit() {
        let mut high = zone();
        high.priority = 2;
        high.focus_offset = -Vec3::Z;
        let mut c = CameraComposer::new([zone(), high], 0., 0., 0.).unwrap();
        assert_eq!(
            c.update(input(), Vec3::ZERO, 0.1).unwrap().target.position,
            -Vec3::Z
        );
        let mut fp = input();
        fp.mode = Mode3::FirstPerson {
            eye_offset: Vec3::Y,
            yaw: 0.,
            pitch: 0.,
        };
        assert_eq!(c.update(fp, Vec3::ZERO, 0.1).unwrap(), fp);
    }
}
