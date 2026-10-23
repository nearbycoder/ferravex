//! Conservative transformed-bound visibility and per-object hysteretic LOD.
//!
//! [`VisibilityQuery`] reuses caller storage for bounded visible IDs and frame
//! diagnostics without allocating. It uses the same [`classify`] path as draws.
use crate::{Mat4, Vec3, retained::ResidentMesh};
use ferravex_graphics::{Bounds, Frustum};

#[derive(Clone, Copy, Debug)]
pub struct VisibilitySettings {
    pub frustum_culling: bool,
    /// Distance to the nearest point on the conservative world AABB, not its center.
    pub max_distance: Option<f32>,
}
impl Default for VisibilitySettings {
    fn default() -> Self {
        Self {
            frustum_culling: true,
            max_distance: None,
        }
    }
}
impl VisibilitySettings {
    pub fn valid(self) -> bool {
        self.max_distance.is_none_or(|n| n.is_finite() && n >= 0.)
    }
}
/// Classification counts. Use [`Self::record`] for saturating accumulation.
/// Invalid transforms contribute to `tested`, but not to the other fields.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct VisibilityStats {
    pub tested: u32,
    pub visible: u32,
    pub frustum_culled: u32,
    pub distance_culled: u32,
}
impl VisibilityStats {
    /// Record one classification, saturating each counter at [`u32::MAX`].
    /// Reset a frame by assigning [`Self::default`].
    ///
    /// ```
    /// use ferravex_render::visibility::{Visibility, VisibilityStats};
    /// let mut stats = VisibilityStats::default();
    /// stats.record(Visibility::Visible);
    /// stats.record(Visibility::TooDistant);
    /// assert_eq!((stats.tested, stats.visible, stats.culled()), (2, 1, 1));
    /// ```
    pub fn record(&mut self, visibility: Visibility) {
        self.tested = self.tested.saturating_add(1);
        let count = match visibility {
            Visibility::Visible => &mut self.visible,
            Visibility::OutsideFrustum => &mut self.frustum_culled,
            Visibility::TooDistant => &mut self.distance_culled,
            Visibility::InvalidTransform => return,
        };
        *count = count.saturating_add(1);
    }

    /// Saturating sum of frustum and distance culls, excluding invalid transforms.
    pub fn culled(self) -> u32 {
        self.frustum_culled.saturating_add(self.distance_culled)
    }
}

/// One query frame's diagnostics; all counts saturate at [`u32::MAX`].
/// `visibility.visible` includes IDs omitted because storage was full. Invalid
/// transforms are rejected separately from culls; invalid bounds stay visible.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct VisibilityQueryStats {
    pub visibility: VisibilityStats,
    pub invalid_transforms: u32,
    /// Visible IDs omitted from storage, not a count of culled objects.
    pub overflow: u32,
}
impl VisibilityQueryStats {
    /// True only when a visible ID was omitted, not merely when storage is full.
    ///
    /// ```
    /// use ferravex_render::visibility::VisibilityQueryStats;
    /// let stats = VisibilityQueryStats { overflow: 1, ..Default::default() };
    /// assert!(stats.is_truncated());
    /// ```
    pub fn is_truncated(self) -> bool {
        self.overflow != 0
    }
}

/// Invalid query configuration. A failed frame change preserves the prior frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityQueryError {
    /// The optional maximum distance must be finite and nonnegative.
    InvalidSettings,
    /// A supplied eye must have finite coordinates, even without distance culling.
    InvalidEye,
}
impl std::fmt::Display for VisibilityQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSettings => "visibility distance must be finite and nonnegative",
            Self::InvalidEye => "visibility eye must be finite",
        })
    }
}
impl std::error::Error for VisibilityQueryError {}

