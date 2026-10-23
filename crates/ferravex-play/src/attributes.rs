//! Bounded attribute modifiers and timed status effects, with explicit stacking.
//! Periodic pulses report work to the host; the host applies authoritative damage.
//!
//! Inspection copies the current simulation state without advancing time or
//! consuming pulses. Feed [`EffectSet::advance`] from fixed simulation ticks.
//!
//! ```
//! use ferravex_play::attributes::{EffectSet, EffectSpec, Periodic, Stacking};
//!
//! let mut effects = EffectSet::<4>::new().unwrap();
//! assert!(effects.is_empty());
//! let mut burn = EffectSpec::modifier(7, 0, 1.0, 2.0);
//! burn.stacking = Stacking::Stack(3);
//! burn.periodic = Some(Periodic { interval: 0.5, amount: 2.0 });
//! effects.apply(burn, 0).unwrap();
//! effects.apply(burn, 0).unwrap();
//! effects.advance(0.125).unwrap();
//!
//! let status = effects.status(7).unwrap();
//! assert_eq!(effects.len(), 1);
//! assert_eq!(status.spec.seconds, 2.0);
//! assert_eq!(status.remaining_seconds, 1.875);
//! assert_eq!(status.stacks, 2);
//! assert_eq!(status.periodic_elapsed_seconds, Some(0.125));
//! assert_eq!(status.periodic_progress(), Some(0.25));
//! assert_eq!(status.seconds_until_pulse(), Some(0.375));
//! assert_eq!(effects.iter().next(), Some(status));
//! ```
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Periodic {
    pub interval: f32,
    pub amount: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
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
            && valid_duration(self.seconds)
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

fn valid_duration(seconds: f32) -> bool {
    seconds.is_finite() && (0.0001..=3600.).contains(&seconds)
}

/// A copied view of one active effect; editing it cannot change the set.
///
/// Times retain the simulation's `f64` precision. See the [module example](crate::attributes)
/// for inspection alongside periodic pulses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectStatus {
    /// The currently applied specification. Its duration is the last accepted
    /// specification's duration, not the effect's age or total lifetime.
    pub spec: EffectSpec,
    /// Simulation seconds until expiry. This may exceed `spec.seconds` after a
    /// duration refresh or reapplication of a shorter specification.
    pub remaining_seconds: f64,
    /// Number of active stacks, including the first application.
    pub stacks: u8,
    /// Elapsed seconds in the current pulse interval, or `None` for a
    /// nonperiodic effect. Refreshes preserve this phase.
    pub periodic_elapsed_seconds: Option<f64>,
}

impl EffectStatus {
    /// Fraction of the current pulse interval elapsed, or `None` for a
    /// nonperiodic effect. Views returned by the set yield a value in `[0, 1)`.
    pub fn periodic_progress(&self) -> Option<f64> {
        let periodic = self.spec.periodic?;
        Some(self.periodic_elapsed_seconds? / f64::from(periodic.interval))
    }

    /// Seconds to the next pulse boundary, or `None` for a nonperiodic effect.
    /// This does not guarantee another pulse: the effect may expire first.
    /// Inspection does not advance time or consume a pulse.
    pub fn seconds_until_pulse(&self) -> Option<f64> {
        let periodic = self.spec.periodic?;
        Some(f64::from(periodic.interval) - self.periodic_elapsed_seconds?)
    }
}

#[derive(Clone, Copy, Debug)]
struct Active {
    spec: EffectSpec,
    remaining: f64,
    phase: f64,
    stacks: u8,
}
impl Active {
    fn status(&self) -> EffectStatus {
        EffectStatus {
            spec: self.spec,
            remaining_seconds: self.remaining,
            stacks: self.stacks,
            periodic_elapsed_seconds: self.spec.periodic.map(|_| self.phase),
        }
    }
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

    /// Number of active effect keys (not the sum of their stacks), at most `N`.
    pub fn len(&self) -> usize {
        self.effects.iter().flatten().count()
    }

    /// Whether there are no active effects.
    pub fn is_empty(&self) -> bool {
        self.effects.iter().all(Option::is_none)
    }

    /// Copies the active status for `key`; missing, expired and zero keys yield
    /// `None`. See the [module example](crate::attributes) for the view's fields.
    pub fn status(&self, key: u32) -> Option<EffectStatus> {
        self.effects
            .iter()
            .flatten()
            .find(|active| active.spec.key == key)
            .map(Active::status)
    }

