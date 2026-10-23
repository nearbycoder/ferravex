use super::{
    MAX_EFFECT_LIFETIME, RibbonError, RibbonFacing, RibbonPoint, RibbonTriangle, RibbonTriangles,
};
pub const MAX_BEAMS: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeamDesc {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    pub width: f32,
    pub lifetime: f32,
}
impl Default for BeamDesc {
    fn default() -> Self {
        Self {
            start: [0.; 3],
            end: [0., 0., 1.],
            start_color: [1.; 4],
            end_color: [1.; 4],
            width: 0.02,
            lifetime: 0.1,
        }
    }
}
impl BeamDesc {
    /// Checks finite endpoints/colors, width in `0..=1e6`, and lifetime in
    /// `0..=MAX_EFFECT_LIFETIME` excluding zero. Coincident endpoints are valid.
    /// Colors may contain finite HDR values; they are not clamped.
    ///
    /// ```
    /// use ferravex_play::particles::{BeamDesc, BeamError};
    /// let mut desc = BeamDesc::default();
    /// assert_eq!(desc.validate(), Ok(()));
    /// desc.lifetime = 0.0;
    /// assert_eq!(desc.validate(), Err(BeamError::InvalidBeam));
    /// ```
    pub fn validate(&self) -> Result<(), BeamError> {
        if !self
            .start
            .into_iter()
            .chain(self.end)
            .chain(self.start_color)
            .chain(self.end_color)
            .chain([self.width, self.lifetime])
            .all(f32::is_finite)
            || !(0.0..=1e6).contains(&self.width)
            || !(0.0..=MAX_EFFECT_LIFETIME).contains(&self.lifetime)
            || self.lifetime == 0.
        {
            Err(BeamError::InvalidBeam)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeamError {
    InvalidBeam,
    InvalidCapacity,
    InvalidDelta,
    Full,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowPolicy {
    DropNewest,
    ReplaceOldest,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeamView {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    pub width: f32,
    pub normalized_age: f32,
}
impl BeamView {
    pub fn points(self) -> [RibbonPoint; 2] {
        [
            RibbonPoint {
                position: self.start,
                color: self.start_color,
                width: self.width,
                break_before: true,
            },
            RibbonPoint {
                position: self.end,
                color: self.end_color,
                width: self.width,
                break_before: false,
            },
        ]
    }

    /// Generates at most two triangles using the shared ribbon tessellator,
    /// without allocation or borrowing the view/pool. Invalid geometry fields
    /// return [`RibbonError::InvalidPoint`]; invalid facing returns
    /// [`RibbonError::InvalidView`]. `normalized_age` is metadata, not geometry.
    ///
    /// Coincident endpoints emit no triangles. Camera-parallel segments use
    /// projected camera up, then a deterministic perpendicular when up is also
    /// parallel. Non-finite output vertices are skipped by the tessellator.
    /// Colors, distance UVs and winding match [`RibbonTriangles`] over
    /// [`Self::points`]. No random state or elapsed time is consumed.
    ///
    /// ```
    /// use ferravex_play::particles::{BeamDesc, BeamPool, OverflowPolicy, RibbonFacing};
    /// let mut pool = BeamPool::new(1, OverflowPolicy::DropNewest);
    /// pool.spawn(BeamDesc::default()).unwrap();
    /// let beam = pool.iter().next().unwrap();
    /// let triangles = beam.triangles(RibbonFacing::Camera {
    ///     eye: [0.0, 0.0, 5.0], up: [0.0, 0.0, 1.0],
    /// }).unwrap();
    /// assert_eq!(triangles.count(), 2);
    /// ```
    pub fn triangles(
        self,
        facing: RibbonFacing,
    ) -> Result<impl Iterator<Item = RibbonTriangle>, RibbonError> {
        let points = self.points();
        if !points.iter().all(RibbonPoint::valid) {
            return Err(RibbonError::InvalidPoint);
        }
        let mut triangles = RibbonTriangles::new(&points, facing, 1.)?;
        Ok([triangles.next(), triangles.next()].into_iter().flatten())
    }
}
struct Beam {
    desc: BeamDesc,
    age: f64,
    serial: u64,
}
pub struct BeamPool {
    beams: Vec<Beam>,
    limit: usize,
    policy: OverflowPolicy,
    serial: u64,
    dropped: u64,
    replaced: u64,
}
impl BeamPool {
    pub fn new(capacity: usize, policy: OverflowPolicy) -> Self {
        Self::try_new(capacity, policy).expect("beam capacity <= 4096")
    }
    pub fn try_new(capacity: usize, policy: OverflowPolicy) -> Result<Self, BeamError> {
        if capacity > MAX_BEAMS {
            return Err(BeamError::InvalidCapacity);
        }
        Ok(Self {
            beams: Vec::with_capacity(capacity),
            limit: capacity,
            policy,
            serial: 0,
            dropped: 0,
            replaced: 0,
        })
    }
    pub fn len(&self) -> usize {
        self.beams.len()
    }
    pub fn is_empty(&self) -> bool {
        self.beams.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.limit
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn replaced(&self) -> u64 {
        self.replaced
    }
    pub fn clear(&mut self) {
        self.beams.clear();
    }

    /// Clears beams, overflow counters and admission ordering while retaining
    /// the allocated storage, capacity and overflow policy. [`Self::clear`]
    /// preserves the counters instead.
    ///
    /// ```
    /// use ferravex_play::particles::{BeamDesc, BeamError, BeamPool, OverflowPolicy};
    /// let mut pool = BeamPool::new(1, OverflowPolicy::DropNewest);
    /// pool.spawn(BeamDesc::default()).unwrap();
    /// assert_eq!(pool.spawn(BeamDesc::default()), Err(BeamError::Full));
    /// pool.reset();
    /// assert!(pool.is_empty());
    /// assert_eq!((pool.capacity(), pool.dropped(), pool.replaced()), (1, 0, 0));
    /// pool.spawn(BeamDesc::default()).unwrap();
    /// ```
    pub fn reset(&mut self) {
        self.clear();
        self.serial = 0;
        self.dropped = 0;
        self.replaced = 0;
    }

    /// Moves the beam at the current [`Self::iter`] index without changing its
    /// age, lifetime, colors, width or replacement priority. Both endpoints must
    /// be finite; coincident endpoints are allowed. Invalid input leaves the
    /// entire pool unchanged. Valid input with no beam at `index` returns `false`.
    ///
    /// Indices are temporary positions, not identities: aging can compact the
    /// pool and spawning can replace a beam. Reacquire indices after those calls.
    ///
    /// ```
    /// use ferravex_play::particles::{BeamDesc, BeamPool, OverflowPolicy};
    /// let mut pool = BeamPool::new(1, OverflowPolicy::DropNewest);
    /// pool.spawn(BeamDesc { lifetime: 1.0, ..BeamDesc::default() }).unwrap();
    /// pool.update(0.25).unwrap();
    /// assert!(pool.set_endpoints(0, [1.0, 0.0, 0.0], [3.0, 0.0, 0.0]).unwrap());
    /// assert_eq!(pool.iter().next().unwrap().normalized_age, 0.25);
    /// assert!(!pool.set_endpoints(1, [0.0; 3], [0.0; 3]).unwrap());
    /// ```
    pub fn set_endpoints(
        &mut self,
        index: usize,
        start: [f32; 3],
        end: [f32; 3],
    ) -> Result<bool, BeamError> {
        if !start.into_iter().chain(end).all(f32::is_finite) {
            return Err(BeamError::InvalidBeam);
        }
        let Some(beam) = self.beams.get_mut(index) else {
            return Ok(false);
        };
        beam.desc.start = start;
        beam.desc.end = end;
        Ok(true)
    }

    /// Reconfigures a beam at the current [`Self::iter`] index and restarts its
    /// lifetime/fade at age zero. It becomes newest for equal-age replacement
    /// ties. This does not spawn, change length or increment overflow counters.
    ///
    /// Validation precedes lookup: invalid descriptors return
    /// [`BeamError::InvalidBeam`] without mutation, and valid descriptors with
    /// no beam at `index` return `false`. Indices have the same temporary meaning
    /// as in [`Self::set_endpoints`].
    ///
    /// ```
    /// use ferravex_play::particles::{BeamDesc, BeamPool, OverflowPolicy};
    /// let mut pool = BeamPool::new(1, OverflowPolicy::ReplaceOldest);
    /// pool.spawn(BeamDesc { lifetime: 1.0, ..BeamDesc::default() }).unwrap();
    /// pool.update(0.5).unwrap();
    /// assert!(pool.reset_beam(0, BeamDesc::default()).unwrap());
    /// assert_eq!(pool.iter().next().unwrap().normalized_age, 0.0);
    /// assert_eq!(pool.replaced(), 0);
    /// ```
    pub fn reset_beam(&mut self, index: usize, desc: BeamDesc) -> Result<bool, BeamError> {
        desc.validate()?;
        let Some(beam) = self.beams.get_mut(index) else {
            return Ok(false);
        };
        *beam = Beam {
            desc,
            age: 0.,
            serial: self.serial,
        };
        self.serial = self.serial.saturating_add(1);
        Ok(true)
    }

    pub fn spawn(&mut self, desc: BeamDesc) -> Result<(), BeamError> {
        desc.validate()?;
        if self.limit == 0
            || self.beams.len() == self.limit && self.policy == OverflowPolicy::DropNewest
        {
            self.dropped = self.dropped.saturating_add(1);
            return Err(BeamError::Full);
        }
        let beam = Beam {
            desc,
            age: 0.,
            serial: self.serial,
        };
        self.serial = self.serial.saturating_add(1);
        if self.beams.len() == self.limit {
            let oldest = self
                .beams
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| {
                    a.age
                        .total_cmp(&b.age)
                        .then_with(|| b.serial.cmp(&a.serial))
                })
                .map(|(i, _)| i)
                .expect("nonempty full beam pool");
            self.beams[oldest] = beam;
            self.replaced = self.replaced.saturating_add(1);
        } else {
            self.beams.push(beam);
        }
        Ok(())
    }
    pub fn update(&mut self, dt: f32) -> Result<(), BeamError> {
        if !dt.is_finite() || dt < 0. {
            return Err(BeamError::InvalidDelta);
        }
        self.beams.retain_mut(|b| {
            b.age += f64::from(dt);
            b.age < f64::from(b.desc.lifetime)
        });
        Ok(())
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = BeamView> + '_ {
        self.beams.iter().map(|b| {
            let t = (b.age / f64::from(b.desc.lifetime)) as f32;
            let fade = |mut color: [f32; 4]| {
                color[3] *= 1. - t;
                color
            };
            BeamView {
                start: b.desc.start,
                end: b.desc.end,
                start_color: fade(b.desc.start_color),
                end_color: fade(b.desc.end_color),
                width: b.desc.width,
                normalized_age: t,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc(x: f32) -> BeamDesc {
        BeamDesc {
            start: [x, 0., 0.],
            end: [x, 0., 2.],
            start_color: [2., 0., 0., 0.8],
            end_color: [0., 3., 0., 0.4],
            width: 0.5,
            lifetime: 2.,
        }
    }

    fn view(desc: BeamDesc) -> BeamView {
        BeamView {
            start: desc.start,
            end: desc.end,
            start_color: desc.start_color,
            end_color: desc.end_color,
            width: desc.width,
            normalized_age: 0.,
        }
    }

    fn state(pool: &BeamPool) -> (Vec<(BeamDesc, f64, u64)>, u64, u64, u64) {
        (
            pool.beams
                .iter()
                .map(|b| (b.desc, b.age, b.serial))
                .collect(),
            pool.serial,
            pool.dropped(),
            pool.replaced(),
        )
    }

    #[test]
    fn descriptor_validation_preserves_spawn_bounds_and_rejection_is_transactional() {
        let valid = BeamDesc {
            start: [f32::MAX; 3],
            end: [f32::MAX; 3],
            start_color: [-f32::MAX; 4],
            end_color: [f32::MAX; 4],
            width: 1e6,
            lifetime: MAX_EFFECT_LIFETIME,
        };
        assert_eq!(valid.validate(), Ok(()));
        assert_eq!(BeamDesc { width: 0., ..valid }.validate(), Ok(()));
        let mut pool = BeamPool::new(1, OverflowPolicy::ReplaceOldest);
        pool.spawn(desc(0.)).unwrap();
        pool.update(0.5).unwrap();
        let before = state(&pool);
        let mut invalid = vec![
            BeamDesc {
                width: -1.,
                ..valid
            },
            BeamDesc {
                width: 1e6 + 1.,
                ..valid
            },
            BeamDesc {
                lifetime: 0.,
                ..valid
            },
            BeamDesc {
                lifetime: -1.,
                ..valid
            },
            BeamDesc {
                lifetime: MAX_EFFECT_LIFETIME + 1.,
                ..valid
            },
        ];
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            invalid.extend([
                BeamDesc {
                    start: [value; 3],
                    ..valid
                },
                BeamDesc {
                    end: [value; 3],
                    ..valid
                },
                BeamDesc {
                    start_color: [value; 4],
                    ..valid
                },
                BeamDesc {
                    end_color: [value; 4],
                    ..valid
                },
                BeamDesc {
                    width: value,
                    ..valid
                },
                BeamDesc {
                    lifetime: value,
                    ..valid
                },
            ]);
        }
        for bad in invalid {
            assert_eq!(bad.validate(), Err(BeamError::InvalidBeam));
            assert_eq!(pool.spawn(bad), Err(BeamError::InvalidBeam));
            assert_eq!(pool.reset_beam(0, bad), Err(BeamError::InvalidBeam));
            assert_eq!(
                pool.reset_beam(usize::MAX, bad),
                Err(BeamError::InvalidBeam)
            );
            assert_eq!(state(&pool), before);
        }
    }

    #[test]
    fn moving_endpoints_preserves_fade_lifetime_and_replacement_priority() {
        let mut pool = BeamPool::new(2, OverflowPolicy::ReplaceOldest);
        pool.spawn(desc(0.)).unwrap();
        pool.spawn(desc(1.)).unwrap();
        pool.update(0.5).unwrap();
        let before = pool.iter().next().unwrap();
        assert!(pool.set_endpoints(0, [3.; 3], [4.; 3]).unwrap());
        let after = pool.iter().next().unwrap();
        assert_eq!(
            after,
            BeamView {
                start: [3.; 3],
                end: [4.; 3],
                ..before
            }
        );
        pool.spawn(desc(2.)).unwrap();
        assert_eq!(
            pool.iter().map(|b| b.start[0]).collect::<Vec<_>>(),
            [2., 1.]
        );
        assert_eq!(pool.replaced(), 1);
        assert!(pool.set_endpoints(1, [9.; 3], [9.; 3]).unwrap());
        pool.update(1.5).unwrap();
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.iter().next().unwrap().start, desc(2.).start);
    }

    #[test]
    fn invalid_endpoints_deltas_and_missing_indices_leave_all_state_unchanged() {
        let mut pool = BeamPool::new(1, OverflowPolicy::DropNewest);
        pool.spawn(desc(0.)).unwrap();
        assert_eq!(pool.spawn(desc(1.)), Err(BeamError::Full));
        pool.update(0.5).unwrap();
        let before = state(&pool);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for (start, end) in [([bad; 3], [1.; 3]), ([1.; 3], [bad; 3])] {
                assert_eq!(
                    pool.set_endpoints(0, start, end),
                    Err(BeamError::InvalidBeam)
                );
                assert_eq!(
                    pool.set_endpoints(usize::MAX, start, end),
                    Err(BeamError::InvalidBeam)
                );
            }
            assert_eq!(pool.update(bad), Err(BeamError::InvalidDelta));
            assert_eq!(state(&pool), before);
        }
        assert_eq!(pool.update(-0.5), Err(BeamError::InvalidDelta));
        assert_eq!(pool.set_endpoints(1, [0.; 3], [0.; 3]), Ok(false));
        assert_eq!(pool.reset_beam(usize::MAX, desc(1.)), Ok(false));
        pool.update(0.).unwrap();
        assert_eq!(state(&pool), before);
        pool.update(1.5).unwrap();
        assert_eq!(pool.set_endpoints(0, [0.; 3], [0.; 3]), Ok(false));
        assert_eq!(pool.reset_beam(0, desc(2.)), Ok(false));
        assert!(pool.is_empty());
    }

    #[test]
    fn reset_beam_restarts_style_and_lifetime_without_counting_an_overflow() {
        let mut pool = BeamPool::new(1, OverflowPolicy::DropNewest);
        pool.spawn(desc(0.)).unwrap();
        pool.update(1.).unwrap();
        let restarted = BeamDesc {
            lifetime: 0.5,
            width: 2.,
            ..desc(9.)
        };
        assert_eq!(pool.reset_beam(0, restarted), Ok(true));
        assert_eq!(pool.iter().next().unwrap(), view(restarted));
        assert_eq!((pool.len(), pool.dropped(), pool.replaced()), (1, 0, 0));
        pool.update(0.25).unwrap();
        let faded = pool.iter().next().unwrap();
        assert_eq!(faded.normalized_age, 0.5);
        assert_eq!(faded.start_color[3], restarted.start_color[3] * 0.5);
        assert_eq!(faded.end_color[3], restarted.end_color[3] * 0.5);
        pool.update(0.25).unwrap();
        assert!(pool.is_empty());
    }

    #[test]
    fn restarted_beam_is_newest_for_equal_age_replacement() {
        let mut pool = BeamPool::new(2, OverflowPolicy::ReplaceOldest);
        pool.spawn(desc(0.)).unwrap();
        pool.spawn(desc(1.)).unwrap();
        pool.reset_beam(0, desc(2.)).unwrap();
        pool.spawn(desc(3.)).unwrap();
        assert_eq!(
            pool.iter().map(|b| b.start[0]).collect::<Vec<_>>(),
            [2., 3.]
        );
        assert_eq!(pool.replaced(), 1);
    }

    #[test]
    fn reset_matches_fresh_pool_and_retains_storage_and_overflow_policy() {
        for policy in [OverflowPolicy::DropNewest, OverflowPolicy::ReplaceOldest] {
            let mut pool = BeamPool::new(2, policy);
            let storage = pool.beams.as_ptr();
            let allocated = pool.beams.capacity();
            for _ in 0..32 {
                pool.spawn(desc(0.)).unwrap();
                pool.spawn(desc(1.)).unwrap();
                let result = pool.spawn(desc(2.));
                if policy == OverflowPolicy::DropNewest {
                    assert_eq!(result, Err(BeamError::Full));
                    assert_eq!(pool.dropped(), 1);
                } else {
                    result.unwrap();
                    assert_eq!(pool.replaced(), 1);
                }
                let counters = (pool.dropped(), pool.replaced());
                pool.clear();
                assert_eq!((pool.dropped(), pool.replaced()), counters);
                pool.reset();
                let fresh = BeamPool::new(2, policy);
                assert_eq!(state(&pool), state(&fresh));
                assert_eq!(pool.capacity(), fresh.capacity());
                assert_eq!(pool.beams.as_ptr(), storage);
                assert_eq!(pool.beams.capacity(), allocated);
            }
        }
        let mut empty = BeamPool::new(0, OverflowPolicy::ReplaceOldest);
        assert_eq!(empty.spawn(desc(0.)), Err(BeamError::Full));
        empty.reset();
        assert_eq!(empty.dropped(), 0);
        assert_eq!(empty.reset_beam(0, desc(0.)), Ok(false));
        assert_eq!(empty.spawn(desc(0.)), Err(BeamError::Full));
    }

    #[test]
    fn geometry_matches_existing_ribbon_colors_uvs_winding_and_bounds() {
        let beam = view(desc(0.));
        for facing in [
            RibbonFacing::Plane {
                normal: [0., 1., 0.],
            },
            RibbonFacing::Camera {
                eye: [1., 2., 3.],
                up: [0., 1., 0.],
            },
        ] {
            let points = beam.points();
            let expected: Vec<_> = RibbonTriangles::new(&points, facing, 1.).unwrap().collect();
            let actual: Vec<_> = beam.triangles(facing).unwrap().collect();
            assert_eq!(actual, expected);
            assert_eq!(actual.len(), 2);
            for v in actual.iter().flat_map(|t| t.vertices) {
                let (center, color, distance) = if v.uv[0] == 0. {
                    (beam.start, beam.start_color, 0.)
                } else {
                    (beam.end, beam.end_color, 2.)
                };
                assert_eq!(v.color, color);
                assert_eq!(v.uv[0], distance);
                let offset = ferravex_math::Vec3::from_array(v.position)
                    - ferravex_math::Vec3::from_array(center);
                assert!((offset.length() - beam.width * 0.5).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn coincident_endpoints_can_move_apart_and_geometry_never_mutates_age() {
        let mut pool = BeamPool::new(1, OverflowPolicy::DropNewest);
        pool.spawn(desc(0.)).unwrap();
        pool.update(0.5).unwrap();
        let facing = RibbonFacing::Camera {
            eye: [0.; 3],
            up: [0., 0., 1.],
        };
        pool.set_endpoints(0, [0.; 3], [0.; 3]).unwrap();
        let before = state(&pool);
        assert_eq!(
            pool.iter()
                .next()
                .unwrap()
                .triangles(facing)
                .unwrap()
                .count(),
            0
        );
        assert_eq!(state(&pool), before);
        pool.set_endpoints(0, desc(0.).start, desc(0.).end).unwrap();
        let beam = pool.iter().next().unwrap();
        assert_eq!(beam.triangles(facing).unwrap().count(), 2);
        assert_eq!(beam.normalized_age, 0.25);
    }

    #[test]
    fn camera_parallel_and_coincident_eye_geometry_is_finite_and_repeatable() {
        for direction in [
            [1., 0., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [1., 1., 1.],
            [1., 2., -3.],
        ] {
            let beam = view(BeamDesc {
                start: [0.; 3],
                end: direction,
                ..desc(0.)
            });
            for distance in [-2., 0., 0.5, 1., 2.] {
                for up in [direction, direction.map(|n| -n), [0., 1., 0.]] {
                    let facing = RibbonFacing::Camera {
                        eye: direction.map(|n| n * distance),
                        up,
                    };
                    let first: Vec<_> = beam.triangles(facing).unwrap().collect();
                    let second: Vec<_> = beam.triangles(facing).unwrap().collect();
                    assert_eq!(first, second);
                    assert_eq!(first.len(), 2);
                    for triangle in first {
                        for v in triangle.vertices {
                            assert!(
                                v.position
                                    .into_iter()
                                    .chain(v.color)
                                    .chain(v.uv)
                                    .all(f32::is_finite)
                            );
                            let center = if v.uv[0] == 0. { beam.start } else { beam.end };
                            let offset = ferravex_math::Vec3::from_array(v.position)
                                - ferravex_math::Vec3::from_array(center);
                            assert!((offset.length() - beam.width * 0.5).abs() < 1e-6);
                            assert!(
                                offset.dot(ferravex_math::Vec3::from_array(direction)).abs() < 1e-6
                            );
                        }
                        let positions = triangle
                            .vertices
                            .map(|v| ferravex_math::Vec3::from_array(v.position));
                        assert!(
                            (positions[1] - positions[0])
                                .cross(positions[2] - positions[0])
                                .length()
                                > 0.
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn geometry_rejects_invalid_fields_and_extreme_segments_stay_finite_and_bounded() {
        let beam = view(desc(0.));
        let facing = RibbonFacing::Plane {
            normal: [0., 1., 0.],
        };
        for invalid in [
            BeamView {
                start: [f32::NAN; 3],
                ..beam
            },
            BeamView {
                end: [f32::INFINITY; 3],
                ..beam
            },
            BeamView {
                start_color: [f32::NAN; 4],
                ..beam
            },
            BeamView {
                end_color: [f32::INFINITY; 4],
                ..beam
            },
            BeamView { width: -1., ..beam },
            BeamView {
                width: f32::NAN,
                ..beam
            },
            BeamView {
                width: 1e6 + 1.,
                ..beam
            },
        ] {
            assert_eq!(
                invalid.triangles(facing).err(),
                Some(RibbonError::InvalidPoint)
            );
        }
        for invalid in [
            RibbonFacing::Plane { normal: [0.; 3] },
            RibbonFacing::Camera {
                eye: [f32::NAN; 3],
                up: [0., 1., 0.],
            },
            RibbonFacing::Camera {
                eye: [0.; 3],
                up: [0.; 3],
            },
            RibbonFacing::Camera {
                eye: [0.; 3],
                up: [f32::INFINITY; 3],
            },
        ] {
            assert_eq!(
                beam.triangles(invalid).err(),
                Some(RibbonError::InvalidView)
            );
        }
        let huge = BeamView {
            start: [-f32::MAX; 3],
            end: [f32::MAX; 3],
            ..beam
        };
        let triangles: Vec<_> = huge.triangles(facing).unwrap().collect();
        assert!(triangles.len() <= 2);
        for vertex in triangles.iter().flat_map(|t| t.vertices) {
            assert!(
                vertex
                    .position
                    .into_iter()
                    .chain(vertex.uv)
                    .all(f32::is_finite)
            );
        }
    }
}