/// Reusable, allocation-free visible-ID storage and per-frame workload counts.
///
/// IDs are caller-defined `Copy` values, retained in submission order without
/// deduplication. The borrowed slice bounds storage, not classification work:
/// every [`Self::test`] is counted even when storage is full or empty. Always
/// inspect [`VisibilityQueryStats::is_truncated`] before treating IDs as complete.
/// No scene data is owned or changed. There is no implicit clock or frame reset.
///
/// ```
/// use ferravex_graphics::Bounds;
/// use ferravex_render::{Mat4, Vec3};
/// use ferravex_render::visibility::{Visibility, VisibilityQuery};
/// let bounds = Bounds { min: -Vec3::ONE, max: Vec3::ONE };
/// let mut storage = [0_u64; 1];
/// let mut query = VisibilityQuery::new(&mut storage, None, None, Default::default())?;
/// assert_eq!(query.capacity(), 1);
/// assert_eq!(query.test(7, bounds, Mat4::IDENTITY), Visibility::Visible);
/// assert!(!query.stats().is_truncated());
/// // Full storage does not change the object's classification.
/// assert_eq!(query.test(8, bounds, Mat4::IDENTITY), Visibility::Visible);
/// assert_eq!(query.visible_ids(), &[7]);
/// assert_eq!(query.stats().visibility.visible, 2);
/// assert_eq!(query.stats().overflow, 1);
/// query.begin_frame(None, None, Default::default())?;
/// assert!(query.visible_ids().is_empty());
/// assert_eq!(query.stats().visibility.tested, 0);
/// # Ok::<(), ferravex_render::visibility::VisibilityQueryError>(())
/// ```
#[derive(Debug)]
pub struct VisibilityQuery<'a, T: Copy> {
    storage: &'a mut [T],
    written: usize,
    stats: VisibilityQueryStats,
    frustum: Option<Frustum>,
    eye: Option<Vec3>,
    settings: VisibilitySettings,
}
impl<'a, T: Copy> VisibilityQuery<'a, T> {
    /// Start an empty frame in initialized caller storage, including zero capacity.
    /// Invalid settings/eye return an error without writing to the slice.
    pub fn new(
        storage: &'a mut [T],
        frustum: Option<Frustum>,
        eye: Option<Vec3>,
        settings: VisibilitySettings,
    ) -> Result<Self, VisibilityQueryError> {
        validate_query(eye, settings)?;
        Ok(Self {
            storage,
            written: 0,
            stats: VisibilityQueryStats::default(),
            frustum,
            eye,
            settings,
        })
    }

    /// Replace the camera/settings and reset IDs and counters, reusing storage.
    /// On error all previous configuration, IDs and counters remain unchanged.
    /// Reset hides stale IDs; it does not overwrite unused storage or allocate.
    pub fn begin_frame(
        &mut self,
        frustum: Option<Frustum>,
        eye: Option<Vec3>,
        settings: VisibilitySettings,
    ) -> Result<(), VisibilityQueryError> {
        validate_query(eye, settings)?;
        self.frustum = frustum;
        self.eye = eye;
        self.settings = settings;
        self.written = 0;
        self.stats = VisibilityQueryStats::default();
        Ok(())
    }

    /// Classify and count an object, retaining its ID when visible and space exists.
    /// Overflow increments `stats().overflow` but still returns `Visible`, so a
    /// caller can process the object immediately instead of dropping visible work.
    pub fn test(&mut self, id: T, bounds: Bounds, model: Mat4) -> Visibility {
        let visibility = classify(bounds, model, self.frustum, self.eye, self.settings);
        self.stats.visibility.record(visibility);
        match visibility {
            Visibility::Visible if self.written < self.storage.len() => {
                self.storage[self.written] = id;
                self.written += 1;
            }
            Visibility::Visible => self.stats.overflow = self.stats.overflow.saturating_add(1),
            Visibility::InvalidTransform => {
                self.stats.invalid_transforms = self.stats.invalid_transforms.saturating_add(1);
            }
            Visibility::OutsideFrustum | Visibility::TooDistant => {}
        }
        visibility
    }

    /// Only this frame's stored visible IDs, in submission order.
    pub fn visible_ids(&self) -> &[T] {
        &self.storage[..self.written]
    }

