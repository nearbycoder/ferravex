//! Parametric solid blueprints shared by rendering, collision and authoring.
//! Construct once, then instantiate the exact same pieces in both hosts.
//!
//! Each shape builder validates its entire request before writing pieces. Use
//! [`EnvironmentPlan::append_atomic`] to group several requests into one edit.
use ferravex_math::{Quat, Vec3};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    Invalid,
    Capacity,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxPiece {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub rotation: Quat,
    pub material: u16,
}
impl BoxPiece {
    pub fn bounds(self) -> (Vec3, Vec3) {
        let h = (self.rotation * Vec3::X).abs() * self.half_extents.x
            + (self.rotation * Vec3::Y).abs() * self.half_extents.y
            + (self.rotation * Vec3::Z).abs() * self.half_extents.z;
        (self.center - h, self.center + h)
    }
}
/// A bounded sequence of authored boxes. Ramps use one box; stairs and bridges
/// expose checked piece counts for capacity planning. Bounds include all solids,
/// including ramp thickness and bridge rails/posts.
///
/// ```
/// use ferravex_math::Vec3;
/// use ferravex_play::environment::{BuildError, EnvironmentPlan};
///
/// let count = EnvironmentPlan::stairs_piece_count(4)?
///     + EnvironmentPlan::bridge_piece_count(2)? + 1; // One ramp.
/// let mut plan = EnvironmentPlan::new(count)?;
/// assert_eq!(plan.capacity(), count);
/// assert_eq!(plan.remaining_capacity(), count);
/// assert_eq!(plan.piece_count(), 0);
/// assert_eq!(plan.bounds(), None);
/// plan.stairs(Vec3::ZERO, 2.0, 0.25, 0.5, 4, 3)?;
/// assert_eq!(plan.piece_count(), 4);
/// assert_eq!(plan.remaining_capacity(), count - 4);
/// assert_eq!(plan.bounds(), Some((Vec3::new(-1.0, 0.0, 0.0),
///                               Vec3::new(1.0, 1.0, 2.0))));
/// # Ok::<(), BuildError>(())
/// ```
#[derive(Clone, Debug)]
pub struct EnvironmentPlan {
    pieces: Vec<BoxPiece>,
    capacity: usize,
}
fn dimension(v: f32) -> bool {
    v.is_finite() && (0.001..=1000.).contains(&v)
}
impl EnvironmentPlan {
    pub fn new(capacity: usize) -> Result<Self, BuildError> {
        if capacity == 0 || capacity > 1024 {
            return Err(BuildError::Capacity);
        }
        Ok(Self {
            pieces: Vec::with_capacity(capacity),
            capacity,
        })
    }
    pub fn pieces(&self) -> &[BoxPiece] {
        &self.pieces
    }

