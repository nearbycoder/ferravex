//! Contact materials. Pair rules use the higher-priority enum variant so the
//! result is symmetric regardless of collider ordering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum CombineRule {
    Average,
    #[default]
    Geometric,
    Minimum,
    Maximum,
    Multiply,
}
impl CombineRule {
    pub fn combine(self, a: f32, b: f32) -> f32 {
        match self {
            Self::Average => (a + b) * 0.5,
            Self::Geometric => (a * b).sqrt(),
            Self::Minimum => a.min(b),
            Self::Maximum => a.max(b),
            Self::Multiply => a * b,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsMaterial {
    pub static_friction: f32,
    pub sliding_friction: f32,
    /// Contact torque impulse limit = normal impulse * this length (meters).
    /// Resists rolling and spin; zero retains the historical behavior.
    pub rolling_resistance: f32,
    pub restitution: f32,
    pub friction_rule: CombineRule,
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
    pub fn valid(self) -> bool {
        [self.static_friction, self.sliding_friction]
            .iter()
            .all(|v| v.is_finite() && (0. ..=1000.).contains(v))
            && self.static_friction >= self.sliding_friction
            && self.rolling_resistance.is_finite()
            && (0. ..=10.).contains(&self.rolling_resistance)
            && self.restitution.is_finite()
            && (0. ..=1.).contains(&self.restitution)
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
}
