//! Fixed-capacity ability charges, resource costs and cancellable reservations.
//! Hosts own conditions and actual effects; no ability creates a hit or movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityError {
    Invalid,
    Capacity,
    Blocked,
    Empty,
    Resource,
    Stale,
}
#[derive(Clone, Copy, Debug)]
pub struct AbilitySpec {
    pub charges: u8,
    pub recharge_seconds: f32,
    pub cost: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct ResourcePool {
    value: f32,
    maximum: f32,
    regeneration: f32,
}
impl ResourcePool {
    pub fn new(value: f32, maximum: f32, regeneration: f32) -> Result<Self, AbilityError> {
        if ![value, maximum, regeneration]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.)
            || value > maximum
            || maximum > 1e9
            || regeneration > 1e9
        {
            return Err(AbilityError::Invalid);
        }
        Ok(Self {
            value,
            maximum,
            regeneration,
        })
    }
    pub fn value(&self) -> f32 {
        self.value
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), AbilityError> {
        valid_dt(dt)?;
        self.value = (self.value + self.regeneration * dt).min(self.maximum);
        Ok(())
    }
    pub fn credit(&mut self, amount: f32) -> Result<(), AbilityError> {
        if !amount.is_finite() || amount < 0. {
            return Err(AbilityError::Invalid);
        }
        self.value = (self.value + amount).min(self.maximum);
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
struct Slot {
    spec: AbilitySpec,
    charges: u8,
    remaining: f64,
    reservation: Option<u64>,
}
/// Opaque, single-use token. Owners must be host-unique generational actor IDs.
#[derive(Clone, Copy, Debug)]
pub struct Reservation {
    owner: u64,
    slot: usize,
    serial: u64,
}
pub struct AbilityBank<const N: usize> {
    owner: u64,
    slots: [Option<Slot>; N],
    serial: u64,
}
impl<const N: usize> AbilityBank<N> {
    pub fn new(owner: u64, specs: &[AbilitySpec]) -> Result<Self, AbilityError> {
        if owner == 0 || N == 0 || N > 64 || specs.is_empty() || specs.len() > N {
            return Err(AbilityError::Capacity);
        }
        let mut slots = [None; N];
        for (out, spec) in slots.iter_mut().zip(specs) {
            if spec.charges == 0
                || !spec.recharge_seconds.is_finite()
                || !(0.0001..=3600.).contains(&spec.recharge_seconds)
                || !spec.cost.is_finite()
                || !(0. ..=1e9).contains(&spec.cost)
            {
                return Err(AbilityError::Invalid);
            }
            *out = Some(Slot {
                spec: *spec,
                charges: spec.charges,
                remaining: 0.,
                reservation: None,
            });
        }
        Ok(Self {
            owner,
            slots,
            serial: 0,
        })
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), AbilityError> {
        valid_dt(dt)?;
        for slot in self.slots.iter_mut().flatten() {
            if slot.reservation.is_some() || slot.charges == slot.spec.charges {
                continue;
            }
            slot.remaining -= f64::from(dt);
            if slot.remaining <= 1e-9 {
                let period = f64::from(slot.spec.recharge_seconds);
                let restored = (1. + (-slot.remaining).max(0.) / period)
                    .floor()
                    .min(f64::from(slot.spec.charges - slot.charges))
                    as u8;
                slot.charges += restored;
                slot.remaining = if slot.charges == slot.spec.charges {
                    0.
                } else {
                    slot.remaining + f64::from(restored) * period
                };
            }
        }
        Ok(())
    }
    pub fn charges(&self, id: usize) -> Option<u8> {
        self.slots.get(id)?.map(|s| s.charges)
    }
    pub fn remaining(&self, id: usize) -> Option<f32> {
        self.slots.get(id)?.map(|s| {
            if s.charges > 0 {
                0.
            } else {
                s.remaining as f32
            }
        })
    }
    pub fn begin(
        &mut self,
        id: usize,
        pool: &mut ResourcePool,
        enabled: bool,
    ) -> Result<Reservation, AbilityError> {
        if !enabled {
            return Err(AbilityError::Blocked);
        }
        let slot = self
            .slots
            .get_mut(id)
            .and_then(Option::as_mut)
            .ok_or(AbilityError::Invalid)?;
        if slot.reservation.is_some() {
            return Err(AbilityError::Blocked);
        }
        if slot.charges == 0 {
            return Err(AbilityError::Empty);
        }
        if pool.value < slot.spec.cost {
            return Err(AbilityError::Resource);
        }
        let serial = self.serial.checked_add(1).ok_or(AbilityError::Capacity)?;
        self.serial = serial;
        slot.charges -= 1;
        pool.value -= slot.spec.cost;
        slot.reservation = Some(serial);
        Ok(Reservation {
            owner: self.owner,
            slot: id,
            serial,
        })
    }
    pub fn commit(&mut self, token: Reservation) -> Result<(), AbilityError> {
        let slot = self.pending(token)?;
        slot.reservation = None;
        if slot.remaining == 0. {
            slot.remaining = f64::from(slot.spec.recharge_seconds)
        }
        Ok(())
    }
    /// Refund only to the same resource pool used by begin. The host owns the pool;
    /// do not move reservations across actors, lives, banks or resource accounts.
    pub fn cancel(
        &mut self,
        token: Reservation,
        pool: &mut ResourcePool,
    ) -> Result<(), AbilityError> {
        let slot = self.pending(token)?;
        slot.reservation = None;
        slot.charges += 1;
        if slot.charges == slot.spec.charges {
            slot.remaining = 0.
        }
        pool.credit(slot.spec.cost)?;
        Ok(())
    }
    fn pending(&mut self, token: Reservation) -> Result<&mut Slot, AbilityError> {
        if token.owner != self.owner {
            return Err(AbilityError::Stale);
        }
        self.slots
            .get_mut(token.slot)
            .and_then(Option::as_mut)
            .filter(|s| s.reservation == Some(token.serial))
            .ok_or(AbilityError::Stale)
    }
    pub fn activate(
        &mut self,
        id: usize,
        pool: &mut ResourcePool,
        enabled: bool,
    ) -> Result<(), AbilityError> {
        let token = self.begin(id, pool, enabled)?;
        self.commit(token)
    }
}
fn valid_dt(dt: f32) -> Result<(), AbilityError> {
    if dt.is_finite() && (0. ..=60.).contains(&dt) {
        Ok(())
    } else {
        Err(AbilityError::Invalid)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn bank() -> AbilityBank<2> {
        AbilityBank::new(
            1,
            &[AbilitySpec {
                charges: 2,
                recharge_seconds: 0.5,
                cost: 3.,
            }],
        )
        .unwrap()
    }
    #[test]
    fn atomic_activation_recharge_and_failure_do_not_spend() {
        let mut b = bank();
        let mut p = ResourcePool::new(10., 10., 1.).unwrap();
        assert_eq!(b.activate(0, &mut p, false), Err(AbilityError::Blocked));
        assert_eq!(p.value(), 10.);
        b.activate(0, &mut p, true).unwrap();
        b.activate(0, &mut p, true).unwrap();
        assert_eq!(p.value(), 4.);
        assert_eq!(b.charges(0), Some(0));
        assert_eq!(b.activate(0, &mut p, true), Err(AbilityError::Empty));
        b.advance(0.75).unwrap();
        assert_eq!(b.charges(0), Some(1));
        b.advance(0.25).unwrap();
        assert_eq!(b.charges(0), Some(2));
        p.advance(1.).unwrap();
        assert_eq!(p.value(), 5.);
    }
    #[test]
    fn reservation_refunds_once_stale_and_foreign_tokens_fail() {
        let mut b = bank();
        let mut p = ResourcePool::new(5., 5., 0.).unwrap();
        let t = b.begin(0, &mut p, true).unwrap();
        assert_eq!(p.value(), 2.);
        b.advance(2.).unwrap();
        assert_eq!(b.charges(0), Some(1));
        b.cancel(t, &mut p).unwrap();
        assert_eq!(p.value(), 5.);
        assert_eq!(b.cancel(t, &mut p), Err(AbilityError::Stale));
        let mut foreign = AbilityBank::<2>::new(
            2,
            &[AbilitySpec {
                charges: 2,
                recharge_seconds: 0.5,
                cost: 3.,
            }],
        )
        .unwrap();
        assert_eq!(foreign.commit(t), Err(AbilityError::Stale));
        assert!(b.advance(f32::NAN).is_err());
    }
    #[test]
    fn charge_clock_is_partition_independent() {
        let mut a = bank();
        let mut b = bank();
        let mut p = ResourcePool::new(10., 10., 0.).unwrap();
        let mut q = p;
        a.activate(0, &mut p, true).unwrap();
        b.activate(0, &mut q, true).unwrap();
        a.advance(1.).unwrap();
        for _ in 0..100 {
            b.advance(0.01).unwrap();
        }
        assert_eq!(a.charges(0), b.charges(0));
    }
}
