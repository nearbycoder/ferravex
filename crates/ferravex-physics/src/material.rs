//! Checked contact-material authoring and symmetric pair mixing.
//!
//! Pair rules use the higher-priority enum variant independently for friction
//! and restitution. Defaults remain geometric friction, average restitution and
//! average rolling resistance. Mixing describes contact coefficients; the solver
//! separately decides when to apply friction, bounce and rolling resistance.

/// Why a checked scalar combination failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombineError {
    /// The first coefficient is negative or non-finite.
    InvalidFirst,
    /// The second coefficient is negative or non-finite.
    InvalidSecond,
    /// The result is too large to represent as a finite `f32`.
    Overflow,
}

impl std::fmt::Display for CombineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidFirst => "first coefficient must be finite and nonnegative",
            Self::InvalidSecond => "second coefficient must be finite and nonnegative",
            Self::Overflow => "combined coefficient exceeds finite f32 range",
        })
    }
}

impl std::error::Error for CombineError {}

/// Material authoring failed before any collider was changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialError {
    /// Static friction must be finite and in `0..=1000`.
    InvalidStaticFriction,
    /// Sliding friction must be finite and in `0..=1000`.
    InvalidSlidingFriction,
    /// Sliding friction must not exceed static friction.
    SlidingExceedsStatic,
    /// Rolling resistance must be finite and in `0..=10` meters.
    InvalidRollingResistance,
    /// Restitution must be finite and in `0..=1`.
    InvalidRestitution,
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidStaticFriction => "static friction must be finite and in 0..=1000",
            Self::InvalidSlidingFriction => "sliding friction must be finite and in 0..=1000",
            Self::SlidingExceedsStatic => "sliding friction must not exceed static friction",
            Self::InvalidRollingResistance => {
                "rolling resistance must be finite and in 0..=10 meters"
            }
            Self::InvalidRestitution => "restitution must be finite and in 0..=1",
        })
    }
}

impl std::error::Error for MaterialError {}

/// A coefficient mixing rule, in increasing priority order:
/// `Average < Geometric < Minimum < Maximum < Multiply`.
///
/// A material pair selects `a_rule.max(b_rule)`, regardless of collider order.
/// Friction and restitution select their rules independently. With one zero
/// coefficient, `Geometric`, `Minimum` and `Multiply` yield zero; `Average`
/// yields half the other coefficient and `Maximum` yields the other coefficient.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum CombineRule {
    /// Arithmetic mean: `(a + b) / 2`.
    Average,
    /// Geometric mean: `sqrt(a * b)`. Default for friction.
    #[default]
    Geometric,
    /// Smaller coefficient.
    Minimum,
    /// Larger coefficient.
    Maximum,
    /// Product; friction may exceed either authored coefficient.
    Multiply,
}
impl CombineRule {
    /// Apply this rule without validating the coefficients or selecting priority.
    ///
    /// Average and geometric mean avoid intermediate overflow/underflow for
    /// finite inputs. Ordinary calculations retain their existing `f32` rounding;
    /// only extreme intermediates use `f64`. Multiplication can overflow to
    /// infinity or round tiny products to zero. NaN, infinity and negative inputs
    /// follow floating-point arithmetic; use [`Self::try_combine`] to reject them.
    pub fn combine(self, a: f32, b: f32) -> f32 {
        match self {
            Self::Average => {
                let sum = a + b;
                if sum.is_infinite() && a.is_finite() && b.is_finite() {
                    ((f64::from(a) + f64::from(b)) * 0.5) as f32
                } else {
                    sum * 0.5
                }
            }
            Self::Geometric => {
                let product = a * b;
                if (product.is_infinite() || product.abs() < f32::MIN_POSITIVE)
                    && a.is_finite()
                    && b.is_finite()
                {
                    (f64::from(a) * f64::from(b)).sqrt() as f32
                } else {
                    product.sqrt()
                }
            }
            Self::Minimum => a.min(b),
            Self::Maximum => a.max(b),
            Self::Multiply => a * b,
        }
    }

