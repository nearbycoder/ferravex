//! Analytic limb IK and presentation-only foot contacts. Hosts supply actual
//! support queries and apply rotations after their authored animation graph.
use super::AnimationError;
use ferravex_math::Vec3;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimbPose {
    pub root: Vec3,
    pub joint: Vec3,
    pub end: Vec3,
    pub clamped: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct TwoBone {
    pub upper: f32,
    pub lower: f32,
}
impl TwoBone {
    pub fn solve(self, root: Vec3, target: Vec3, pole: Vec3) -> Result<LimbPose, AnimationError> {
        if ![self.upper, self.lower]
            .iter()
            .all(|v| v.is_finite() && (0.001..=1000.).contains(v))
            || ![root, target, pole]
                .iter()
                .all(|v| v.is_finite() && v.abs().max_element() <= 1e6)
        {
            return Err(AnimationError::InvalidPose);
        }
        let delta = target - root;
        let length = delta.length();
        let axis = delta.try_normalize().unwrap_or(-Vec3::Y);
        let epsilon = self.upper.min(self.lower) * 0.0001;
        let reach = length.clamp(
            (self.upper - self.lower).abs() + epsilon,
            self.upper + self.lower - epsilon,
        );
        let along =
            (self.upper * self.upper - self.lower * self.lower + reach * reach) / (2. * reach);
        let side = (pole - root - axis * (pole - root).dot(axis))
            .try_normalize()
            .unwrap_or_else(|| axis.any_orthonormal_vector());
        Ok(LimbPose {
            root,
            joint: root
                + axis * along
                + side * (self.upper * self.upper - along * along).max(0.).sqrt(),
            end: root + axis * reach,
            clamped: (reach - length).abs() > epsilon,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FootContact {
    pub position: Vec3,
    pub normal: Vec3,
    pub support: u64,
    pub support_delta: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootPose {
    pub position: Vec3,
    pub normal: Vec3,
    pub weight: f32,
}
/// Contact acquisition and retention rules for [`FootPlant::with_thresholds`].
///
/// A new support must be within `enter_distance` and have `normal.y` strictly
/// above `enter_normal_y`. The current support may use the full maximum
/// correction distance and the lower `exit_normal_y` threshold. Both distance
/// limits are inclusive. A support change always uses the acquisition rules.
///
/// ```
/// use ferravex_play::animation::{FootContact, FootPlant, FootPlantThresholds};
/// use ferravex_math::Vec3;
///
/// let thresholds = FootPlantThresholds {
///     enter_distance: 0.3,
///     enter_normal_y: 0.6,
///     exit_normal_y: 0.4,
///     release_delay: 0.05,
/// };
/// let mut foot = FootPlant::with_thresholds(0.5, 0.05, thresholds)?;
/// assert_eq!(foot.thresholds(), thresholds);
/// let contact = FootContact {
///     position: Vec3::Y * 0.2,
///     normal: Vec3::Y,
///     support: 7,
///     support_delta: Vec3::ZERO,
/// };
/// let pose = foot.update(Vec3::ZERO, Some(contact), true, 0.05)?;
/// assert_eq!(foot.support(), Some(7));
/// assert_eq!(foot.pose(), pose);
/// assert!((foot.correction().y - 0.1).abs() < 1e-6);
/// // One short query dropout holds the anchor and its correction.
/// assert_eq!(foot.update(Vec3::ZERO, None, true, 0.01)?, pose);
/// # Ok::<(), ferravex_play::animation::AnimationError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootPlantThresholds {
    /// Finite acquisition distance in `0..=max_correction`.
    pub enter_distance: f32,
    /// Finite, strict minimum upward normal component in `0..1`.
    pub enter_normal_y: f32,
    /// Finite retention threshold in `0..=enter_normal_y`.
    pub exit_normal_y: f32,
    /// Seconds of marginal/missing contact tolerated in `0..=0.25`.
    /// At this duration the anchor releases; zero disables the grace period.
    pub release_delay: f32,
}

/// Allocation-free presentation contact state. The caller supplies support
/// queries and elapsed presentation time; this never changes body motion.
#[derive(Clone, Copy, Debug)]
pub struct FootPlant {
    support: Option<u64>,
    pose: FootPose,
    max_correction: f32,
    half_life: f32,
    thresholds: FootPlantThresholds,
    missing_seconds: f64,
    correction: Vec3,
}
impl FootPlant {
    /// Uses the original single cutoff: maximum correction distance, upward
    /// normal component strictly above `0.5`, and no contact-loss grace period.
    /// Use [`Self::with_thresholds`] to opt into hysteresis.
    pub fn new(max_correction: f32, half_life: f32) -> Result<Self, AnimationError> {
        Self::with_thresholds(
            max_correction,
            half_life,
            FootPlantThresholds {
                enter_distance: max_correction,
                enter_normal_y: 0.5,
                exit_normal_y: 0.5,
                release_delay: 0.,
            },
        )
    }

    /// Creates a plant with validated hysteresis. Maximum correction must be
    /// in `0.001..=10` world units and half-life in `0.001..=1` seconds. Invalid
    /// or non-finite configuration returns [`AnimationError::InvalidPose`].
    /// See [`FootPlantThresholds`] for a compiling usage example and field limits.
    pub fn with_thresholds(
        max_correction: f32,
        half_life: f32,
        thresholds: FootPlantThresholds,
    ) -> Result<Self, AnimationError> {
        if !max_correction.is_finite()
            || !(0.001..=10.).contains(&max_correction)
            || !half_life.is_finite()
            || !(0.001..=1.).contains(&half_life)
            || !thresholds.enter_distance.is_finite()
            || !(0. ..=max_correction).contains(&thresholds.enter_distance)
            || !thresholds.enter_normal_y.is_finite()
            || !(0. ..1.).contains(&thresholds.enter_normal_y)
            || !thresholds.exit_normal_y.is_finite()
            || !(0. ..=thresholds.enter_normal_y).contains(&thresholds.exit_normal_y)
            || !thresholds.release_delay.is_finite()
            || !(0. ..=0.25).contains(&thresholds.release_delay)
        {
            return Err(AnimationError::InvalidPose);
        }
        Ok(Self {
            support: None,
            pose: FootPose {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                weight: 0.,
            },
            max_correction,
            half_life,
            thresholds,
            missing_seconds: 0.,
            correction: Vec3::ZERO,
        })
    }

    /// Discards the anchor, normal, weight, grace timer and correction. The
    /// unweighted target returns to the origin; use [`Self::reset`] on teleport.
    pub fn clear(&mut self) {
        self.support = None;
        self.pose = FootPose {
            position: Vec3::ZERO,
            normal: Vec3::Y,
            weight: 0.,
        };
        self.missing_seconds = 0.;
        self.correction = Vec3::ZERO;
    }

    /// Clears all contact history at a new animated position. A non-finite
    /// position returns [`AnimationError::InvalidPose`] without changing state.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::animation::FootPlant;
    /// let mut foot = FootPlant::new(0.5, 0.05)?;
    /// let pose = foot.reset(Vec3::X * 10.)?;
    /// assert_eq!(pose.position, Vec3::X * 10.);
    /// assert_eq!(pose.weight, 0.);
    /// assert_eq!(foot.support(), None);
    /// assert_eq!(foot.correction(), Vec3::ZERO);
    /// foot.clear();
    /// assert_eq!(foot.pose().position, Vec3::ZERO);
    /// # Ok::<(), ferravex_play::animation::AnimationError>(())
    /// ```
    pub fn reset(&mut self, animated: Vec3) -> Result<FootPose, AnimationError> {
        if !animated.is_finite() {
            return Err(AnimationError::InvalidPose);
        }
        self.clear();
        self.pose.position = animated;
        Ok(self.pose)
    }

    /// Returns the last unweighted target, normal and blend weight.
    pub fn pose(&self) -> FootPose {
        self.pose
    }

    /// Returns the accepted support, including while held during a grace period.
    pub fn support(&self) -> Option<u64> {
        self.support
    }

    /// Returns the validated acquisition/retention configuration.
    pub fn thresholds(&self) -> FootPlantThresholds {
        self.thresholds
    }

    /// Weighted translation from the last positive-time update's animated
    /// position: `(pose.position - animated) * pose.weight`. Add this to that
    /// animated position to apply the contact blend. Zero after clear/reset.
    /// It is bounded by `max_correction`, subject to floating-point rounding.
    pub fn correction(&self) -> Vec3 {
        self.correction
    }

    /// Updates presentation state with finite positions, a unit contact normal
    /// (squared-length tolerance `0.001`), and `dt` in `0..=0.25` seconds.
    /// Invalid input leaves all state unchanged; zero time validates input but
    /// holds all state, so a support delta is not consumed while paused.
    ///
    /// Once acquired, an anchor follows the same support's delta exactly once
    /// per positive-time update, independently of blend weight. Query position
    /// jitter does not move that anchor. If support motion takes the anchor out
    /// of range, a usable contact reanchors it within the correction limit.
    ///
    /// Missing/marginal contact holds a bounded anchor for less than
    /// `release_delay` seconds; absent queries imply no known support motion.
    /// A valid contact resets that timer. `plant = false`, a different support,
    /// or an out-of-range anchor bypasses the grace period. Release restores the
    /// animated position/up normal immediately while the blend weight decays.
    pub fn update(
        &mut self,
        animated: Vec3,
        contact: Option<FootContact>,
        plant: bool,
        dt: f32,
    ) -> Result<FootPose, AnimationError> {
        if !animated.is_finite()
            || !dt.is_finite()
            || !(0. ..=0.25).contains(&dt)
            || contact.is_some_and(|c| {
                !c.position.is_finite()
                    || !c.normal.is_finite()
                    || (c.normal.length_squared() - 1.).abs() > 0.001
                    || !c.support_delta.is_finite()
            })
        {
            return Err(AnimationError::InvalidPose);
        }
        if dt == 0. {
            return Ok(self.pose);
        }
        let alpha = -(-std::f32::consts::LN_2 * dt / self.half_life).exp_m1();
        let same_support = contact.is_some_and(|c| self.support == Some(c.support));
        let anchor = if same_support {
            self.pose.position + contact.expect("same support has a contact").support_delta
        } else {
            self.pose.position
        };
        let anchor_in_range =
            anchor.is_finite() && anchor.distance(animated) <= self.max_correction;
        let usable = contact.filter(|c| {
            plant
                && c.normal.y
                    > if same_support {
                        self.thresholds.exit_normal_y
                    } else {
                        self.thresholds.enter_normal_y
                    }
                && c.position.distance(animated)
                    <= if same_support {
                        self.max_correction
                    } else {
                        self.thresholds.enter_distance
                    }
        });
        if let Some(c) = usable {
            self.pose.position = if same_support && anchor_in_range {
                anchor
            } else {
                c.position
            };
            self.pose.normal = if same_support {
                self.pose.normal.lerp(c.normal, alpha)
            } else {
                c.normal
            }
            .normalize_or(Vec3::Y);
            self.pose.weight += (1. - self.pose.weight) * alpha;
            self.support = Some(c.support);
            self.missing_seconds = 0.;
        } else if plant
            && self.support.is_some()
            && (contact.is_none() || same_support)
            && anchor_in_range
            && self.missing_seconds + f64::from(dt) < f64::from(self.thresholds.release_delay)
        {
            self.pose.position = anchor;
            self.missing_seconds += f64::from(dt);
        } else {
            self.support = None;
            self.missing_seconds = 0.;
            self.pose.weight *= 1. - alpha;
            self.pose.position = animated;
            self.pose.normal = Vec3::Y;
        }
        self.correction = (self.pose.position - animated) * self.pose.weight;
        Ok(self.pose)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ik_preserves_lengths_and_handles_unreachable_and_collinear_poles() {
        for target in [Vec3::ZERO, Vec3::X, Vec3::X * 5.] {
            let p = TwoBone {
                upper: 0.6,
                lower: 0.4,
            }
            .solve(Vec3::ZERO, target, Vec3::X)
            .unwrap();
            assert!((p.joint.distance(p.root) - 0.6).abs() < 0.0001);
            assert!((p.end.distance(p.joint) - 0.4).abs() < 0.0001);
            assert!(p.joint.is_finite());
        }
    }
    #[test]
    fn contact_moves_with_support_and_releases_without_changing_body() {
        let mut p = FootPlant::new(0.5, 0.01).unwrap();
        let c = FootContact {
            position: Vec3::Y * 0.1,
            normal: Vec3::Y,
            support: 2,
            support_delta: Vec3::ZERO,
        };
        let a = p.update(Vec3::ZERO, Some(c), true, 0.1).unwrap();
        assert!(a.weight > 0.9);
        let b = p
            .update(
                Vec3::ZERO,
                Some(FootContact {
                    support_delta: Vec3::X * 0.1,
                    ..c
                }),
                true,
                0.1,
            )
            .unwrap();
        assert!(b.position.x > 0.09);
        let gone = p.update(Vec3::ZERO, None, false, 0.1).unwrap();
        assert!(gone.weight < 0.01);
    }

    fn thresholds() -> FootPlantThresholds {
        FootPlantThresholds {
            enter_distance: 0.3,
            enter_normal_y: 0.6,
            exit_normal_y: 0.4,
            release_delay: 0.0625,
        }
    }

    fn contact(position: Vec3, normal_y: f32) -> FootContact {
        FootContact {
            position,
            normal: Vec3::new((1. - normal_y * normal_y).sqrt(), normal_y, 0.),
            support: 7,
            support_delta: Vec3::ZERO,
        }
    }

    fn planted() -> FootPlant {
        let mut foot = FootPlant::with_thresholds(0.5, 0.1, thresholds()).unwrap();
        foot.update(Vec3::ZERO, Some(contact(Vec3::Y * 0.2, 1.)), true, 0.1)
            .unwrap();
        foot
    }

    fn assert_same_state(a: FootPlant, b: FootPlant) {
        assert_eq!(a.pose(), b.pose());
        assert_eq!(a.support(), b.support());
        assert_eq!(a.correction(), b.correction());
        assert_eq!(a.thresholds(), b.thresholds());
        assert_eq!(a.missing_seconds, b.missing_seconds);
    }

    #[test]
    fn thresholds_reject_nonfinite_unordered_and_out_of_range_values() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 0.51] {
            assert!(matches!(
                FootPlant::with_thresholds(
                    0.5,
                    0.1,
                    FootPlantThresholds {
                        enter_distance: value,
                        ..thresholds()
                    }
                ),
                Err(AnimationError::InvalidPose)
            ));
        }
        for value in [f32::NAN, f32::INFINITY, -0.01, 0.3, 1.] {
            assert!(matches!(
                FootPlant::with_thresholds(
                    0.5,
                    0.1,
                    FootPlantThresholds {
                        enter_normal_y: value,
                        ..thresholds()
                    }
                ),
                Err(AnimationError::InvalidPose)
            ));
        }
        for value in [f32::NAN, f32::INFINITY, -0.01, 0.61] {
            assert!(matches!(
                FootPlant::with_thresholds(
                    0.5,
                    0.1,
                    FootPlantThresholds {
                        exit_normal_y: value,
                        ..thresholds()
                    }
                ),
                Err(AnimationError::InvalidPose)
            ));
        }
        for value in [f32::NAN, f32::INFINITY, -0.01, 0.251] {
            assert!(matches!(
                FootPlant::with_thresholds(
                    0.5,
                    0.1,
                    FootPlantThresholds {
                        release_delay: value,
                        ..thresholds()
                    }
                ),
                Err(AnimationError::InvalidPose)
            ));
        }
        for (maximum, half_life) in [
            (f32::NAN, 0.1),
            (f32::INFINITY, 0.1),
            (0., 0.1),
            (10.1, 0.1),
            (0.5, f32::NAN),
            (0.5, f32::INFINITY),
            (0.5, 0.),
            (0.5, 1.1),
        ] {
            assert!(FootPlant::new(maximum, half_life).is_err());
        }
        for (maximum, half_life, enter_distance, release_delay) in
            [(0.001, 0.001, 0., 0.), (10., 1., 10., 0.25)]
        {
            assert!(
                FootPlant::with_thresholds(
                    maximum,
                    half_life,
                    FootPlantThresholds {
                        enter_distance,
                        enter_normal_y: 0.,
                        exit_normal_y: 0.,
                        release_delay,
                    },
                )
                .is_ok()
            );
        }
    }

    #[test]
    fn legacy_constructor_keeps_single_cutoffs_and_immediate_release() {
        let mut foot = FootPlant::new(0.5, 0.1).unwrap();
        for sample in [contact(Vec3::Y * 0.501, 1.), contact(Vec3::Y * 0.2, 0.5)] {
            foot.update(Vec3::ZERO, Some(sample), true, 0.1).unwrap();
            assert_eq!(foot.support(), None);
        }
        foot.update(Vec3::ZERO, Some(contact(Vec3::Y * 0.5, 1.)), true, 0.1)
            .unwrap();
        assert_eq!(foot.support(), Some(7));
        foot.update(Vec3::ZERO, None, true, 0.001).unwrap();
        assert_eq!(foot.support(), None);
        assert_eq!(foot.correction(), Vec3::ZERO);
    }

    #[test]
    fn marginal_contacts_retain_an_anchor_but_cannot_acquire_one() {
        let mut foot = FootPlant::with_thresholds(0.5, 0.1, thresholds()).unwrap();
        for sample in [contact(Vec3::Y * 0.31, 1.), contact(Vec3::Y * 0.2, 0.6)] {
            foot.update(Vec3::ZERO, Some(sample), true, 0.1).unwrap();
            assert_eq!(foot.support(), None);
        }
        let anchor = Vec3::Y * 0.3;
        foot.update(Vec3::ZERO, Some(contact(anchor, 1.)), true, 0.1)
            .unwrap();
        for i in 0..32 {
            // Both values straddle the old single cutoff while remaining
            // inside the new retention band, for longer than the grace period.
            let normal_y = if i % 2 == 0 { 0.49 } else { 0.51 };
            foot.update(
                Vec3::ZERO,
                Some(contact(Vec3::Y * 0.5, normal_y)),
                true,
                0.01,
            )
            .unwrap();
            assert_eq!(foot.support(), Some(7));
            assert_eq!(foot.pose().position, anchor);
            assert!(foot.pose().normal.is_normalized());
        }
    }

    #[test]
    fn contact_loss_grace_uses_elapsed_time_and_recovery_rearms_it() {
        let mut foot = planted();
        let initial = foot.pose();
        for _ in 0..3 {
            assert_eq!(foot.update(Vec3::ZERO, None, true, 0.015625), Ok(initial));
            assert_eq!(foot.support(), Some(7));
        }
        foot.update(Vec3::ZERO, None, true, 0.015625).unwrap();
        assert_eq!(foot.support(), None);
        assert_eq!(foot.pose().position, Vec3::ZERO);

        let mut foot = planted();
        foot.update(Vec3::ZERO, None, true, 0.046875).unwrap();
        foot.update(Vec3::ZERO, Some(contact(Vec3::Y * 0.2, 1.)), true, 0.01)
            .unwrap();
        foot.update(Vec3::ZERO, None, true, 0.046875).unwrap();
        assert_eq!(foot.support(), Some(7));
        foot.update(Vec3::ZERO, None, true, 0.015625).unwrap();
        assert_eq!(foot.support(), None);
    }

    #[test]
    fn marginal_distance_and_slope_have_bounded_grace() {
        for sample in [contact(Vec3::Y * 0.501, 1.), contact(Vec3::Y * 0.2, 0.4)] {
            let mut foot = planted();
            let initial = foot.pose();
            foot.update(Vec3::ZERO, Some(sample), true, 0.03125)
                .unwrap();
            assert_eq!(foot.support(), Some(7));
            assert_eq!(foot.pose(), initial);
            foot.update(Vec3::ZERO, Some(sample), true, 0.03125)
                .unwrap();
            assert_eq!(foot.support(), None);
        }
    }

    #[test]
    fn plant_release_and_support_changes_bypass_grace() {
        let mut foot = planted();
        foot.update(Vec3::ZERO, Some(contact(Vec3::Y * 0.2, 1.)), false, 0.001)
            .unwrap();
        assert_eq!(foot.support(), None);
        assert_eq!(foot.correction(), Vec3::ZERO);

        let mut foot = planted();
        let other = FootContact {
            support: 8,
            support_delta: Vec3::X * 100.,
            ..contact(Vec3::Y * 0.4, 0.5)
        };
        foot.update(Vec3::ZERO, Some(other), true, 0.001).unwrap();
        assert_eq!(foot.support(), None);
        let other = FootContact {
            position: Vec3::Z * 0.2,
            normal: Vec3::Y,
            ..other
        };
        foot.update(Vec3::ZERO, Some(other), true, 0.001).unwrap();
        assert_eq!(foot.support(), Some(8));
        assert_eq!(foot.pose().position, other.position);
    }

    #[test]
    fn moving_support_is_followed_exactly_even_during_low_weight_and_grace() {
        let mut foot = FootPlant::with_thresholds(0.5, 1., thresholds()).unwrap();
        foot.update(Vec3::ZERO, Some(contact(Vec3::Y * 0.2, 1.)), true, 0.001)
            .unwrap();
        assert!(foot.pose().weight < 0.01);
        for step in 1..=3 {
            let sample = FootContact {
                // Deliberately jitter the query while keeping the anchor.
                position: Vec3::Y * 0.25,
                support_delta: Vec3::X * 0.05,
                ..contact(Vec3::ZERO, 1.)
            };
            foot.update(Vec3::ZERO, Some(sample), true, 0.001).unwrap();
            assert!((foot.pose().position.x - step as f32 * 0.05).abs() < 1e-6);
            assert_eq!(foot.pose().position.y, 0.2);
        }
        let sample = FootContact {
            support_delta: Vec3::X * 0.05,
            ..contact(Vec3::Y * 0.2, 0.3)
        };
        foot.update(Vec3::ZERO, Some(sample), true, 0.01).unwrap();
        assert!((foot.pose().position.x - 0.2).abs() < 1e-6);
        assert_eq!(foot.support(), Some(7));
    }

    #[test]
    fn correction_is_weighted_and_unreachable_anchors_never_leak() {
        let mut foot = planted();
        assert!((foot.correction().y - 0.1).abs() < 1e-6);
        let animated = Vec3::X * 0.4;
        let sample = FootContact {
            support_delta: -Vec3::X * 0.2,
            ..contact(animated + Vec3::Y * 0.2, 1.)
        };
        let pose = foot.update(animated, Some(sample), true, 0.001).unwrap();
        // The old anchor is now too far away; reanchor without blending from it.
        assert_eq!(pose.position, sample.position);
        assert_eq!(foot.correction(), (pose.position - animated) * pose.weight);
        assert!(foot.correction().length() <= 0.5);

        foot.update(Vec3::X * 10., None, true, 0.001).unwrap();
        assert_eq!(foot.support(), None);
        assert_eq!(foot.correction(), Vec3::ZERO);
        assert_eq!(foot.pose().position, Vec3::X * 10.);
    }

    #[test]
    fn zero_time_and_rejected_updates_preserve_anchor_and_grace_timer() {
        let mut foot = planted();
        foot.update(Vec3::ZERO, None, true, 0.03125).unwrap();
        let before = foot;
        for (animated, sample, plant) in [
            (Vec3::X * 10., None, false),
            (
                Vec3::ZERO,
                Some(FootContact {
                    support_delta: Vec3::X * 0.1,
                    ..contact(Vec3::Y * 0.2, 1.)
                }),
                true,
            ),
        ] {
            foot.update(animated, sample, plant, 0.).unwrap();
            assert_same_state(foot, before);
        }
        let valid = contact(Vec3::Y * 0.2, 1.);
        for (animated, sample, dt) in [
            (Vec3::NAN, Some(valid), 0.01),
            (Vec3::INFINITY, Some(valid), 0.01),
            (Vec3::ZERO, Some(valid), f32::NAN),
            (Vec3::ZERO, Some(valid), f32::INFINITY),
            (Vec3::ZERO, Some(valid), -0.01),
            (Vec3::ZERO, Some(valid), 0.251),
            (
                Vec3::ZERO,
                Some(FootContact {
                    position: Vec3::NAN,
                    ..valid
                }),
                0.01,
            ),
            (
                Vec3::ZERO,
                Some(FootContact {
                    normal: Vec3::ZERO,
                    ..valid
                }),
                0.01,
            ),
            (
                Vec3::ZERO,
                Some(FootContact {
                    normal: Vec3::NAN,
                    ..valid
                }),
                0.,
            ),
            (
                Vec3::ZERO,
                Some(FootContact {
                    support_delta: Vec3::INFINITY,
                    ..valid
                }),
                0.01,
            ),
        ] {
            assert_eq!(
                foot.update(animated, sample, true, dt),
                Err(AnimationError::InvalidPose)
            );
            assert_same_state(foot, before);
        }
        foot.update(Vec3::ZERO, None, true, 0.03125).unwrap();
        assert_eq!(foot.support(), None);
    }

    #[test]
    fn clear_and_reset_discard_normal_anchor_weight_and_grace_history() {
        let mut foot = planted();
        foot.update(Vec3::ZERO, Some(contact(Vec3::Y * 0.2, 0.5)), true, 0.1)
            .unwrap();
        foot.update(Vec3::ZERO, None, true, 0.03125).unwrap();
        assert_ne!(foot.pose().normal, Vec3::Y);
        let before = foot;
        assert_eq!(foot.reset(Vec3::NAN), Err(AnimationError::InvalidPose));
        assert_same_state(foot, before);
        foot.clear();
        assert_eq!(
            foot.pose(),
            FootPose {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                weight: 0.
            }
        );
        assert_eq!(foot.correction(), Vec3::ZERO);
        assert_eq!(foot.support(), None);
        // Reset must not let a former support bypass acquisition thresholds.
        foot.update(Vec3::ZERO, Some(contact(Vec3::Y * 0.2, 0.5)), true, 0.01)
            .unwrap();
        assert_eq!(foot.support(), None);

        let location = Vec3::X * 10.;
        let pose = foot.reset(location).unwrap();
        assert_eq!(pose.position, location);
        assert_eq!(pose.normal, Vec3::Y);
        assert_eq!(pose.weight, 0.);
        foot.update(
            location,
            Some(contact(location + Vec3::Y * 0.2, 1.)),
            true,
            0.01,
        )
        .unwrap();
        foot.update(location, None, true, 0.046875).unwrap();
        assert_eq!(foot.support(), Some(7));
        assert_eq!(foot.pose().normal, Vec3::Y);
    }

    #[test]
    fn extreme_finite_positions_and_support_deltas_keep_output_finite() {
        let huge = Vec3::splat(f32::MAX);
        let mut foot = planted();
        foot.update(huge, Some(contact(-huge, 1.)), true, 0.01)
            .unwrap();
        assert_eq!(foot.pose().position, huge);
        assert_eq!(foot.correction(), Vec3::ZERO);
        foot.update(huge, Some(contact(huge, 1.)), true, 0.01)
            .unwrap();
        let sample = FootContact {
            support_delta: huge,
            ..contact(huge, 1.)
        };
        let pose = foot.update(huge, Some(sample), true, 0.01).unwrap();
        assert!(pose.position.is_finite());
        assert!(pose.normal.is_normalized());
        assert!((0. ..=1.).contains(&pose.weight));
        assert_eq!(foot.correction(), Vec3::ZERO);
    }

    #[test]
    fn two_bone_extremes_remain_finite_and_bound_unreachable_targets() {
        for (upper, lower) in [
            (0.001, 0.001),
            (1000., 1000.),
            (0.001, 1000.),
            (1000., 0.001),
        ] {
            let limb = TwoBone { upper, lower };
            for target in [Vec3::ZERO, Vec3::X * 0.001, Vec3::splat(1e6)] {
                for pole in [Vec3::ZERO, target, -Vec3::Y] {
                    let pose = limb.solve(Vec3::ZERO, target, pole).unwrap();
                    assert!(pose.joint.is_finite());
                    assert!(pose.end.is_finite());
                    let reach = pose.end.length();
                    assert!(reach <= upper + lower + 0.001);
                    assert!(reach + 0.001 >= (upper - lower).abs());
                    if target == Vec3::splat(1e6) {
                        assert!(pose.clamped);
                    }
                }
            }
        }
    }
}