    /// Maximum stored IDs per frame; equal to the original slice length.
    pub fn capacity(&self) -> usize {
        self.storage.len()
    }

    /// Copy of this frame's counters, including classifications after storage fills.
    pub fn stats(&self) -> VisibilityQueryStats {
        self.stats
    }
}

fn validate_query(
    eye: Option<Vec3>,
    settings: VisibilitySettings,
) -> Result<(), VisibilityQueryError> {
    if !settings.valid() {
        return Err(VisibilityQueryError::InvalidSettings);
    }
    if eye.is_some_and(|eye| !eye.is_finite()) {
        return Err(VisibilityQueryError::InvalidEye);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    OutsideFrustum,
    TooDistant,
    InvalidTransform,
}
/// Distance to the nearest point on the conservative world AABB.
/// Invalid bounds, non-affine/nonfinite transforms, nonfinite eyes or overflow
/// in transformed bounds return zero: unknown bounds must not force a far LOD.
/// Valid distances larger than `f32::MAX` return infinity.
///
/// ```
/// use ferravex_graphics::Bounds;
/// use ferravex_render::{Mat4, Vec3, visibility::distance_to_bounds};
/// let bounds = Bounds { min: -Vec3::ONE, max: Vec3::ONE };
/// assert_eq!(distance_to_bounds(bounds, Mat4::IDENTITY, Vec3::X * 4.0), 3.0);
/// ```
pub fn distance_to_bounds(bounds: Bounds, model: Mat4, eye: Vec3) -> f32 {
    if !valid_transform(model) || !eye.is_finite() {
        return 0.;
    }
    let Some((center, half)) = world_bounds(bounds, model) else {
        return 0.;
    };
    bounds_distance(center, half, eye)
}

fn valid_transform(model: Mat4) -> bool {
    let a = model.to_cols_array();
    model.is_finite() && a[3] == 0. && a[7] == 0. && a[11] == 0. && a[15] == 1.
}

fn world_bounds(bounds: Bounds, model: Mat4) -> Option<(Vec3, Vec3)> {
    if !bounds.min.is_finite() || !bounds.max.is_finite() || !bounds.min.cmple(bounds.max).all() {
        return None;
    }
    let center = model.transform_point3(bounds.center());
    let h = bounds.half_extents();
    let half = model.x_axis.truncate().abs() * h.x
        + model.y_axis.truncate().abs() * h.y
        + model.z_axis.truncate().abs() * h.z;
    (center.is_finite() && half.is_finite()).then_some((center, half))
}

fn bounds_distance(center: Vec3, half: Vec3, eye: Vec3) -> f32 {
    // Subtraction can overflow f32 even when the final distance is representable.
    let distance = |eye: f32, center: f32, half: f32| {
        ((f64::from(eye) - f64::from(center)).abs() - f64::from(half)).max(0.)
    };
    let x = distance(eye.x, center.x, half.x);
    let y = distance(eye.y, center.y, half.y);
    let z = distance(eye.z, center.z, half.z);
    (x * x + y * y + z * z).sqrt() as f32
}

/// Classify with conservative transformed bounds; invalid transforms are rejected.
/// Invalid/reversed bounds or unrepresentable world bounds remain visible.
/// Missing frustum/eye disables the corresponding cull. An invalid distance limit
/// or nonfinite eye disables distance culling; [`VisibilityQuery`] instead rejects
/// those configuration errors before starting a frame.
///
/// ```
/// use ferravex_graphics::Bounds;
/// use ferravex_render::{Mat4, Vec3, visibility::{classify, Visibility}};
/// let unknown = Bounds { min: Vec3::NAN, max: Vec3::ONE };
/// assert_eq!(classify(unknown, Mat4::IDENTITY, None, None, Default::default()),
///            Visibility::Visible);
/// ```
pub fn classify(
    bounds: Bounds,
    model: Mat4,
    frustum: Option<Frustum>,
    eye: Option<Vec3>,
    settings: VisibilitySettings,
) -> Visibility {
    if !valid_transform(model) {
        return Visibility::InvalidTransform;
    }
    let Some((center, half)) = world_bounds(bounds, model) else {
        return Visibility::Visible;
    };
    // Keep the existing oriented support test for ordinary scene bounds. Avoid
    // overflow/NaN in its dot products and radius sums for extreme finite inputs.
    let safe_support = center.abs().element_sum().is_finite()
        && half.element_sum().is_finite()
        && [model.x_axis, model.y_axis, model.z_axis]
            .iter()
            .all(|axis| axis.truncate().abs().element_sum().is_finite());
    if settings.frustum_culling
        && safe_support
        && frustum.is_some_and(|f| !f.intersects_bounds(bounds, model))
    {
        return Visibility::OutsideFrustum;
    }
    if let (Some(eye), Some(max)) = (eye, settings.max_distance)
        && eye.is_finite()
        && settings.valid()
        && bounds_distance(center, half, eye) > max
    {
        return Visibility::TooDistant;
    }
    Visibility::Visible
}
#[derive(Clone, Copy, Default, Debug)]
pub struct LodState {
    level: usize,
    initialized: bool,
}
impl LodState {
    pub fn level(self) -> usize {
        self.level
    }
}
pub struct LodChain {
    levels: Vec<ResidentMesh>,
    thresholds: Vec<f32>,
    hysteresis: f32,
    bounds: Bounds,
}
impl LodChain {
    /// 1..=16 authored levels and strictly increasing positive transition distances.
    /// Threshold distance is measured to the union of all level bounds.
    pub fn new(
        levels: Vec<ResidentMesh>,
        thresholds: Vec<f32>,
        hysteresis: f32,
    ) -> Result<Self, &'static str> {
        if levels.is_empty()
            || levels.len() > 16
            || thresholds.len() + 1 != levels.len()
            || !hysteresis.is_finite()
            || !(0. ..=0.5).contains(&hysteresis)
            || thresholds.iter().any(|t| !t.is_finite() || *t <= 0.)
            || thresholds.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("invalid LOD levels/thresholds/hysteresis");
        }
        let mut bounds = levels[0].bounds();
        for level in &levels[1..] {
            let b = level.bounds();
            bounds.min = bounds.min.min(b.min);
            bounds.max = bounds.max.max(b.max);
        }
        Ok(Self {
            levels,
            thresholds,
            hysteresis,
            bounds,
        })
    }
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }
    pub fn select(&self, distance: f32, state: &mut LodState) -> &ResidentMesh {
        let distance = if distance.is_finite() {
            distance.max(0.)
        } else {
            0.
        };
        if !state.initialized || state.level >= self.levels.len() {
            state.level = self.thresholds.partition_point(|t| distance > *t);
            state.initialized = true;
        } else {
            while state.level < self.thresholds.len()
                && distance > self.thresholds[state.level] * (1. + self.hysteresis)
            {
                state.level += 1;
            }
            while state.level > 0
                && distance < self.thresholds[state.level - 1] * (1. - self.hysteresis)
            {
                state.level -= 1;
            }
        }
        &self.levels[state.level]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WHITE, mesh::Mesh3d};
    use ferravex_graphics::ClipDepth;

    fn unit_bounds() -> Bounds {
        Bounds {
            min: -Vec3::ONE,
            max: Vec3::ONE,
        }
    }

    #[test]
    fn query_counts_all_work_after_storage_fills() {
        let mut storage = [99; 1];
        let frustum = Frustum::from_matrix(Mat4::IDENTITY, ClipDepth::NegativeOneToOne);
        let settings = VisibilitySettings {
            max_distance: Some(1.),
            ..Default::default()
        };
        let mut query =
            VisibilityQuery::new(&mut storage, frustum, Some(Vec3::ZERO), settings).unwrap();
        assert_eq!(
            query.test(7, unit_bounds(), Mat4::IDENTITY),
            Visibility::Visible
        );
        assert!(!query.stats().is_truncated());
        assert_eq!(
            query.test(8, unit_bounds(), Mat4::IDENTITY),
            Visibility::Visible
        );
        assert_eq!(
            query.test(9, unit_bounds(), Mat4::from_translation(Vec3::X * 4.)),
            Visibility::OutsideFrustum
        );
        // A corner of the frustum is farther than one unit from the eye.
        let point = Bounds {
            min: Vec3::ONE,
            max: Vec3::ONE,
        };
        assert_eq!(
            query.test(10, point, Mat4::IDENTITY),
            Visibility::TooDistant
        );
        let mut invalid = Mat4::IDENTITY;
        invalid.x_axis.w = 0.5;
        assert_eq!(
            query.test(11, unit_bounds(), invalid),
            Visibility::InvalidTransform
        );
        assert_eq!(query.visible_ids(), &[7]);
        assert_eq!(query.capacity(), 1);
        assert_eq!(
            query.stats(),
            VisibilityQueryStats {
                visibility: VisibilityStats {
                    tested: 5,
                    visible: 2,
                    frustum_culled: 1,
                    distance_culled: 1,
                },
                invalid_transforms: 1,
                overflow: 1,
            }
        );
        assert_eq!(query.stats().visibility.culled(), 2);
        assert!(query.stats().is_truncated());
    }

    #[test]
    fn zero_capacity_query_still_classifies_every_object() {
        let mut query = VisibilityQuery::new(&mut [], None, None, Default::default()).unwrap();
        assert_eq!(
            query.test(1_u64, unit_bounds(), Mat4::IDENTITY),
            Visibility::Visible
        );
        assert_eq!(
            query.test(2, unit_bounds(), Mat4::ZERO),
            Visibility::InvalidTransform
        );
        assert!(query.visible_ids().is_empty());
        assert_eq!(query.capacity(), 0);
        assert_eq!(query.stats().visibility.tested, 2);
        assert_eq!(query.stats().visibility.visible, 1);
        assert_eq!(query.stats().invalid_transforms, 1);
        assert_eq!(query.stats().overflow, 1);
        query.begin_frame(None, None, Default::default()).unwrap();
        assert_eq!(query.stats(), VisibilityQueryStats::default());
    }

    #[test]
    fn frames_reuse_storage_hide_stale_ids_and_preserve_submission_order() {
        let mut storage = [99; 4];
        let pointer = storage.as_ptr();
        {
            let mut query =
                VisibilityQuery::new(&mut storage, None, None, Default::default()).unwrap();
            for id in [9, 2, 9, 1] {
                assert_eq!(
                    query.test(id, unit_bounds(), Mat4::IDENTITY),
                    Visibility::Visible
                );
            }
            assert_eq!(query.visible_ids(), &[9, 2, 9, 1]);
            assert_eq!(query.visible_ids().as_ptr(), pointer);
            assert!(!query.stats().is_truncated());
            query
                .begin_frame(
                    None,
                    Some(Vec3::ZERO),
                    VisibilitySettings {
                        max_distance: Some(0.),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert!(query.visible_ids().is_empty());
            assert_eq!(query.stats(), VisibilityQueryStats::default());
            assert_eq!(
                query.test(3, unit_bounds(), Mat4::IDENTITY),
                Visibility::Visible
            );
            assert_eq!(
                query.test(4, unit_bounds(), Mat4::from_translation(Vec3::X * 4.)),
                Visibility::TooDistant
            );
            assert_eq!(query.visible_ids(), &[3]);
            assert_eq!(query.visible_ids().as_ptr(), pointer);
            assert_eq!(query.capacity(), 4);
            assert_eq!(query.stats().visibility.tested, 2);
        }
        // Only the written prefix changes; old suffix values are never exposed by query results.
        assert_eq!(storage, [3, 2, 9, 1]);
    }

    #[test]
    fn query_configuration_errors_are_atomic() {
        let mut storage = [41_u64; 2];
        for max_distance in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let error = VisibilityQuery::new(
                &mut storage,
                None,
                None,
                VisibilitySettings {
                    max_distance: Some(max_distance),
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert_eq!(error, VisibilityQueryError::InvalidSettings);
        }
        for eye in [Vec3::NAN, Vec3::INFINITY, Vec3::NEG_INFINITY] {
            let error = VisibilityQuery::new(&mut storage, None, Some(eye), Default::default())
                .unwrap_err();
            assert_eq!(error, VisibilityQueryError::InvalidEye);
        }
        assert_eq!(storage, [41; 2]);

        let settings = VisibilitySettings {
            max_distance: Some(1.),
            ..Default::default()
        };
        let mut query =
            VisibilityQuery::new(&mut storage, None, Some(Vec3::ZERO), settings).unwrap();
        query.test(7, unit_bounds(), Mat4::IDENTITY);
        let stats = query.stats();
        let frustum = Frustum::from_matrix(Mat4::IDENTITY, ClipDepth::NegativeOneToOne);
        assert_eq!(
            query.begin_frame(
                frustum,
                None,
                VisibilitySettings {
                    max_distance: Some(-1.),
                    ..Default::default()
                }
            ),
            Err(VisibilityQueryError::InvalidSettings)
        );
        assert_eq!(
            query.begin_frame(frustum, Some(Vec3::NAN), Default::default()),
            Err(VisibilityQueryError::InvalidEye)
        );
        assert_eq!(query.visible_ids(), &[7]);
        assert_eq!(query.stats(), stats);
        // Neither failed call installed its frustum, eye or settings.
        assert_eq!(
            query.test(8, unit_bounds(), Mat4::from_translation(Vec3::X * 4.)),
            Visibility::TooDistant
        );
    }

    #[test]
    fn workload_counters_saturate_and_reset() {
        let mut stats = VisibilityStats {
            tested: u32::MAX,
            visible: u32::MAX,
            frustum_culled: u32::MAX,
            distance_culled: u32::MAX,
        };
        let full = stats;
        for outcome in [
            Visibility::Visible,
            Visibility::OutsideFrustum,
            Visibility::TooDistant,
            Visibility::InvalidTransform,
        ] {
            stats.record(outcome);
            assert_eq!(stats, full);
        }
        assert_eq!(stats.culled(), u32::MAX);
        let mut query = VisibilityQuery::new(&mut [], None, None, Default::default()).unwrap();
        query.stats = VisibilityQueryStats {
            visibility: full,
            invalid_transforms: u32::MAX,
            overflow: u32::MAX,
        };
        let full = query.stats();
        query.test(1, unit_bounds(), Mat4::IDENTITY);
        query.test(2, unit_bounds(), Mat4::ZERO);
        assert_eq!(query.stats(), full);
        query.begin_frame(None, None, Default::default()).unwrap();
        assert_eq!(query.stats(), VisibilityQueryStats::default());
    }

    #[test]
    fn invalid_bounds_fail_open_but_invalid_transforms_still_reject() {
        let frustum = Frustum::from_matrix(Mat4::IDENTITY, ClipDepth::NegativeOneToOne);
        let settings = VisibilitySettings {
            max_distance: Some(0.),
            ..Default::default()
        };
        let mut storage = [0; 4];
        let mut query =
            VisibilityQuery::new(&mut storage, frustum, Some(Vec3::ZERO), settings).unwrap();
        for (id, bounds) in [
            Bounds {
                min: Vec3::NAN,
                max: Vec3::ONE,
            },
            Bounds {
                min: -Vec3::ONE,
                max: Vec3::INFINITY,
            },
            Bounds {
                min: Vec3::NEG_INFINITY,
                max: Vec3::ONE,
            },
            Bounds {
                min: Vec3::new(-1., 2., -1.),
                max: Vec3::ONE,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let model = Mat4::from_translation(Vec3::X * 100.);
            assert_eq!(query.test(id, bounds, model), Visibility::Visible);
            assert_eq!(distance_to_bounds(bounds, model, Vec3::ZERO), 0.);
            assert_eq!(
                classify(bounds, Mat4::ZERO, frustum, Some(Vec3::ZERO), settings),
                Visibility::InvalidTransform
            );
        }
        assert_eq!(query.visible_ids(), &[0, 1, 2, 3]);
        assert_eq!(query.stats().visibility.visible, 4);
        assert_eq!(query.stats().visibility.culled(), 0);
    }

    #[test]
    fn extreme_bounds_do_not_become_false_frustum_culls() {
        let frustum = Frustum::from_matrix(Mat4::IDENTITY, ClipDepth::NegativeOneToOne);
        for (bounds, model) in [
            (
                Bounds {
                    min: Vec3::splat(-f32::MAX),
                    max: Vec3::splat(f32::MAX),
                },
                Mat4::IDENTITY,
            ),
            (
                Bounds {
                    min: Vec3::splat(f32::MAX * 0.75),
                    max: Vec3::splat(f32::MAX),
                },
                Mat4::IDENTITY,
            ),
            (unit_bounds(), Mat4::from_scale(Vec3::splat(f32::MAX))),
            (
                unit_bounds(),
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(f32::MAX),
                    crate::Quat::IDENTITY,
                    Vec3::splat(f32::MAX),
                ),
            ),
        ] {
            assert_eq!(
                classify(bounds, model, frustum, None, Default::default()),
                Visibility::Visible
            );
        }
    }

    #[test]
    fn invalid_distance_configuration_does_not_create_a_false_cull() {
        let bounds = unit_bounds();
        for max in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let settings = VisibilitySettings {
                max_distance: Some(max),
                ..Default::default()
            };
            assert_eq!(
                classify(bounds, Mat4::IDENTITY, None, Some(Vec3::X * 100.), settings),
                Visibility::Visible
            );
        }
        for eye in [Vec3::NAN, Vec3::INFINITY, Vec3::NEG_INFINITY] {
            assert_eq!(distance_to_bounds(bounds, Mat4::IDENTITY, eye), 0.);
            assert_eq!(
                classify(
                    bounds,
                    Mat4::IDENTITY,
                    None,
                    Some(eye),
                    VisibilitySettings {
                        max_distance: Some(0.),
                        ..Default::default()
                    }
                ),
                Visibility::Visible
            );
        }
        let mut nonfinite = Mat4::IDENTITY;
        nonfinite.w_axis.x = f32::NAN;
        assert_eq!(
            classify(bounds, nonfinite, None, None, Default::default()),
            Visibility::InvalidTransform
        );
        assert_eq!(distance_to_bounds(bounds, nonfinite, Vec3::ZERO), 0.);
    }

    #[test]
    fn distance_uses_nearest_point_without_intermediate_subtraction_overflow() {
        let model = Mat4::from_scale_rotation_translation(
            Vec3::splat(1.0e38),
            crate::Quat::IDENTITY,
            Vec3::X * -2.0e38,
        );
        let eye = Vec3::X * 2.0e38;
        let distance = distance_to_bounds(unit_bounds(), model, eye);
        assert!(distance.is_finite());
        assert!((distance / 3.0e38 - 1.).abs() < 1.0e-6);
        let settings = VisibilitySettings {
            frustum_culling: false,
            max_distance: Some(3.1e38),
        };
        assert_eq!(
            classify(unit_bounds(), model, None, Some(eye), settings),
            Visibility::Visible
        );
        assert_eq!(
            classify(
                unit_bounds(),
                model,
                None,
                Some(eye),
                VisibilitySettings {
                    max_distance: Some(2.9e38),
                    ..settings
                }
            ),
            Visibility::TooDistant
        );
    }

    #[test]
    fn query_matches_existing_culling_for_scene_transforms() {
        let frustum = Frustum::from_matrix(Mat4::IDENTITY, ClipDepth::NegativeOneToOne).unwrap();
        let mut storage = [0; 6];
        let mut query =
            VisibilityQuery::new(&mut storage, Some(frustum), None, Default::default()).unwrap();
        let mut shear = Mat4::IDENTITY;
        shear.y_axis.x = 3.;
        let transforms = [
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::X * 4.),
            Mat4::from_translation(Vec3::Z * 1.5),
            Mat4::from_scale(Vec3::new(-5., 2., 1.)),
            Mat4::from_rotation_y(0.6),
            shear,
        ];
        let mut visible = Vec::new();
        for (id, model) in transforms.into_iter().enumerate() {
            let intersects = frustum.intersects_bounds(unit_bounds(), model);
            assert_eq!(
                query.test(id, unit_bounds(), model) == Visibility::Visible,
                intersects
            );
            if intersects {
                visible.push(id);
            }
        }
        assert_eq!(query.visible_ids(), visible);
        assert_eq!(query.stats().visibility.tested, 6);
        assert!(!query.stats().is_truncated());
    }

    #[test]
    fn conservative_visibility_handles_near_plane_scale_and_large_objects() {
        let bounds = Bounds {
            min: Vec3::splat(-1.),
            max: Vec3::ONE,
        };
        let f = Frustum::from_matrix(Mat4::IDENTITY, ClipDepth::NegativeOneToOne);
        assert_eq!(
            classify(bounds, Mat4::IDENTITY, f, None, Default::default()),
            Visibility::Visible
        );
        assert_eq!(
            classify(
                bounds,
                Mat4::from_translation(Vec3::X * 4.),
                f,
                None,
                Default::default()
            ),
            Visibility::OutsideFrustum
        );
        assert_eq!(
            classify(
                bounds,
                Mat4::from_translation(Vec3::Z * 1.5),
                f,
                None,
                Default::default()
            ),
            Visibility::Visible
        );
        assert_eq!(
            classify(
                bounds,
                Mat4::from_scale(Vec3::new(-5., 2., 1.)),
                f,
                None,
                Default::default()
            ),
            Visibility::Visible
        );
        let settings = VisibilitySettings {
            max_distance: Some(1.),
            ..Default::default()
        };
        assert_eq!(
            classify(
                bounds,
                Mat4::from_scale(Vec3::splat(100.)),
                None,
                Some(Vec3::X * 50.),
                settings
            ),
            Visibility::Visible
        );
        assert_eq!(
            classify(bounds, Mat4::IDENTITY, None, Some(Vec3::X * 50.), settings),
            Visibility::TooDistant
        );
    }
    #[test]
    fn lod_hysteresis_retains_level_near_boundary_and_handles_camera_jumps() {
        let mesh = Mesh3d::cuboid(Vec3::ONE, WHITE).resident().clone();
        let chain =
            LodChain::new(vec![mesh.clone(), mesh.clone(), mesh], vec![10., 30.], 0.1).unwrap();
        let mut state = LodState::default();
        for (distance, level) in [
            (0., 0),
            (10.5, 0),
            (12., 1),
            (9.5, 1),
            (8., 0),
            (100., 2),
            (0., 0),
        ] {
            chain.select(distance, &mut state);
            assert_eq!(state.level(), level);
        }
        let mut fresh = LodState::default();
        chain.select(100., &mut fresh);
        assert_eq!(fresh.level(), 2);
    }
    #[test]
    fn invalid_lod_configs_are_rejected() {
        assert!(LodChain::new(vec![], vec![], 0.1).is_err());
        let mesh = Mesh3d::cuboid(Vec3::ONE, WHITE).resident().clone();
        assert!(LodChain::new(vec![mesh.clone(), mesh.clone()], vec![f32::NAN], 0.1).is_err());
        assert!(LodChain::new(vec![mesh.clone()], vec![], 1.).is_err());
        assert!(
            LodChain::new(vec![mesh.clone(), mesh.clone(), mesh], vec![20., 10.], 0.1).is_err()
        );
    }
}