    /// Combine finite, nonnegative coefficients without clamping.
    ///
    /// This scalar helper does not enforce material authoring bounds or select
    /// the higher-priority rule. Both inputs are checked, including a coefficient
    /// that `Minimum`/`Maximum` would otherwise ignore. The first invalid input
    /// wins. A non-finite result is an overflow error; underflow to zero in
    /// `Multiply` is allowed. Signed zero is accepted.
    ///
    /// ```
    /// use ferravex_physics::CombineRule;
    ///
    /// assert_eq!(CombineRule::Geometric.try_combine(0.0, 0.8).unwrap(), 0.0);
    /// assert_eq!(CombineRule::Average.try_combine(f32::MAX, f32::MAX).unwrap(), f32::MAX);
    /// assert!(CombineRule::Multiply.try_combine(f32::MAX, 2.0).is_err());
    /// assert!(CombineRule::Minimum.try_combine(0.0, f32::NAN).is_err());
    /// ```
    pub fn try_combine(self, a: f32, b: f32) -> Result<f32, CombineError> {
        if !a.is_finite() || a < 0.0 {
            return Err(CombineError::InvalidFirst);
        }
        if !b.is_finite() || b < 0.0 {
            return Err(CombineError::InvalidSecond);
        }
        let combined = self.combine(a, b);
        if !combined.is_finite() {
            return Err(CombineError::Overflow);
        }
        Ok(combined)
    }
}

/// Authored collider coefficients, or effective coefficients from a material pair.
///
/// Struct literals remain supported. Use [`Self::validate`] before applying an
/// authored value, or [`Self::try_new`] for default-rule authoring. Pair outputs
/// may exceed the authored friction limit when using `Multiply`; see
/// [`Self::try_combine`]. These operations allocate no storage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsMaterial {
    /// Authored static friction in `0..=1000`, at least sliding friction.
    pub static_friction: f32,
    /// Authored sliding friction in `0..=1000`.
    pub sliding_friction: f32,
    /// Contact torque impulse limit = normal impulse * this length (meters).
    /// Authored range is `0..=10`. Resists rolling and spin; zero retains the
    /// historical behavior. Pairs always use the arithmetic mean.
    pub rolling_resistance: f32,
    /// Authored restitution in `0..=1`.
    pub restitution: f32,
    /// Rule shared by static and sliding friction; the pair's higher rule wins.
    pub friction_rule: CombineRule,
    /// Independent restitution rule; the pair's higher rule wins.
    pub restitution_rule: CombineRule,
}
impl Default for PhysicsMaterial {
    fn default() -> Self {
        Self {
            static_friction: 0.6,
            sliding_friction: 0.6,
            rolling_resistance: 0.,
            restitution: 0.,
            friction_rule: CombineRule::Geometric,
            restitution_rule: CombineRule::Average,
        }
    }
}
impl PhysicsMaterial {
    /// Author checked coefficients with the default mixing rules and no rolling
    /// resistance. Invalid values are rejected, never clamped or reordered.
    ///
    /// ```
    /// use ferravex_physics::PhysicsMaterial;
    ///
    /// let rubber = PhysicsMaterial::try_new(0.9, 0.7, 0.8).unwrap();
    /// assert!(rubber.valid());
    /// assert!(PhysicsMaterial::try_new(0.2, 0.7, 0.8).is_err());
    /// ```
    pub fn try_new(
        static_friction: f32,
        sliding_friction: f32,
        restitution: f32,
    ) -> Result<Self, MaterialError> {
        let material = Self {
            static_friction,
            sliding_friction,
            restitution,
            ..Self::default()
        };
        material.validate()?;
        Ok(material)
    }

    /// Validate authored coefficients without changing them.
    ///
    /// Reports the first error in this order: static friction range, sliding
    /// friction range, sliding/static ordering, rolling resistance, restitution.
    /// All bounds are inclusive; signed zero and positive subnormals are valid.
    /// Every non-finite value is rejected. Mixing rules do not affect validity.
    ///
    /// ```
    /// use ferravex_physics::{CombineRule, PhysicsMaterial};
    ///
    /// let material = PhysicsMaterial {
    ///     rolling_resistance: 0.05,
    ///     restitution_rule: CombineRule::Maximum,
    ///     ..PhysicsMaterial::default()
    /// };
    /// assert!(material.validate().is_ok());
    /// let invalid = PhysicsMaterial { restitution: 1.01, ..material };
    /// assert!(invalid.validate().is_err());
    /// ```
    pub fn validate(self) -> Result<(), MaterialError> {
        if !self.static_friction.is_finite() || !(0.0..=1000.0).contains(&self.static_friction) {
            return Err(MaterialError::InvalidStaticFriction);
        }
        if !self.sliding_friction.is_finite() || !(0.0..=1000.0).contains(&self.sliding_friction) {
            return Err(MaterialError::InvalidSlidingFriction);
        }
        if self.sliding_friction > self.static_friction {
            return Err(MaterialError::SlidingExceedsStatic);
        }
        if !self.rolling_resistance.is_finite() || !(0.0..=10.0).contains(&self.rolling_resistance)
        {
            return Err(MaterialError::InvalidRollingResistance);
        }
        if !self.restitution.is_finite() || !(0.0..=1.0).contains(&self.restitution) {
            return Err(MaterialError::InvalidRestitution);
        }
        Ok(())
    }

