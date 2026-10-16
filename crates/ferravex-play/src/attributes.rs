//! Bounded attribute modifiers and timed status effects, with explicit stacking.
//! Periodic pulses report work to the host; the host applies authoritative damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeError {
    Invalid,
    Capacity,
    Incompatible,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stacking {
    Refresh,
    Strongest,
    Stack(u8),
}
#[derive(Clone, Copy, Debug)]
pub struct Periodic {
    pub interval: f32,
    pub amount: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct EffectSpec {
    pub key: u32,
    pub attribute: u16,
    pub tags: u64,
    pub seconds: f32,
    pub additive: f32,
    pub multiplier: f32,
    /// Explicit strength ordering avoids confusing a stronger slow with a larger multiplier.
    pub strength: f32,
    pub stacking: Stacking,
    pub periodic: Option<Periodic>,
}
impl EffectSpec {
    pub fn modifier(key: u32, attribute: u16, multiplier: f32, seconds: f32) -> Self {
        Self {
            key,
            attribute,
            tags: 0,
            seconds,
            additive: 0.,
            multiplier,
            strength: (1. - multiplier).abs(),
            stacking: Stacking::Refresh,
            periodic: None,
        }
    }
    fn valid(self) -> bool {
        self.key != 0
            && self.seconds.is_finite()
            && (0.0001..=3600.).contains(&self.seconds)
            && self.additive.is_finite()
            && self.additive.abs() <= 1e6
            && self.multiplier.is_finite()
            && (0. ..=4.).contains(&self.multiplier)
            && self.strength.is_finite()
            && (0. ..=1e6).contains(&self.strength)
            && !matches!(self.stacking,Stacking::Stack(n) if n==0||n>8)
            && self.periodic.is_none_or(|p| {
                p.interval.is_finite()
                    && (0.01..=3600.).contains(&p.interval)
                    && p.amount.is_finite()
                    && p.amount.abs() <= 1e6
            })
    }
}
#[derive(Clone, Copy, Debug)]
struct Active {
    spec: EffectSpec,
    remaining: f64,
    phase: f64,
    stacks: u8,
}
#[derive(Clone, Copy, Debug)]
pub struct Pulse {
    pub key: u32,
    pub attribute: u16,
    pub count: u32,
    pub amount: f64,
}
#[derive(Clone, Debug)]
pub struct EffectSet<const N: usize> {
    effects: [Option<Active>; N],
}
impl<const N: usize> EffectSet<N> {
    pub fn new() -> Result<Self, AttributeError> {
        if N == 0 || N > 32 {
            return Err(AttributeError::Capacity);
        }
        Ok(Self { effects: [None; N] })
    }
    pub fn apply(&mut self, spec: EffectSpec, immune_tags: u64) -> Result<bool, AttributeError> {
        if !spec.valid() {
            return Err(AttributeError::Invalid);
        }
        if spec.tags & immune_tags != 0 {
            return Ok(false);
        }
        if let Some(active) = self
            .effects
            .iter_mut()
            .flatten()
            .find(|e| e.spec.key == spec.key)
        {
            if active.spec.attribute != spec.attribute
                || active.spec.stacking != spec.stacking
                || active.spec.periodic.map(|p| p.interval) != spec.periodic.map(|p| p.interval)
            {
                return Err(AttributeError::Incompatible);
            }
            match spec.stacking {
                Stacking::Refresh => {
                    active.spec = spec;
                    active.remaining = active.remaining.max(f64::from(spec.seconds));
                }
                Stacking::Strongest => {
                    if spec.strength >= active.spec.strength {
                        active.spec = spec;
                        active.remaining = active.remaining.max(f64::from(spec.seconds));
                    }
                }
                Stacking::Stack(max) => {
                    active.stacks = active.stacks.saturating_add(1).min(max);
                    active.spec = spec;
                    active.remaining = active.remaining.max(f64::from(spec.seconds));
                }
            }
            return Ok(true);
        }
        let slot = self
            .effects
            .iter_mut()
            .find(|e| e.is_none())
            .ok_or(AttributeError::Capacity)?;
        *slot = Some(Active {
            spec,
            remaining: f64::from(spec.seconds),
            phase: 0.,
            stacks: 1,
        });
        Ok(true)
    }
    /// At most N pulse reports regardless of dt; elapsed periodic ticks aggregate.
    /// Refreshing an effect preserves its pulse phase, preventing refresh starvation.
    pub fn advance(&mut self, dt: f32) -> Result<[Option<Pulse>; N], AttributeError> {
        if !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(AttributeError::Invalid);
        }
        let mut pulses = [None; N];
        for (slot, out) in self.effects.iter_mut().zip(pulses.iter_mut()) {
            let Some(active) = slot else { continue };
            let elapsed = f64::from(dt).min(active.remaining);
            if let Some(periodic) = active.spec.periodic {
                active.phase += elapsed;
                let period = f64::from(periodic.interval);
                let count = ((active.phase + 1e-9) / period).floor() as u32;
                active.phase = (active.phase - f64::from(count) * period).max(0.);
                if count > 0 {
                    *out = Some(Pulse {
                        key: active.spec.key,
                        attribute: active.spec.attribute,
                        count,
                        amount: f64::from(periodic.amount)
                            * f64::from(count)
                            * f64::from(active.stacks),
                    });
                }
            }
            active.remaining -= elapsed;
            if active.remaining <= 1e-9 {
                *slot = None;
            }
        }
        Ok(pulses)
    }
    /// Additive-then-multiplicative evaluation, clamped to host bounds.
    pub fn value(
        &self,
        attribute: u16,
        base: f32,
        min: f32,
        max: f32,
    ) -> Result<f32, AttributeError> {
        if ![base, min, max]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1e9)
            || min > max
        {
            return Err(AttributeError::Invalid);
        }
        let mut add = 0f64;
        let mut scale = 1f64;
        for e in self
            .effects
            .iter()
            .flatten()
            .filter(|e| e.spec.attribute == attribute)
        {
            add += f64::from(e.spec.additive) * f64::from(e.stacks);
            scale *= f64::from(e.spec.multiplier).powi(i32::from(e.stacks));
        }
        Ok(((f64::from(base) + add) * scale).clamp(f64::from(min), f64::from(max)) as f32)
    }
    pub fn tags(&self) -> u64 {
        self.effects
            .iter()
            .flatten()
            .fold(0, |tags, e| tags | e.spec.tags)
    }
    pub fn remaining(&self, key: u32) -> f32 {
        self.effects
            .iter()
            .flatten()
            .find(|e| e.spec.key == key)
            .map_or(0., |e| e.remaining as f32)
    }
    pub fn remove(&mut self, key: u32) -> bool {
        if let Some(slot) = self
            .effects
            .iter_mut()
            .find(|e| e.is_some_and(|s| s.spec.key == key))
        {
            *slot = None;
            true
        } else {
            false
        }
    }
    pub fn clear(&mut self) {
        self.effects.fill(None)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Vitals {
    health: f32,
    maximum: f32,
    shield: f32,
    armor: f32,
    resistance: [f32; 8],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Damage {
    pub shield: f32,
    pub health: f32,
    pub killed: bool,
}
impl Vitals {
    pub fn new(
        health: f32,
        shield: f32,
        armor: f32,
        resistance: [f32; 8],
    ) -> Result<Self, AttributeError> {
        if ![health, shield, armor]
            .iter()
            .all(|v| v.is_finite() && (0. ..=1e9).contains(v))
            || health == 0.
            || !resistance
                .iter()
                .all(|r| r.is_finite() && (0. ..=1.).contains(r))
        {
            return Err(AttributeError::Invalid);
        }
        Ok(Self {
            health,
            maximum: health,
            shield,
            armor,
            resistance,
        })
    }
    pub fn health(&self) -> f32 {
        self.health
    }
    pub fn shield(&self) -> f32 {
        self.shield
    }
    pub fn heal(&mut self, amount: f32) -> Result<(), AttributeError> {
        if !amount.is_finite() || amount < 0. {
            return Err(AttributeError::Invalid);
        }
        self.health = (self.health + amount).min(self.maximum);
        Ok(())
    }
    /// Resistance, then flat armor, then shield, then health. Unknown damage kinds fail.
    pub fn damage(&mut self, amount: f32, kind: usize) -> Result<Damage, AttributeError> {
        if !amount.is_finite() || !(0. ..=1e9).contains(&amount) || kind >= 8 {
            return Err(AttributeError::Invalid);
        }
        let effective = (amount * (1. - self.resistance[kind]) - self.armor).max(0.);
        let shield = effective.min(self.shield);
        let health = (effective - shield).min(self.health);
        let was_alive = self.health > 0.;
        self.shield -= shield;
        self.health -= health;
        Ok(Damage {
            shield,
            health,
            killed: was_alive && self.health == 0.,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slows_stack_refresh_and_expire_without_mutating_base() {
        let mut s = EffectSet::<4>::new().unwrap();
        let slow = EffectSpec::modifier(1, 0, 0.5, 1.);
        s.apply(slow, 0).unwrap();
        s.advance(0.5).unwrap();
        s.apply(slow, 0).unwrap();
        assert_eq!(s.value(0, 100., 0., 1000.).unwrap(), 50.);
        let mut stacked = EffectSpec::modifier(2, 0, 0.5, 1.);
        stacked.stacking = Stacking::Stack(2);
        for _ in 0..10 {
            s.apply(stacked, 0).unwrap();
        }
        assert_eq!(s.value(0, 100., 0., 1000.).unwrap(), 12.5);
        s.advance(1.).unwrap();
        assert_eq!(s.value(0, 100., 0., 1000.).unwrap(), 100.);
    }
    #[test]
    fn periodic_refresh_does_not_starve_and_stops_at_expiry() {
        let mut s = EffectSet::<1>::new().unwrap();
        let mut burn = EffectSpec::modifier(1, 0, 1., 1.);
        burn.periodic = Some(Periodic {
            interval: 0.25,
            amount: 2.,
        });
        s.apply(burn, 0).unwrap();
        s.advance(0.125).unwrap();
        s.apply(burn, 0).unwrap();
        assert_eq!(s.advance(0.125).unwrap()[0].unwrap().amount, 2.);
        assert_eq!(s.advance(10.).unwrap()[0].unwrap().count, 3);
        assert_eq!(s.remaining(1), 0.);
    }
    #[test]
    fn resistance_shield_damage_and_rejection_conserve_health() {
        let mut r = [0.; 8];
        r[1] = 0.5;
        let mut v = Vitals::new(100., 10., 2., r).unwrap();
        let hit = v.damage(40., 1).unwrap();
        assert_eq!(
            hit,
            Damage {
                shield: 10.,
                health: 8.,
                killed: false
            }
        );
        assert_eq!((v.health(), v.shield()), (92., 0.));
        assert!(v.damage(f32::NAN, 1).is_err());
        assert_eq!(v.health(), 92.);
        v.heal(1000.).unwrap();
        assert_eq!(v.health(), 100.);
    }
    #[test]
    fn immunity_capacity_strongest_and_invalid_edits_are_explicit() {
        let mut s = EffectSet::<1>::new().unwrap();
        let mut slow = EffectSpec::modifier(1, 0, 0.5, 1.);
        slow.tags = 2;
        slow.stacking = Stacking::Strongest;
        assert!(!s.apply(slow, 2).unwrap());
        s.apply(slow, 0).unwrap();
        slow.multiplier = 0.8;
        slow.strength = 0.2;
        s.apply(slow, 0).unwrap();
        assert_eq!(s.value(0, 100., 0., 100.).unwrap(), 50.);
        slow.key = 2;
        assert_eq!(s.apply(slow, 0), Err(AttributeError::Capacity));
        slow.multiplier = f32::NAN;
        assert!(s.apply(slow, 0).is_err());
        assert_eq!(s.tags(), 2);
        assert!(s.remove(1));
    }
}