    /// Number of authored boxes, in the order returned by [`Self::pieces`].
    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }

    /// Maximum total piece count for this plan, independent of allocator sizing.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of additional boxes this plan can accept.
    pub fn remaining_capacity(&self) -> usize {
        self.capacity - self.piece_count()
    }

    /// Union of every piece's world-axis-aligned [`BoxPiece::bounds`], or `None`
    /// for an empty plan. Visits each piece once without allocating.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut pieces = self.pieces.iter();
        let first = pieces.next()?.bounds();
        Some(pieces.fold(first, |(min, max), piece| {
            let (piece_min, piece_max) = piece.bounds();
            (min.min(piece_min), max.max(piece_max))
        }))
    }

    /// Boxes required by [`Self::stairs`]. Rejects counts outside `1..=128`.
    /// This checks the count only; the builder also checks origin and dimensions.
    pub fn stairs_piece_count(count: u16) -> Result<usize, BuildError> {
        if !(1..=128).contains(&count) {
            return Err(BuildError::Invalid);
        }
        Ok(usize::from(count))
    }

    /// Boxes required by [`Self::bridge`]: one deck, two rails and two posts at
    /// each of `segments + 1` stations. Rejects segment counts outside `1..=128`.
    /// This checks the count only; the builder also checks origin and dimensions.
    pub fn bridge_piece_count(segments: u16) -> Result<usize, BuildError> {
        if !(1..=128).contains(&segments) {
            return Err(BuildError::Invalid);
        }
        Ok(3 + 2 * (usize::from(segments) + 1))
    }

    /// Append a group of shapes only if `build` succeeds and every staged piece
    /// fits. An error leaves this plan's pieces, bounds and capacity unchanged.
    ///
    /// The callback receives an **empty** temporary plan limited to this plan's
    /// remaining capacity, including zero when full. Propagate builder errors
    /// with `?`: deliberately handled errors do not abort the group. The final
    /// count is checked again even if the callback replaces the temporary plan.
    /// Existing pieces are not visible in the temporary plan.
    ///
    /// This authoring operation allocates temporary storage for at most the
    /// remaining capacity (at most 1024 boxes). Individual shape builders still
    /// write directly into their plan. The callback's external side effects are
    /// not rolled back; if it unwinds, the destination plan remains unchanged.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::environment::{BuildError, EnvironmentPlan};
    ///
    /// let mut plan = EnvironmentPlan::new(12)?;
    /// plan.ramp(Vec3::ZERO, 2.0, 1.0, 3.0, 0.2, 0)?;
    /// let before = plan.pieces().to_vec();
    /// let result = plan.append_atomic(|group| {
    ///     group.stairs(Vec3::ZERO, 2.0, 0.25, 0.5, 4, 1)?;
    ///     group.bridge(Vec3::ZERO, 2.0, 4.0, 1.0, 2, 2) // Needs nine more.
    /// });
    /// assert_eq!(result, Err(BuildError::Capacity));
    /// assert_eq!(plan.pieces(), before);
    /// plan.append_atomic(|group| {
    ///     group.stairs(Vec3::ZERO, 2.0, 0.25, 0.5, 4, 1)?;
    ///     group.bridge(Vec3::ZERO, 2.0, 4.0, 1.0, 1, 2) // Seven fits exactly.
    /// })?;
    /// assert_eq!(plan.remaining_capacity(), 0);
    /// # Ok::<(), BuildError>(())
    /// ```
    pub fn append_atomic(
        &mut self,
        build: impl FnOnce(&mut Self) -> Result<(), BuildError>,
    ) -> Result<(), BuildError> {
        let remaining = self.remaining_capacity();
        let mut group = Self {
            pieces: Vec::with_capacity(remaining),
            capacity: remaining,
        };
        build(&mut group)?;
        if group.piece_count() > remaining {
            return Err(BuildError::Capacity);
        }
        self.pieces.extend_from_slice(group.pieces());
        Ok(())
    }

    fn validate(&self, origin: Vec3, dimensions: &[f32], count: usize) -> Result<(), BuildError> {
        if !origin.is_finite()
            || origin.abs().max_element() > 100000.
            || !dimensions.iter().all(|v| dimension(*v))
        {
            return Err(BuildError::Invalid);
        }
        if count > self.remaining_capacity() {
            return Err(BuildError::Capacity);
        }
        Ok(())
    }
    /// Origin is the low edge at floor height. Each solid riser extends to floor.
    pub fn stairs(
        &mut self,
        origin: Vec3,
        width: f32,
        rise: f32,
        run: f32,
        count: u16,
        material: u16,
    ) -> Result<(), BuildError> {
        let pieces = Self::stairs_piece_count(count)?;
        self.validate(origin, &[width, rise, run], pieces)?;
        for i in 0..count {
            let h = rise * f32::from(i + 1);
            self.pieces.push(BoxPiece {
                center: origin + Vec3::new(0., h * 0.5, (f32::from(i) + 0.5) * run),
                half_extents: Vec3::new(width * 0.5, h * 0.5, run * 0.5),
                rotation: Quat::IDENTITY,
                material,
            });
        }
        Ok(())
    }
    /// A single oriented slab; rise/run specify its centerline endpoints.
    pub fn ramp(
        &mut self,
        origin: Vec3,
        width: f32,
        rise: f32,
        run: f32,
        thickness: f32,
        material: u16,
    ) -> Result<(), BuildError> {
        self.validate(origin, &[width, rise, run, thickness], 1)?;
        self.pieces.push(BoxPiece {
            center: origin + Vec3::new(0., rise * 0.5, run * 0.5),
            half_extents: Vec3::new(width * 0.5, thickness * 0.5, rise.hypot(run) * 0.5),
            rotation: Quat::from_rotation_x(-rise.atan2(run)),
            material,
        });
        Ok(())
    }
    /// Straight deck, rail beams and evenly spaced support posts. All parts are
    /// actual solids, including rails; no invisible collision approximation.
    pub fn bridge(
        &mut self,
        origin: Vec3,
        width: f32,
        length: f32,
        height: f32,
        segments: u16,
        material: u16,
    ) -> Result<(), BuildError> {
        let count = Self::bridge_piece_count(segments)?;
        self.validate(origin, &[width, length, height], count)?;
        self.pieces.push(BoxPiece {
            center: origin + Vec3::new(0., height, 0.),
            half_extents: Vec3::new(width * 0.5, 0.12, length * 0.5),
            rotation: Quat::IDENTITY,
            material,
        });
        for side in [-1., 1.] {
            let x = side * width * 0.5;
            self.pieces.push(BoxPiece {
                center: origin + Vec3::new(x, height + 0.9, 0.),
                half_extents: Vec3::new(0.06, 0.06, length * 0.5),
                rotation: Quat::IDENTITY,
                material,
            });
            for i in 0..=segments {
                self.pieces.push(BoxPiece {
                    center: origin
                        + Vec3::new(
                            x,
                            (height + 1.) * 0.5,
                            (f32::from(i) / f32::from(segments) - 0.5) * length,
                        ),
                    half_extents: Vec3::new(0.07, (height + 1.) * 0.5, 0.07),
                    rotation: Quat::IDENTITY,
                    material,
                });
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stairs_and_ramp_have_matching_authored_endpoints() {
        let mut p = EnvironmentPlan::new(16).unwrap();
        p.stairs(Vec3::ZERO, 2., 0.2, 0.5, 6, 0).unwrap();
        assert!((p.pieces()[5].bounds().1.y - 1.2).abs() < 1e-6);
        p.ramp(Vec3::ZERO, 2., 1.2, 3., 0.2, 1).unwrap();
        let r = p.pieces()[6];
        let axis = r.rotation * Vec3::Z * r.half_extents.z;
        assert!((r.center - axis).length() < 1e-5);
        assert!((r.center + axis - Vec3::new(0., 1.2, 3.)).length() < 1e-5);
    }
    #[test]
    fn rejected_builds_are_transactional_and_plans_repeat() {
        let mut p = EnvironmentPlan::new(5).unwrap();
        p.stairs(Vec3::ZERO, 2., 0.2, 0.5, 3, 0).unwrap();
        let old = p.pieces().to_vec();
        assert_eq!(
            p.bridge(Vec3::ZERO, 2., 3., 1., 2, 0),
            Err(BuildError::Capacity)
        );
        assert_eq!(p.pieces(), old);
        assert_eq!(
            p.ramp(Vec3::ZERO, 2., f32::NAN, 3., 0.2, 0),
            Err(BuildError::Invalid)
        );
        assert_eq!(p.pieces(), old);
    }

    #[test]
    fn checked_counts_match_builders_at_exact_capacity() {
        for count in [1, 2, 64, 128] {
            let stairs = EnvironmentPlan::stairs_piece_count(count).unwrap();
            assert_eq!(stairs, usize::from(count));
            let mut plan = EnvironmentPlan::new(stairs).unwrap();
            plan.stairs(Vec3::ZERO, 2., 0.25, 0.5, count, 17).unwrap();
            assert_eq!(plan.piece_count(), stairs);
            assert_eq!(plan.remaining_capacity(), 0);
            assert!(plan.pieces().iter().all(|piece| piece.material == 17));

            let bridge = EnvironmentPlan::bridge_piece_count(count).unwrap();
            assert_eq!(bridge, 5 + 2 * usize::from(count));
            let mut plan = EnvironmentPlan::new(bridge).unwrap();
            plan.bridge(Vec3::ZERO, 2., 4., 1., count, 23).unwrap();
            assert_eq!(plan.piece_count(), bridge);
            assert_eq!(plan.remaining_capacity(), 0);
            assert!(plan.pieces().iter().all(|piece| piece.material == 23));
        }
        for count in [0, 129, u16::MAX] {
            assert_eq!(
                EnvironmentPlan::stairs_piece_count(count),
                Err(BuildError::Invalid)
            );
            assert_eq!(
                EnvironmentPlan::bridge_piece_count(count),
                Err(BuildError::Invalid)
            );
        }
    }

    type Build = fn(&mut EnvironmentPlan) -> Result<(), BuildError>;

    #[test]
    fn every_builder_rejects_capacity_without_changing_existing_geometry() {
        let builds: [Build; 3] = [
            |p| p.stairs(Vec3::ZERO, 2., 0.25, 0.5, 1, 0),
            |p| p.ramp(Vec3::ZERO, 2., 1., 3., 0.2, 0),
            |p| p.bridge(Vec3::ZERO, 2., 4., 1., 1, 0),
        ];
        let mut plan = EnvironmentPlan::new(1).unwrap();
        plan.ramp(Vec3::Y, 2., 1., 3., 0.2, 19).unwrap();
        let before = plan.pieces().to_vec();
        let bounds = plan.bounds();
        for build in builds {
            assert_eq!(build(&mut plan), Err(BuildError::Capacity));
            assert_eq!(plan.pieces(), before);
            assert_eq!(plan.bounds(), bounds);
            assert_eq!((plan.capacity(), plan.remaining_capacity()), (1, 0));
        }
    }

    #[test]
    fn invalid_geometry_and_counts_take_precedence_even_when_full() {
        let builds: [Build; 9] = [
            |p| p.stairs(Vec3::ZERO, 0., 0.25, 0.5, 1, 0),
            |p| p.stairs(Vec3::ZERO, 2., 0.25, 0.5, 129, 0),
            |p| p.stairs(Vec3::splat(f32::NAN), 2., 0.25, 0.5, 1, 0),
            |p| p.ramp(Vec3::ZERO, 2., -1., 3., 0.2, 0),
            |p| p.ramp(Vec3::ZERO, 2., 1., 3., f32::INFINITY, 0),
            |p| p.ramp(Vec3::splat(100001.), 2., 1., 3., 0.2, 0),
            |p| p.bridge(Vec3::ZERO, 2., 1001., 1., 1, 0),
            |p| p.bridge(Vec3::ZERO, 2., 4., 1., 0, 0),
            |p| p.bridge(Vec3::splat(f32::NEG_INFINITY), 2., 4., 1., 1, 0),
        ];
        let mut plan = EnvironmentPlan::new(1).unwrap();
        plan.ramp(Vec3::Y, 2., 1., 3., 0.2, 19).unwrap();
        let before = plan.pieces().to_vec();
        for build in builds {
            assert_eq!(build(&mut plan), Err(BuildError::Invalid));
            assert_eq!(plan.pieces(), before);
        }
    }

    #[test]
    fn compound_failure_rolls_back_earlier_successes_for_each_shape() {
        let builds: [(Build, BuildError); 6] = [
            (
                |p| p.stairs(Vec3::ZERO, 2., 0.25, 0.5, 5, 0),
                BuildError::Capacity,
            ),
            (
                |p| p.ramp(Vec3::ZERO, 2., 1., 3., 0.2, 0),
                BuildError::Capacity,
            ),
            (
                |p| p.bridge(Vec3::ZERO, 2., 4., 1., 1, 0),
                BuildError::Capacity,
            ),
            (
                |p| p.stairs(Vec3::ZERO, 2., f32::NAN, 0.5, 2, 0),
                BuildError::Invalid,
            ),
            (
                |p| p.ramp(Vec3::ZERO, 2., 1., 0., 0.2, 0),
                BuildError::Invalid,
            ),
            (
                |p| p.bridge(Vec3::ZERO, 2., 4., -1., 1, 0),
                BuildError::Invalid,
            ),
        ];
        let mut plan = EnvironmentPlan::new(5).unwrap();
        plan.ramp(Vec3::Y, 2., 1., 3., 0.2, 19).unwrap();
        let before = plan.pieces().to_vec();
        let bounds = plan.bounds();
        for (build, error) in builds {
            assert_eq!(
                plan.append_atomic(|group| {
                    assert_eq!(group.piece_count(), 0);
                    assert_eq!(group.bounds(), None);
                    assert_eq!(group.capacity(), 4);
                    group.stairs(Vec3::ZERO, 2., 0.25, 0.5, 3, 1)?;
                    group.ramp(Vec3::ZERO, 2., 1., 3., 0.2, 2)?;
                    build(group)
                }),
                Err(error)
            );
            assert_eq!(plan.pieces(), before);
            assert_eq!(plan.bounds(), bounds);
            assert_eq!((plan.capacity(), plan.remaining_capacity()), (5, 4));
        }
        // Failure does not consume capacity: a later edit can fill it exactly.
        plan.append_atomic(|p| p.stairs(Vec3::ZERO, 2., 0.25, 0.5, 4, 1))
            .unwrap();
        assert_eq!(plan.remaining_capacity(), 0);
    }

    fn compound(plan: &mut EnvironmentPlan) -> Result<(), BuildError> {
        plan.stairs(Vec3::new(1., 2., 3.), 2., 0.25, 0.5, 3, 4)?;
        plan.ramp(Vec3::new(-4., -2., -1.), 2., 1.5, 3., 0.2, 5)?;
        plan.bridge(Vec3::new(6., -1., 2.), 4., 8., 2., 2, 6)
    }

    #[test]
    fn successful_compound_preserves_piece_order_materials_and_geometry() {
        let mut plan = EnvironmentPlan::new(15).unwrap();
        plan.stairs(Vec3::ZERO, 2., 0.25, 0.5, 2, 3).unwrap();
        let mut direct = plan.clone();
        compound(&mut direct).unwrap();
        let storage = plan.pieces().as_ptr();
        plan.append_atomic(compound).unwrap();
        assert_eq!(plan.pieces(), direct.pieces());
        assert_eq!(plan.bounds(), direct.bounds());
        assert_eq!((plan.piece_count(), plan.remaining_capacity()), (15, 0));
        assert_eq!(plan.pieces().as_ptr(), storage);
    }

    #[test]
    fn compound_cannot_bypass_destination_capacity_by_replacing_staging_plan() {
        let mut plan = EnvironmentPlan::new(2).unwrap();
        plan.ramp(Vec3::ZERO, 2., 1., 3., 0.2, 0).unwrap();
        let before = plan.pieces().to_vec();
        assert_eq!(
            plan.append_atomic(|group| {
                *group = EnvironmentPlan::new(1024)?;
                group.stairs(Vec3::ZERO, 2., 0.25, 0.5, 2, 1)
            }),
            Err(BuildError::Capacity)
        );
        assert_eq!(plan.pieces(), before);
        assert_eq!((plan.capacity(), plan.remaining_capacity()), (2, 1));
    }

    #[test]
    fn callback_error_and_unwind_do_not_change_the_destination() {
        let mut plan = EnvironmentPlan::new(8).unwrap();
        plan.ramp(Vec3::ZERO, 2., 1., 3., 0.2, 0).unwrap();
        let before = plan.pieces().to_vec();
        assert_eq!(
            plan.append_atomic(|group| {
                group.stairs(Vec3::ZERO, 2., 0.25, 0.5, 2, 1)?;
                Err(BuildError::Invalid)
            }),
            Err(BuildError::Invalid)
        );
        assert_eq!(plan.pieces(), before);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = plan.append_atomic(|group| {
                group.stairs(Vec3::ZERO, 2., 0.25, 0.5, 2, 1)?;
                panic!("authoring callback failed");
            });
        }));
        assert!(result.is_err());
        assert_eq!(plan.pieces(), before);
        assert_eq!((plan.capacity(), plan.remaining_capacity()), (8, 7));
    }

    #[test]
    fn empty_compounds_are_noops_even_on_full_plans() {
        let mut plan = EnvironmentPlan::new(1).unwrap();
        assert_eq!(plan.bounds(), None);
        plan.append_atomic(|_| Ok(())).unwrap();
        assert_eq!(plan.piece_count(), 0);
        plan.ramp(Vec3::ZERO, 2., 1., 3., 0.2, 0).unwrap();
        let before = plan.pieces().to_vec();
        plan.append_atomic(|group| {
            assert_eq!(group.capacity(), 0);
            assert_eq!(group.remaining_capacity(), 0);
            assert_eq!(group.bounds(), None);
            group.append_atomic(|_| Ok(()))
        })
        .unwrap();
        assert_eq!(plan.pieces(), before);
        assert_eq!(plan.remaining_capacity(), 0);
    }

    #[test]
    fn aggregate_bounds_enclose_rotated_corners_and_all_bridge_solids() {
        let mut plan = EnvironmentPlan::new(13).unwrap();
        compound(&mut plan).unwrap();
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for piece in plan.pieces() {
            for x in [-1., 1.] {
                for y in [-1., 1.] {
                    for z in [-1., 1.] {
                        let corner = piece.center
                            + piece.rotation * (piece.half_extents * Vec3::new(x, y, z));
                        min = min.min(corner);
                        max = max.max(corner);
                    }
                }
            }
        }
        let (bounds_min, bounds_max) = plan.bounds().unwrap();
        assert!((bounds_min - min).abs().max_element() < 1e-5);
        assert!((bounds_max - max).abs().max_element() < 1e-5);

        let mut bridge = EnvironmentPlan::new(9).unwrap();
        let origin = Vec3::new(-3., 4., 5.);
        bridge.bridge(origin, 4., 6., 2., 2, 0).unwrap();
        let (min, max) = bridge.bounds().unwrap();
        assert!((min - (origin + Vec3::new(-2.07, 0., -3.07))).length() < 1e-5);
        assert!((max - (origin + Vec3::new(2.07, 3., 3.07))).length() < 1e-5);
    }
}