    /// Iterates over copied statuses without allocation, scanning at most `N`
    /// slots. Order is stable slot order, not key or application order: removal
    /// leaves a vacancy and a new key reuses the first vacancy. Refreshes retain
    /// their slot. Holding a copied status does not borrow or track the set.
    pub fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = EffectStatus> + std::iter::FusedIterator + '_ {
        self.effects.iter().flatten().map(Active::status)
    }

    /// Applies a specification without changing any effect on error or immunity
    /// rejection. Compatible reapplications preserve periodic phase and never
    /// shorten remaining time. A weaker `Strongest` application returns `true`
    /// but leaves the active effect unchanged, including its duration.
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

    /// Ensures an existing effect has at least `seconds` remaining, without
    /// changing its specification, stacks or periodic phase. This never adds
    /// an effect and works at capacity. It does not recheck immunity: the host
    /// decides whether to refresh an already active effect.
    ///
    /// Zero keys and nonfinite durations or durations outside `0.0001..=3600.0`
    /// return [`AttributeError::Invalid`] before any change, even for missing
    /// keys. Valid missing keys return `Ok(false)`; present keys return `Ok(true)`
    /// even if their remaining duration already meets the requested minimum.
    ///
    /// ```
    /// use ferravex_play::attributes::{AttributeError, EffectSet, EffectSpec, Stacking};
    /// let mut effects = EffectSet::<1>::new().unwrap();
    /// let mut slow = EffectSpec::modifier(1, 0, 0.5, 1.0);
    /// slow.stacking = Stacking::Stack(3);
    /// effects.apply(slow, 0).unwrap();
    /// effects.advance(0.5).unwrap();
    /// assert_eq!(effects.refresh_duration(1, 2.0), Ok(true));
    /// let status = effects.status(1).unwrap();
    /// assert_eq!(status.remaining_seconds, 2.0);
    /// assert_eq!(status.stacks, 1);
    /// assert_eq!(status.spec, slow);
    /// assert_eq!(effects.refresh_duration(1, f32::NAN), Err(AttributeError::Invalid));
    /// assert_eq!(effects.status(1), Some(status));
    /// assert_eq!(effects.refresh_duration(2, 1.0), Ok(false));
    /// ```
    pub fn refresh_duration(&mut self, key: u32, seconds: f32) -> Result<bool, AttributeError> {
        if key == 0 || !valid_duration(seconds) {
            return Err(AttributeError::Invalid);
        }
        let Some(active) = self
            .effects
            .iter_mut()
            .flatten()
            .find(|active| active.spec.key == key)
        else {
            return Ok(false);
        };
        active.remaining = active.remaining.max(f64::from(seconds));
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
    /// Removes all stacks for `key` and returns their final status in one
    /// operation. Missing, expired and zero keys return `None` without changes.
    /// Other effects retain their slots and state; removal emits no pulse.
    /// Reapplying a removed key starts at one stack with a fresh pulse phase.
    ///
    /// ```
    /// use ferravex_play::attributes::{EffectSet, EffectSpec};
    /// let mut effects = EffectSet::<1>::new().unwrap();
    /// effects.apply(EffectSpec::modifier(7, 0, 0.5, 2.0), 0).unwrap();
    /// effects.advance(0.5).unwrap();
    /// let removed = effects.take(7).unwrap();
    /// assert_eq!(removed.remaining_seconds, 1.5);
    /// assert_eq!(removed.periodic_progress(), None);
    /// assert!(effects.is_empty());
    /// assert_eq!(effects.take(7), None);
    /// ```
    pub fn take(&mut self, key: u32) -> Option<EffectStatus> {
        self.effects
            .iter_mut()
            .find(|e| e.is_some_and(|s| s.spec.key == key))
            .and_then(Option::take)
            .map(|active| active.status())
    }

    /// Removes all stacks for `key`, returning whether it was present.
    /// Use [`Self::take`] to also inspect the removed status.
    pub fn remove(&mut self, key: u32) -> bool {
        self.take(key).is_some()
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

    fn burn(key: u32, stacking: Stacking) -> EffectSpec {
        EffectSpec {
            tags: 2,
            stacking,
            periodic: Some(Periodic {
                interval: 0.25,
                amount: 2.,
            }),
            ..EffectSpec::modifier(key, 0, 0.5, 2.)
        }
    }

    #[test]
    fn status_views_report_stacks_and_periodic_progress_without_consuming_pulses() {
        let mut effects = EffectSet::<2>::new().unwrap();
        let spec = burn(1, Stacking::Stack(3));
        effects.apply(spec, 0).unwrap();
        effects.apply(spec, 0).unwrap();
        effects
            .apply(EffectSpec::modifier(2, 1, 1., 2.), 0)
            .unwrap();
        let initial = effects.status(1).unwrap();
        assert_eq!(initial.spec, spec);
        assert_eq!(initial.remaining_seconds, 2.);
        assert_eq!(initial.stacks, 2);
        assert_eq!(initial.periodic_elapsed_seconds, Some(0.));
        assert_eq!(initial.periodic_progress(), Some(0.));
        assert_eq!(initial.seconds_until_pulse(), Some(0.25));
        let plain = effects.status(2).unwrap();
        assert_eq!(plain.periodic_elapsed_seconds, None);
        assert_eq!(plain.periodic_progress(), None);
        assert_eq!(plain.seconds_until_pulse(), None);

        let pulses = effects.advance(0.625).unwrap();
        assert_eq!(pulses[0].unwrap().count, 2);
        assert_eq!(pulses[0].unwrap().amount, 8.);
        assert!(pulses[1].is_none());
        let halfway = effects.status(1).unwrap();
        assert_eq!(halfway.remaining_seconds, 1.375);
        assert_eq!(halfway.periodic_elapsed_seconds, Some(0.125));
        assert_eq!(halfway.periodic_progress(), Some(0.5));
        assert_eq!(halfway.seconds_until_pulse(), Some(0.125));
        for _ in 0..4 {
            assert_eq!(effects.iter().next(), Some(halfway));
            assert_eq!(effects.status(1), Some(halfway));
        }
        assert!(effects.advance(0.).unwrap().iter().all(Option::is_none));
        assert_eq!(effects.status(1), Some(halfway));
        let pulse = effects.advance(0.125).unwrap()[0].unwrap();
        assert_eq!((pulse.count, pulse.amount), (1, 4.));
        assert_eq!(effects.status(1).unwrap().periodic_progress(), Some(0.));
        // A copied view remains the state it captured, even after advancement.
        assert_eq!(initial.remaining_seconds, 2.);
        assert_eq!(halfway.periodic_progress(), Some(0.5));
        let pulse = effects.advance(60.).unwrap()[0].unwrap();
        assert_eq!((pulse.count, pulse.amount), (5, 20.));
        assert!(effects.is_empty());
        assert_eq!(effects.status(1), None);
        assert_eq!(effects.iter().next(), None);
    }

    #[test]
    fn iteration_is_bounded_and_preserves_slots_across_removal_and_reuse() {
        assert!(matches!(
            EffectSet::<0>::new(),
            Err(AttributeError::Capacity)
        ));
        assert!(matches!(
            EffectSet::<33>::new(),
            Err(AttributeError::Capacity)
        ));
        let mut effects = EffectSet::<32>::new().unwrap();
        assert!(effects.is_empty());
        assert_eq!(effects.len(), 0);
        assert_eq!(effects.iter().next_back(), None);
        assert_eq!(effects.status(0), None);
        for key in 1..=32 {
            effects
                .apply(EffectSpec::modifier(key, 0, 1., 1.), 0)
                .unwrap();
        }
        assert_eq!(effects.len(), 32);
        assert!(!effects.is_empty());
        assert_eq!(effects.iter().count(), 32);
        assert_eq!(effects.status(33), None);
        assert_eq!(
            effects.apply(EffectSpec::modifier(33, 0, 1., 1.), 0),
            Err(AttributeError::Capacity)
        );
        assert_eq!(effects.take(2).unwrap().spec.key, 2);
        assert_eq!(effects.len(), 31);
        effects
            .apply(EffectSpec::modifier(33, 0, 1., 1.), 0)
            .unwrap();
        effects.refresh_duration(1, 2.).unwrap();
        effects
            .apply(EffectSpec::modifier(3, 0, 0.5, 2.), 0)
            .unwrap();
        {
            let mut iter = effects.iter();
            assert_eq!(iter.next().unwrap().spec.key, 1);
            assert_eq!(iter.next_back().unwrap().spec.key, 32);
            assert_eq!(iter.next().unwrap().spec.key, 33);
            for key in 3..32 {
                assert_eq!(iter.next().unwrap().spec.key, key);
            }
            assert_eq!(iter.next(), None);
            assert_eq!(iter.next_back(), None);
            assert_eq!(iter.next(), None);
        }
        effects.clear();
        assert_eq!(effects.len(), 0);
        assert!(effects.is_empty());
        assert_eq!(effects.iter().next(), None);
        assert_eq!(effects.status(1), None);
    }

    #[test]
    fn duration_refresh_keeps_spec_stacks_phase_and_other_effects_at_capacity() {
        let mut effects = EffectSet::<2>::new().unwrap();
        let spec = burn(1, Stacking::Stack(3));
        effects.apply(spec, 0).unwrap();
        effects.apply(spec, 0).unwrap();
        effects.apply(burn(2, Stacking::Strongest), 0).unwrap();
        effects.advance(0.125).unwrap();
        let before = effects.status(1).unwrap();
        let other = effects.status(2).unwrap();
        assert_eq!(effects.refresh_duration(1, 3.), Ok(true));
        assert_eq!(
            effects.status(1),
            Some(EffectStatus {
                remaining_seconds: 3.,
                ..before
            })
        );
        assert_eq!(effects.status(2), Some(other));
        assert_eq!(effects.refresh_duration(1, 0.5), Ok(true));
        assert_eq!(effects.status(1).unwrap().remaining_seconds, 3.);
        assert_eq!(effects.value(0, 100., 0., 100.).unwrap(), 12.5);
        assert_eq!(effects.advance(0.125).unwrap()[0].unwrap().amount, 4.);
        assert_eq!(effects.refresh_duration(1, 3600.), Ok(true));
        assert_eq!(effects.remaining(1), 3600.);
        assert_eq!(effects.refresh_duration(1, 0.0001), Ok(true));
        assert_eq!(effects.remaining(1), 3600.);
    }

    #[test]
    fn invalid_or_missing_refreshes_and_invalid_advancement_leave_all_state_intact() {
        let mut effects = EffectSet::<2>::new().unwrap();
        effects.apply(burn(1, Stacking::Stack(3)), 0).unwrap();
        effects.apply(burn(2, Stacking::Refresh), 0).unwrap();
        effects.advance(0.125).unwrap();
        let before: Vec<_> = effects.iter().collect();
        for seconds in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -1.,
            0.,
            0.00001,
            3601.,
        ] {
            for key in [1, 3] {
                assert_eq!(
                    effects.refresh_duration(key, seconds),
                    Err(AttributeError::Invalid)
                );
                assert_eq!(effects.iter().collect::<Vec<_>>(), before);
            }
        }
        assert_eq!(
            effects.refresh_duration(0, 1.),
            Err(AttributeError::Invalid)
        );
        assert_eq!(effects.refresh_duration(3, 1.), Ok(false));
        assert_eq!(effects.iter().collect::<Vec<_>>(), before);
        for dt in [f32::NAN, f32::INFINITY, -0.01, 61.] {
            assert!(matches!(effects.advance(dt), Err(AttributeError::Invalid)));
            assert_eq!(effects.iter().collect::<Vec<_>>(), before);
        }
        effects.advance(2.).unwrap();
        assert_eq!(effects.refresh_duration(1, 1.), Ok(false));
        assert!(effects.is_empty());
    }

    #[test]
    fn reapplication_errors_and_immunity_are_transactional_in_status_views() {
        let mut effects = EffectSet::<1>::new().unwrap();
        let spec = burn(1, Stacking::Stack(3));
        effects.apply(spec, 0).unwrap();
        effects.advance(0.125).unwrap();
        let before = effects.status(1);
        for invalid in [
            EffectSpec {
                seconds: f32::NAN,
                ..spec
            },
            EffectSpec {
                additive: f32::INFINITY,
                ..spec
            },
            EffectSpec {
                periodic: Some(Periodic {
                    interval: 0.,
                    amount: 2.,
                }),
                ..spec
            },
        ] {
            assert_eq!(effects.apply(invalid, 0), Err(AttributeError::Invalid));
            assert_eq!(effects.status(1), before);
        }
        for incompatible in [
            EffectSpec {
                attribute: 1,
                ..spec
            },
            EffectSpec {
                stacking: Stacking::Stack(2),
                ..spec
            },
            EffectSpec {
                periodic: None,
                ..spec
            },
            EffectSpec {
                periodic: Some(Periodic {
                    interval: 0.5,
                    amount: 2.,
                }),
                ..spec
            },
        ] {
            assert_eq!(
                effects.apply(incompatible, 0),
                Err(AttributeError::Incompatible)
            );
            assert_eq!(effects.status(1), before);
        }
        assert_eq!(effects.apply(spec, 2), Ok(false));
        assert_eq!(effects.apply(EffectSpec { key: 2, ..spec }, 2), Ok(false));
        assert_eq!(
            effects.apply(EffectSpec { key: 2, ..spec }, 0),
            Err(AttributeError::Capacity)
        );
        assert_eq!(effects.status(1), before);

        let replacement = EffectSpec {
            seconds: 0.5,
            tags: 4,
            periodic: Some(Periodic {
                interval: 0.25,
                amount: 3.,
            }),
            ..spec
        };
        for _ in 0..5 {
            effects.apply(replacement, 0).unwrap();
        }
        let status = effects.status(1).unwrap();
        assert_eq!(status.spec, replacement);
        assert_eq!(status.remaining_seconds, before.unwrap().remaining_seconds);
        assert_eq!(status.periodic_elapsed_seconds, Some(0.125));
        assert_eq!(status.stacks, 3);
        assert_eq!(effects.tags(), 4);
        assert_eq!(effects.advance(0.125).unwrap()[0].unwrap().amount, 9.);
    }

    #[test]
    fn strongest_views_keep_weaker_refreshes_out_and_accept_equal_strength() {
        let mut effects = EffectSet::<1>::new().unwrap();
        let spec = burn(1, Stacking::Strongest);
        effects.apply(spec, 0).unwrap();
        effects.advance(0.125).unwrap();
        let before = effects.status(1).unwrap();
        let weaker = EffectSpec {
            strength: 0.1,
            seconds: 3.,
            ..spec
        };
        assert_eq!(effects.apply(weaker, 0), Ok(true));
        assert_eq!(effects.status(1), Some(before));
        let stronger = EffectSpec {
            strength: 0.75,
            multiplier: 0.25,
            seconds: 0.5,
            ..spec
        };
        effects.apply(stronger, 0).unwrap();
        assert_eq!(
            effects.status(1),
            Some(EffectStatus {
                spec: stronger,
                ..before
            })
        );
        let equal = EffectSpec {
            seconds: 3.,
            ..stronger
        };
        effects.apply(equal, 0).unwrap();
        assert_eq!(
            effects.status(1),
            Some(EffectStatus {
                spec: equal,
                remaining_seconds: 3.,
                ..before
            })
        );
    }

    #[test]
    fn take_returns_final_state_and_reapplication_starts_fresh() {
        let mut effects = EffectSet::<2>::new().unwrap();
        let spec = burn(1, Stacking::Stack(3));
        effects.apply(spec, 0).unwrap();
        effects.apply(spec, 0).unwrap();
        effects
            .apply(
                EffectSpec {
                    tags: 4,
                    ..burn(2, Stacking::Refresh)
                },
                0,
            )
            .unwrap();
        effects.advance(0.125).unwrap();
        let removed = effects.status(1).unwrap();
        let other = effects.status(2).unwrap();
        assert_eq!(effects.take(0), None);
        assert_eq!(effects.take(3), None);
        assert_eq!(effects.take(1), Some(removed));
        assert_eq!(effects.take(1), None);
        assert!(!effects.remove(1));
        assert_eq!(effects.status(1), None);
        assert_eq!(effects.status(2), Some(other));
        assert_eq!(effects.len(), 1);
        assert_eq!(effects.tags(), 4);
        assert_eq!(effects.value(0, 100., 0., 100.).unwrap(), 50.);
        effects.apply(spec, 0).unwrap();
        let fresh = effects.status(1).unwrap();
        assert_eq!(fresh.stacks, 1);
        assert_eq!(fresh.remaining_seconds, 2.);
        assert_eq!(fresh.periodic_elapsed_seconds, Some(0.));
        let pulses = effects.advance(0.125).unwrap();
        assert!(pulses[0].is_none());
        assert_eq!(pulses[1].unwrap().count, 1);
        assert!(effects.remove(1));
        assert!(effects.remove(2));
        assert!(effects.is_empty());
        assert!(effects.advance(1.).unwrap().iter().all(Option::is_none));
    }

    #[test]
    fn views_keep_f64_time_and_do_not_promise_pulses_past_expiry() {
        let mut effects = EffectSet::<1>::new().unwrap();
        effects.apply(burn(1, Stacking::Refresh), 0).unwrap();
        effects.advance(0.1).unwrap();
        let status = effects.status(1).unwrap();
        assert_eq!(status.remaining_seconds, 2. - f64::from(0.1f32));
        assert_eq!(status.periodic_elapsed_seconds, Some(f64::from(0.1f32)));
        assert_eq!(status.seconds_until_pulse(), Some(0.25 - f64::from(0.1f32)));
        effects.clear();
        effects
            .apply(
                EffectSpec {
                    seconds: 0.125,
                    ..burn(1, Stacking::Refresh)
                },
                0,
            )
            .unwrap();
        let status = effects.status(1).unwrap();
        assert!(status.seconds_until_pulse().unwrap() > status.remaining_seconds);
        assert!(effects.advance(1.).unwrap()[0].is_none());
        assert_eq!(effects.take(1), None);
        assert!(effects.is_empty());
    }

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
