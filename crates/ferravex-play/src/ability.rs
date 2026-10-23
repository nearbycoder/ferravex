//! Fixed-capacity ability charges, resource costs and cancellable reservations.
//! Hosts own conditions and actual effects; no ability creates a hit or movement.
//!
//! [`AbilityBank::availability`] provides a read-only HUD/AI snapshot. Previewing
//! never spends charges/resources, advances recharge, or creates a reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityError {
    Invalid,
    Capacity,
    Blocked,
    Empty,
    Resource,
    Stale,
}
/// The first condition preventing a new reservation or immediate activation.
///
/// Conditions are checked in declaration order. A snapshot still exposes charges,
/// costs and pending reservations when an earlier condition takes precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityBlocker {
    /// The host supplied `enabled = false`.
    Disabled,
    /// This slot already has an uncommitted, uncancelled reservation.
    Busy,
    /// No unreserved charges remain.
    NoCharges,
    /// The supplied resource pool cannot pay the cost.
    InsufficientResource,
    /// The bank has issued every possible reservation serial.
    ReservationExhausted,
}
/// A value snapshot returned by [`AbilityBank::availability`].
///
/// This is advisory information, not a reservation: changing the bank, resource
/// pool or host's enabled condition may change the next activation's result.
/// See [`AbilityBank::availability`] for a compiling HUD/AI usage example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbilityAvailability {
    /// Unreserved charges currently held by the slot.
    pub charges: u8,
    /// The slot's configured charge capacity.
    pub max_charges: u8,
    /// The same charge cooldown as [`AbilityBank::remaining`]: zero if any
    /// charge remains. This is not a readiness guarantee; other blockers apply.
    pub cooldown_remaining: f32,
    /// Remaining simulation seconds on the next-charge timer, even if another
    /// charge is available. Zero when full or when the first reservation has
    /// not yet been committed. The timer is frozen while `reserved` is true.
    pub recharge_remaining: f32,
    /// Resource amount required for a new reservation.
    pub resource_cost: f32,
    /// Current value of the supplied pool, after any previous reservations.
    pub resource_available: f32,
    /// Whether this slot has a pending reservation, regardless of `blocker`.
    pub reserved: bool,
    /// First blocker, or `None` when a new reservation can succeed.
    pub blocker: Option<AbilityBlocker>,
}
impl AbilityAvailability {
    /// Convert this snapshot's blocker to the existing activation error.
    ///
    /// Both host-disabled and busy states map to [`AbilityError::Blocked`].
    /// This only reads the snapshot; it never authorizes or performs an action.
    ///
    /// ```
    /// use ferravex_play::ability::{AbilityBank, AbilityError, AbilitySpec, ResourcePool};
    /// let bank = AbilityBank::<1>::new(1, &[AbilitySpec {
    ///     charges: 1, recharge_seconds: 0.5, cost: 3.0,
    /// }]).unwrap();
    /// let pool = ResourcePool::new(2.0, 5.0, 0.0).unwrap();
    /// let preview = bank.availability(0, &pool, true).unwrap();
    /// assert_eq!(preview.check(), Err(AbilityError::Resource));
    /// assert_eq!(pool.value(), 2.0);
    /// ```
    pub fn check(&self) -> Result<(), AbilityError> {
        match self.blocker {
            None => Ok(()),
            Some(AbilityBlocker::Disabled | AbilityBlocker::Busy) => Err(AbilityError::Blocked),
            Some(AbilityBlocker::NoCharges) => Err(AbilityError::Empty),
            Some(AbilityBlocker::InsufficientResource) => Err(AbilityError::Resource),
            Some(AbilityBlocker::ReservationExhausted) => Err(AbilityError::Capacity),
        }
    }
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
    /// Inspect whether a new reservation or immediate activation can succeed.
    ///
    /// Reads only immutable references and allocates no storage. Charges and pool
    /// value already exclude anything spent by pending reservations. Recharge is
    /// driven only by [`Self::advance`], not by previews or presentation time.
    ///
    /// A valid slot returns a snapshot even when blocked. An unknown slot returns
    /// [`AbilityError::Invalid`] when enabled, or [`AbilityError::Blocked`] when
    /// disabled, preserving [`Self::begin`]'s host-condition-first error order.
    /// For unchanged inputs, `availability(...).and_then(|a| a.check())` matches
    /// both `begin(...).map(|_| ())` and [`Self::activate`].
    ///
    /// ```
    /// use ferravex_play::ability::{AbilityBank, AbilityBlocker, AbilitySpec, ResourcePool};
    /// let mut bank = AbilityBank::<1>::new(1, &[AbilitySpec {
    ///     charges: 2, recharge_seconds: 0.5, cost: 3.0,
    /// }]).unwrap();
    /// let mut pool = ResourcePool::new(6.0, 6.0, 0.0).unwrap();
    /// let hud = bank.availability(0, &pool, true).unwrap();
    /// assert_eq!((hud.charges, hud.max_charges), (2, 2));
    /// assert_eq!((hud.resource_cost, hud.resource_available), (3.0, 6.0));
    /// assert_eq!(hud.blocker, None);
    /// assert!(!hud.reserved);
    /// assert_eq!(pool.value(), 6.0);
    ///
    /// let token = bank.begin(0, &mut pool, true).unwrap();
    /// let busy = bank.availability(0, &pool, true).unwrap();
    /// assert_eq!(busy.blocker, Some(AbilityBlocker::Busy));
    /// assert_eq!((busy.charges, busy.resource_available), (1, 3.0));
    /// bank.cancel(token, &mut pool).unwrap();
    /// assert_eq!(bank.availability(0, &pool, true).unwrap(), hud);
    /// ```
    pub fn availability(
        &self,
        id: usize,
        pool: &ResourcePool,
        enabled: bool,
    ) -> Result<AbilityAvailability, AbilityError> {
        let slot = self
            .slots
            .get(id)
            .and_then(Option::as_ref)
            .ok_or(if enabled {
                AbilityError::Invalid
            } else {
                AbilityError::Blocked
            })?;
        let reserved = slot.reservation.is_some();
        let blocker = if !enabled {
            Some(AbilityBlocker::Disabled)
        } else if reserved {
            Some(AbilityBlocker::Busy)
        } else if slot.charges == 0 {
            Some(AbilityBlocker::NoCharges)
        } else if pool.value < slot.spec.cost {
            Some(AbilityBlocker::InsufficientResource)
        } else if self.serial == u64::MAX {
            Some(AbilityBlocker::ReservationExhausted)
        } else {
            None
        };
        Ok(AbilityAvailability {
            charges: slot.charges,
            max_charges: slot.spec.charges,
            cooldown_remaining: if slot.charges > 0 {
                0.
            } else {
                slot.remaining as f32
            },
            recharge_remaining: slot.remaining as f32,
            resource_cost: slot.spec.cost,
            resource_available: pool.value,
            reserved,
            blocker,
        })
    }
    pub fn begin(
        &mut self,
        id: usize,
        pool: &mut ResourcePool,
        enabled: bool,
    ) -> Result<Reservation, AbilityError> {
        self.availability(id, pool, enabled)?.check()?;
        let slot = self
            .slots
            .get_mut(id)
            .and_then(Option::as_mut)
            .ok_or(AbilityError::Invalid)?;
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

    fn assert_blocked(
        bank: &mut AbilityBank<2>,
        pool: &mut ResourcePool,
        enabled: bool,
        blocker: AbilityBlocker,
        error: AbilityError,
    ) {
        let before = bank.availability(0, pool, enabled).unwrap();
        let serial = bank.serial;
        let reservation = bank.slots[0].unwrap().reservation;
        assert_eq!(before.blocker, Some(blocker));
        assert_eq!(before.check(), Err(error));
        assert_eq!(bank.begin(0, pool, enabled).map(|_| ()), Err(error));
        assert_eq!(bank.activate(0, pool, enabled), Err(error));
        assert_eq!(bank.availability(0, pool, enabled).unwrap(), before);
        assert_eq!(bank.serial, serial);
        assert_eq!(bank.slots[0].unwrap().reservation, reservation);
    }

    #[test]
    fn availability_is_read_only_and_reservations_remain_single_use() {
        let mut b = bank();
        let mut p = ResourcePool::new(6., 6., 0.).unwrap();
        let ready = AbilityAvailability {
            charges: 2,
            max_charges: 2,
            cooldown_remaining: 0.,
            recharge_remaining: 0.,
            resource_cost: 3.,
            resource_available: 6.,
            reserved: false,
            blocker: None,
        };
        for _ in 0..100 {
            assert_eq!(b.availability(0, &p, true), Ok(ready));
        }
        assert_eq!(ready.check(), Ok(()));
        assert_eq!(b.serial, 0);
        assert_eq!(p.value(), 6.);
        let token = b.begin(0, &mut p, true).unwrap();
        assert_eq!(token.serial, 1);
        let busy = b.availability(0, &p, true).unwrap();
        assert_eq!((busy.charges, busy.resource_available), (1, 3.));
        assert!(busy.reserved);
        assert_eq!(busy.recharge_remaining, 0.);
        assert_blocked(
            &mut b,
            &mut p,
            true,
            AbilityBlocker::Busy,
            AbilityError::Blocked,
        );
        b.advance(5.).unwrap();
        assert_eq!(b.availability(0, &p, true), Ok(busy));
        b.cancel(token, &mut p).unwrap();
        assert_eq!(b.availability(0, &p, true), Ok(ready));
        assert_eq!(b.commit(token), Err(AbilityError::Stale));
        assert_eq!(b.cancel(token, &mut p), Err(AbilityError::Stale));
        b.activate(0, &mut p, true).unwrap();
        assert_eq!(b.charges(0), Some(1));
        assert_eq!(p.value(), 3.);
    }

    #[test]
    fn availability_explains_blocker_precedence_and_recovery() {
        let mut b = bank();
        let mut p = ResourcePool::new(6., 6., 0.).unwrap();
        b.activate(0, &mut p, true).unwrap();
        b.advance(0.125).unwrap();
        let partial = b.availability(0, &p, true).unwrap();
        assert_eq!(partial.check(), Ok(()));
        assert_eq!(partial.cooldown_remaining, 0.);
        assert_eq!(partial.recharge_remaining, 0.375);
        assert_eq!(b.remaining(0), Some(partial.cooldown_remaining));

        let token = b.begin(0, &mut p, true).unwrap();
        assert_blocked(
            &mut b,
            &mut p,
            false,
            AbilityBlocker::Disabled,
            AbilityError::Blocked,
        );
        let disabled = b.availability(0, &p, false).unwrap();
        assert!(disabled.reserved);
        assert_eq!(disabled.charges, 0);
        assert_eq!(disabled.resource_available, 0.);
        assert_eq!(disabled.cooldown_remaining, 0.375);
        assert_blocked(
            &mut b,
            &mut p,
            true,
            AbilityBlocker::Busy,
            AbilityError::Blocked,
        );
        b.advance(60.).unwrap();
        assert_eq!(b.availability(0, &p, false), Ok(disabled));
        b.commit(token).unwrap();
        assert_blocked(
            &mut b,
            &mut p,
            true,
            AbilityBlocker::NoCharges,
            AbilityError::Empty,
        );
        b.advance(0.25).unwrap();
        let cooling = b.availability(0, &p, true).unwrap();
        assert_eq!(cooling.cooldown_remaining, 0.125);
        assert_eq!(b.remaining(0), Some(cooling.cooldown_remaining));
        b.advance(0.125).unwrap();
        assert_blocked(
            &mut b,
            &mut p,
            true,
            AbilityBlocker::InsufficientResource,
            AbilityError::Resource,
        );
        p.credit(3.).unwrap();
        let ready = b.availability(0, &p, true).unwrap();
        assert_eq!(ready.check(), Ok(()));
        assert_eq!(ready.charges, 1);
        assert_eq!(ready.recharge_remaining, 0.5);
        b.activate(0, &mut p, true).unwrap();
        b.advance(1.).unwrap();
        let full = b.availability(0, &p, true).unwrap();
        assert_eq!(full.charges, full.max_charges);
        assert_eq!(full.recharge_remaining, 0.);
        assert_eq!(full.cooldown_remaining, 0.);
    }

    #[test]
    fn availability_unknown_ids_preserve_disabled_first_errors() {
        let mut b = bank();
        let mut p = ResourcePool::new(6., 6., 0.).unwrap();
        let before = b.availability(0, &p, true).unwrap();
        for id in [1, 2, usize::MAX] {
            for enabled in [false, true] {
                let error = if enabled {
                    AbilityError::Invalid
                } else {
                    AbilityError::Blocked
                };
                assert_eq!(b.availability(id, &p, enabled), Err(error));
                assert_eq!(b.begin(id, &mut p, enabled).map(|_| ()), Err(error));
                assert_eq!(b.activate(id, &mut p, enabled), Err(error));
            }
        }
        assert_eq!(b.availability(0, &p, true), Ok(before));
        assert_eq!(b.serial, 0);
    }

    #[test]
    fn availability_checks_resource_boundaries_and_free_abilities() {
        for cost in [0., 3., 1e9] {
            let mut b = AbilityBank::<2>::new(
                1,
                &[AbilitySpec {
                    charges: 1,
                    recharge_seconds: 0.5,
                    cost,
                }],
            )
            .unwrap();
            let mut p = ResourcePool::new(cost, cost, 0.).unwrap();
            let ready = b.availability(0, &p, true).unwrap();
            assert_eq!(ready.check(), Ok(()));
            assert_eq!(
                (ready.resource_available, ready.resource_cost),
                (cost, cost)
            );
            let token = b.begin(0, &mut p, true).unwrap();
            assert_eq!(p.value(), 0.);
            assert_blocked(
                &mut b,
                &mut p,
                true,
                AbilityBlocker::Busy,
                AbilityError::Blocked,
            );
            b.cancel(token, &mut p).unwrap();
            assert_eq!(b.availability(0, &p, true), Ok(ready));
            b.activate(0, &mut p, true).unwrap();
            assert_eq!(p.value(), 0.);
            let empty = b.availability(0, &p, true).unwrap();
            assert_eq!(empty.blocker, Some(AbilityBlocker::NoCharges));
            assert_eq!(empty.cooldown_remaining, 0.5);
        }

        let mut b = bank();
        let mut p = ResourcePool::new(f32::from_bits(3.0_f32.to_bits() - 1), 3., 1.).unwrap();
        assert_blocked(
            &mut b,
            &mut p,
            true,
            AbilityBlocker::InsufficientResource,
            AbilityError::Resource,
        );
        p.advance(1.).unwrap();
        assert_eq!(b.availability(0, &p, true).unwrap().check(), Ok(()));
        b.activate(0, &mut p, true).unwrap();
        assert_eq!(p.value(), 0.);
    }

    #[test]
    fn availability_predicts_serial_exhaustion_without_issuing_tokens() {
        let mut b = bank();
        let mut p = ResourcePool::new(6., 6., 0.).unwrap();
        b.serial = u64::MAX - 1;
        for _ in 0..100 {
            assert_eq!(b.availability(0, &p, true).unwrap().check(), Ok(()));
        }
        assert_eq!(b.serial, u64::MAX - 1);
        let token = b.begin(0, &mut p, true).unwrap();
        assert_eq!(token.serial, u64::MAX);
        assert_blocked(
            &mut b,
            &mut p,
            true,
            AbilityBlocker::Busy,
            AbilityError::Blocked,
        );
        b.cancel(token, &mut p).unwrap();
        assert_blocked(
            &mut b,
            &mut p,
            false,
            AbilityBlocker::Disabled,
            AbilityError::Blocked,
        );
        assert_blocked(
            &mut b,
            &mut p,
            true,
            AbilityBlocker::ReservationExhausted,
            AbilityError::Capacity,
        );
        let mut empty_pool = ResourcePool::new(0., 0., 0.).unwrap();
        assert_blocked(
            &mut b,
            &mut empty_pool,
            true,
            AbilityBlocker::InsufficientResource,
            AbilityError::Resource,
        );
    }
}