    /// Whether this material meets the authored bounds of [`Self::validate`].
    pub fn valid(self) -> bool {
        self.validate().is_ok()
    }

    /// Inspect the effective contact material using the solver's pair rules.
    ///
    /// Validates `self`, then `other`, as authored materials. For each of friction
    /// and restitution, the higher-priority [`CombineRule`] wins. Rolling
    /// resistance always uses the arithmetic mean. Inputs are unchanged.
    ///
    /// The result is finite and nonnegative, with static friction at least
    /// sliding friction and restitution in `0..=1`. Friction is **not clamped**:
    /// `Multiply` can yield up to `1_000_000`, preserving solver behavior. Such a
    /// result is contact data and can fail [`Self::validate`] if reused as an
    /// authored collider material (or as an input to this helper).
    ///
    /// Zero friction on either collider makes default geometric mixing zero.
    /// `Maximum` can override that behavior; zero is not an unconditional opt-out
    /// of friction. Rolling resistance is independent of friction coefficients.
    ///
    /// ```
    /// use ferravex_physics::{CombineRule, PhysicsMaterial};
    ///
    /// let ice = PhysicsMaterial::try_new(0.0, 0.0, 0.0).unwrap();
    /// let rubber = PhysicsMaterial::try_new(0.9, 0.7, 0.8).unwrap();
    /// let contact = ice.try_combine(rubber).unwrap();
    /// assert_eq!(contact.sliding_friction, 0.0);
    /// assert_eq!(contact.restitution, 0.4);
    /// assert_eq!(contact, rubber.try_combine(ice).unwrap());
    /// let grip = PhysicsMaterial { friction_rule: CombineRule::Maximum, ..rubber };
    /// assert_eq!(ice.try_combine(grip).unwrap().sliding_friction, 0.7);
    /// ```
    pub fn try_combine(self, other: Self) -> Result<Self, MaterialError> {
        self.validate()?;
        other.validate()?;
        Ok(self.pair(other))
    }
    pub(crate) fn pair(self, other: Self) -> Self {
        let f = self.friction_rule.max(other.friction_rule);
        let r = self.restitution_rule.max(other.restitution_rule);
        Self {
            static_friction: f.combine(self.static_friction, other.static_friction),
            sliding_friction: f.combine(self.sliding_friction, other.sliding_friction),
            rolling_resistance: (self.rolling_resistance + other.rolling_resistance) * 0.5,
            restitution: r.combine(self.restitution, other.restitution),
            friction_rule: f,
            restitution_rule: r,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const RULES: [CombineRule; 5] = [
        CombineRule::Average,
        CombineRule::Geometric,
        CombineRule::Minimum,
        CombineRule::Maximum,
        CombineRule::Multiply,
    ];

    #[test]
    fn pairs_are_symmetric_and_defaults_preserve_rules() {
        let a = PhysicsMaterial {
            static_friction: 1.,
            sliding_friction: 0.5,
            restitution: 0.6,
            ..Default::default()
        };
        let b = PhysicsMaterial {
            friction_rule: CombineRule::Maximum,
            restitution: 0.2,
            ..Default::default()
        };
        assert_eq!(a.pair(b), b.pair(a));
        assert_eq!(a.pair(b).static_friction, 1.);
        assert!((a.pair(b).restitution - 0.4).abs() < 1e-6);
        assert!(
            !PhysicsMaterial {
                static_friction: 0.1,
                sliding_friction: 0.5,
                ..a
            }
            .valid()
        );
    }

    #[test]
    fn checked_authoring_preserves_defaults_and_reports_each_field() {
        assert_eq!(
            PhysicsMaterial::try_new(0.6, 0.6, 0.0),
            Ok(PhysicsMaterial::default())
        );
        assert_eq!(
            PhysicsMaterial::try_new(0.2, 0.7, 0.8),
            Err(MaterialError::SlidingExceedsStatic)
        );
        for bad in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -f32::from_bits(1),
            f32::from_bits(1000.0_f32.to_bits() + 1),
            f32::MAX,
        ] {
            let static_error = PhysicsMaterial::try_new(bad, 0.0, 0.0);
            assert_eq!(static_error, Err(MaterialError::InvalidStaticFriction));
            let sliding_error = PhysicsMaterial::try_new(1000.0, bad, 0.0);
            assert_eq!(sliding_error, Err(MaterialError::InvalidSlidingFriction));
        }
        for (max, expected) in [
            (10.0_f32, MaterialError::InvalidRollingResistance),
            (1.0, MaterialError::InvalidRestitution),
        ] {
            for bad in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -f32::from_bits(1),
                f32::from_bits(max.to_bits() + 1),
                f32::MAX,
            ] {
                let mut material = PhysicsMaterial::default();
                if expected == MaterialError::InvalidRollingResistance {
                    material.rolling_resistance = bad;
                } else {
                    material.restitution = bad;
                }
                assert_eq!(material.validate(), Err(expected));
                assert!(!material.valid());
            }
        }
    }

    #[test]
    fn validation_accepts_endpoints_signed_zero_and_subnormals() {
        let max = PhysicsMaterial {
            static_friction: 1000.0,
            sliding_friction: 1000.0,
            rolling_resistance: 10.0,
            restitution: 1.0,
            ..Default::default()
        };
        assert_eq!(max.validate(), Ok(()));
        for value in [0.0, -0.0, f32::from_bits(1), f32::MIN_POSITIVE] {
            let material = PhysicsMaterial {
                static_friction: value,
                sliding_friction: value,
                rolling_resistance: value,
                restitution: value,
                ..Default::default()
            };
            assert!(material.valid());
            assert_eq!(material.validate(), Ok(()));
            let authored = PhysicsMaterial::try_new(value, value, value).unwrap();
            assert_eq!(authored.static_friction.to_bits(), value.to_bits());
        }
    }

    #[test]
    fn validation_reports_first_error_and_combining_checks_both_inputs() {
        let invalid_first = PhysicsMaterial {
            static_friction: -1.0,
            restitution: f32::NAN,
            ..Default::default()
        };
        let invalid_second = PhysicsMaterial {
            rolling_resistance: f32::INFINITY,
            ..Default::default()
        };
        let valid = PhysicsMaterial::default();
        assert_eq!(
            invalid_first.try_combine(invalid_second),
            Err(MaterialError::InvalidStaticFriction)
        );
        assert_eq!(
            valid.try_combine(invalid_second),
            Err(MaterialError::InvalidRollingResistance)
        );
        assert_eq!(
            invalid_second.try_combine(valid),
            Err(MaterialError::InvalidRollingResistance)
        );
        assert_eq!(valid, PhysicsMaterial::default());
    }

    #[test]
    fn pair_priority_is_symmetric_and_independent_for_each_coefficient() {
        for (fi, friction_a) in RULES.into_iter().enumerate() {
            for (fj, friction_b) in RULES.into_iter().enumerate() {
                for (ri, restitution_a) in RULES.into_iter().enumerate() {
                    for (rj, restitution_b) in RULES.into_iter().enumerate() {
                        let a = PhysicsMaterial {
                            static_friction: 4.0,
                            sliding_friction: 1.0,
                            rolling_resistance: 0.25,
                            restitution: 0.25,
                            friction_rule: friction_a,
                            restitution_rule: restitution_a,
                        };
                        let b = PhysicsMaterial {
                            static_friction: 1.0,
                            sliding_friction: 0.25,
                            rolling_resistance: 0.75,
                            restitution: 1.0,
                            friction_rule: friction_b,
                            restitution_rule: restitution_b,
                        };
                        let contact = a.try_combine(b).unwrap();
                        assert_eq!(contact, b.try_combine(a).unwrap());
                        assert_eq!(contact, a.pair(b));
                        assert_eq!(contact.friction_rule, RULES[fi.max(fj)]);
                        assert_eq!(contact.restitution_rule, RULES[ri.max(rj)]);
                        assert_eq!(contact.rolling_resistance, 0.5);
                        assert!(contact.static_friction >= contact.sliding_friction);
                        assert!((0.0..=1.0).contains(&contact.restitution));
                    }
                }
            }
        }
    }

    #[test]
    fn zero_friction_obeys_selected_rule_and_does_not_disable_rolling() {
        for (rule, expected) in RULES.into_iter().zip([0.5, 0.0, 0.0, 1.0, 0.0]) {
            let zero = PhysicsMaterial {
                static_friction: 0.0,
                sliding_friction: 0.0,
                restitution: 0.0,
                friction_rule: rule,
                restitution_rule: rule,
                ..Default::default()
            };
            let other = PhysicsMaterial {
                static_friction: 1.0,
                sliding_friction: 1.0,
                rolling_resistance: 1.0,
                restitution: 1.0,
                ..zero
            };
            let contact = zero.try_combine(other).unwrap();
            assert_eq!(contact.static_friction, expected);
            assert_eq!(contact.sliding_friction, expected);
            assert_eq!(contact.restitution, expected);
            assert_eq!(contact.rolling_resistance, 0.5);
        }
        let ice = PhysicsMaterial::try_new(0.0, 0.0, 0.0).unwrap();
        let defaults = ice.try_combine(PhysicsMaterial::default()).unwrap();
        assert_eq!(defaults.static_friction, 0.0);
        assert_eq!(defaults.sliding_friction, 0.0);
    }

    #[test]
    fn multiplied_contact_friction_preserves_unclamped_solver_range() {
        let authored = PhysicsMaterial {
            static_friction: 1000.0,
            sliding_friction: 500.0,
            rolling_resistance: 10.0,
            restitution: 1.0,
            friction_rule: CombineRule::Multiply,
            restitution_rule: CombineRule::Multiply,
        };
        let contact = authored.try_combine(authored).unwrap();
        assert_eq!(contact.static_friction, 1_000_000.0);
        assert_eq!(contact.sliding_friction, 250_000.0);
        assert_eq!(contact.restitution, 1.0);
        assert_eq!(contact.rolling_resistance, 10.0);
        assert_eq!(contact, authored.pair(authored));
        assert_eq!(
            contact.validate(),
            Err(MaterialError::InvalidStaticFriction)
        );
    }

    #[test]
    fn checked_scalar_rules_reject_invalid_inputs_even_when_not_selected() {
        for rule in RULES {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
                assert_eq!(rule.try_combine(bad, 0.0), Err(CombineError::InvalidFirst));
                assert_eq!(rule.try_combine(0.0, bad), Err(CombineError::InvalidSecond));
                assert_eq!(rule.try_combine(bad, bad), Err(CombineError::InvalidFirst));
            }
            assert_eq!(rule.try_combine(-0.0, 0.0), Ok(0.0));
        }
    }

    #[test]
    fn scalar_rules_have_documented_results_and_preserve_ordinary_rounding() {
        for (rule, expected) in RULES.into_iter().zip([5.0, 4.0, 2.0, 8.0, 16.0]) {
            assert_eq!(rule.try_combine(2.0, 8.0), Ok(expected));
        }
        for a in [0.0_f32, 0.1, 0.2, 0.5, 0.6, 1.0, 4.0, 1000.0] {
            for b in [0.0_f32, 0.1, 0.2, 0.5, 0.6, 1.0, 4.0, 1000.0] {
                let legacy = [(a + b) * 0.5, (a * b).sqrt(), a.min(b), a.max(b), a * b];
                for (rule, expected) in RULES.into_iter().zip(legacy) {
                    assert_eq!(rule.combine(a, b).to_bits(), expected.to_bits());
                }
            }
        }
    }

    #[test]
    fn extreme_means_remain_finite_and_geometric_mean_does_not_underflow() {
        for rule in [CombineRule::Average, CombineRule::Geometric] {
            assert_eq!(rule.try_combine(f32::MAX, f32::MAX), Ok(f32::MAX));
        }
        for value in [f32::from_bits(1), f32::MIN_POSITIVE, 1e-20] {
            assert_eq!(CombineRule::Geometric.try_combine(value, value), Ok(value));
            let material = PhysicsMaterial::try_new(value, value, 0.0).unwrap();
            assert_eq!(
                material.try_combine(material).unwrap().static_friction,
                value
            );
        }
        let small = f32::from_bits(1);
        let expected = (f64::from(small) * f64::from(f32::MAX)).sqrt() as f32;
        let actual = CombineRule::Geometric.try_combine(small, f32::MAX).unwrap();
        assert!((actual - expected).abs() <= expected * f32::EPSILON);
        assert_eq!(
            CombineRule::Multiply.try_combine(f32::MAX, 2.0),
            Err(CombineError::Overflow)
        );
        assert_eq!(CombineRule::Multiply.try_combine(small, small), Ok(0.0));
    }

    #[test]
    fn error_messages_identify_the_constraint() {
        let errors: [(&dyn std::error::Error, &str); 3] = [
            (&MaterialError::InvalidRestitution, "0..=1"),
            (&MaterialError::SlidingExceedsStatic, "static friction"),
            (&CombineError::InvalidSecond, "second coefficient"),
        ];
        for (error, constraint) in errors {
            assert!(error.to_string().contains(constraint));
        }
    }
}
