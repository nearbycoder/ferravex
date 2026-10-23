//! Fixed-capacity inventories with one bounded stack per item and atomic edits.
//!
//! [`Inventory::exchange`] trades existing items between two inventories, while
//! [`Inventory::transfer_to`] moves them in one direction. Their `check_` methods
//! perform the same preflight without changing either inventory. Recipe costs and
//! newly created outputs remain the responsibility of [`super::crafting::craft`].
use super::LoopError;

/// Maximum number of outgoing entries per side of an exchange, including repeats.
pub const MAX_EXCHANGE_ITEMS: usize = 32;

/// Inventory to which an [`ExchangeError`] applies, relative to the method call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExchangeSide {
    /// The inventory on which the method was called (the source of a transfer).
    This,
    /// The other inventory (the destination of a transfer).
    Other,
}

/// First rejected part of an exchange. Neither inventory changes on an error.
///
/// Missing quantities are totals for an item across the side's outgoing list.
/// Capacity and limit errors describe the recipient after all outgoing items
/// have been removed. `requested` in a capacity error is the incoming quantity;
/// `available` is remaining room in that stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExchangeError {
    /// The outgoing list exceeds [`MAX_EXCHANGE_ITEMS`].
    TooManyItems { side: ExchangeSide, count: usize },
    /// An outgoing entry requests zero items.
    ZeroAmount { side: ExchangeSide, item: u32 },
    /// The original inventory cannot supply the total requested quantity.
    MissingItem {
        side: ExchangeSide,
        item: u32,
        requested: u64,
        available: u32,
    },
    /// The recipient has no empty slot for this new item.
    NoFreeSlot { side: ExchangeSide, item: u32 },
    /// The recipient's existing stack cannot hold all incoming items.
    StackCapacity {
        side: ExchangeSide,
        item: u32,
        requested: u32,
        available: u32,
    },
    /// Source and recipient stacks have different limits for the same item.
    LimitMismatch {
        side: ExchangeSide,
        item: u32,
        expected: u32,
        actual: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: u32,
    pub amount: u32,
    pub limit: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct Inventory<const N: usize> {
    slots: [Option<Stack>; N],
}
impl<const N: usize> Inventory<N> {
    pub fn new() -> Result<Self, LoopError> {
        if N == 0 || N > 256 {
            return Err(LoopError::Invalid);
        }
        Ok(Self { slots: [None; N] })
    }
    pub fn stacks(&self) -> impl Iterator<Item = Stack> + '_ {
        self.slots.iter().flatten().copied()
    }
    pub fn count(&self, item: u32) -> u32 {
        self.stacks()
            .find(|s| s.item == item)
            .map_or(0, |s| s.amount)
    }
    pub fn add(&mut self, stack: Stack) -> Result<(), LoopError> {
        if stack.amount == 0 || stack.limit == 0 || stack.amount > stack.limit {
            return Err(LoopError::Invalid);
        }
        if let Some(slot) = self
            .slots
            .iter_mut()
            .flatten()
            .find(|s| s.item == stack.item)
        {
            if slot.limit != stack.limit {
                return Err(LoopError::Invalid);
            }
            let amount = slot
                .amount
                .checked_add(stack.amount)
                .filter(|n| *n <= slot.limit)
                .ok_or(LoopError::Capacity)?;
            slot.amount = amount;
        } else {
            let slot = self
                .slots
                .iter_mut()
                .find(|s| s.is_none())
                .ok_or(LoopError::Capacity)?;
            *slot = Some(stack);
        }
        Ok(())
    }
    pub fn remove(&mut self, item: u32, amount: u32) -> Result<(), LoopError> {
        if amount == 0 {
            return Err(LoopError::Invalid);
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|s| s.is_some_and(|s| s.item == item))
            .ok_or(LoopError::Unavailable)?;
        let mut stack = slot.expect("occupied");
        stack.amount = stack
            .amount
            .checked_sub(amount)
            .ok_or(LoopError::Unavailable)?;
        *slot = (stack.amount > 0).then_some(stack);
        Ok(())
    }

    /// Preflight an exchange without changing either inventory.
    ///
    /// `give` contains `(item, amount)` entries from `self`; `take` contains
    /// entries from `other`. Each list may contain at most [`MAX_EXCHANGE_ITEMS`]
    /// entries. Repeated items are summed in `u64` before checking availability,
    /// and zero amounts are rejected. Empty lists are valid, including a no-op
    /// exchange. Incoming items cannot fund outgoing requirements.
    ///
    /// Both inventories release outgoing items before receiving anything, so
    /// full inventories can swap stacks. Each incoming stack retains its source
    /// limit and must match any remaining stack of that item in the recipient.
    /// A fully removed stack imposes no limit on its replacement, just as with
    /// [`Self::remove`] followed by [`Self::add`].
    ///
    /// Checks use bounded local copies with no heap allocation. List lengths are
    /// checked first, then outgoing requirements for `This` and `Other`, then
    /// incoming capacity for `This` and `Other`. Within each list, zero amounts
    /// are checked before availability; distinct items follow first occurrence
    /// order. A later [`Self::exchange`] rechecks the current state.
    ///
    /// ```
    /// use ferravex_play::gameplay::inventory::{ExchangeError, ExchangeSide, Inventory};
    /// let bag = Inventory::<1>::new().unwrap();
    /// let chest = Inventory::<2>::new().unwrap();
    /// assert_eq!(bag.check_exchange(&chest, &[(7, 2), (7, 3)], &[]),
    ///     Err(ExchangeError::MissingItem {
    ///         side: ExchangeSide::This, item: 7, requested: 5, available: 0,
    ///     }));
    /// ```
    pub fn check_exchange<const M: usize>(
        &self,
        other: &Inventory<M>,
        give: &[(u32, u32)],
        take: &[(u32, u32)],
    ) -> Result<(), ExchangeError> {
        self.prepare_exchange(other, give, take).map(|_| ())
    }

    /// Atomically trade existing items, using [`Self::check_exchange`]'s rules.
    ///
    /// Success conserves each item's combined total. Any error preserves both
    /// inventories exactly, including slot order and stack limits. Transactions
    /// are synchronous; hosts remain responsible for authorizing the trade.
    ///
    /// ```
    /// use ferravex_play::gameplay::inventory::{Inventory, Stack};
    /// let mut bag = Inventory::<1>::new().unwrap();
    /// let mut trader = Inventory::<1>::new().unwrap();
    /// bag.add(Stack { item: 1, amount: 3, limit: 10 }).unwrap();
    /// trader.add(Stack { item: 2, amount: 1, limit: 5 }).unwrap();
    /// bag.exchange(&mut trader, &[(1, 3)], &[(2, 1)]).unwrap();
    /// assert_eq!((bag.count(1), bag.count(2)), (0, 1));
    /// assert_eq!((trader.count(1), trader.count(2)), (3, 0));
    /// ```
    pub fn exchange<const M: usize>(
        &mut self,
        other: &mut Inventory<M>,
        give: &[(u32, u32)],
        take: &[(u32, u32)],
    ) -> Result<(), ExchangeError> {
        let (next, other_next) = self.prepare_exchange(other, give, take)?;
        *self = next;
        *other = other_next;
        Ok(())
    }

    /// Preflight a one-way transfer; equivalent to `check_exchange` with no take.
    ///
    /// ```
    /// use ferravex_play::gameplay::inventory::{Inventory, Stack};
    /// let mut bag = Inventory::<1>::new().unwrap();
    /// let chest = Inventory::<2>::new().unwrap();
    /// bag.add(Stack { item: 7, amount: 3, limit: 5 }).unwrap();
    /// bag.check_transfer_to(&chest, &[(7, 2)]).unwrap();
    /// assert_eq!((bag.count(7), chest.count(7)), (3, 0));
    /// ```
    pub fn check_transfer_to<const M: usize>(
        &self,
        destination: &Inventory<M>,
        items: &[(u32, u32)],
    ) -> Result<(), ExchangeError> {
        self.check_exchange(destination, items, &[])
    }

    /// Atomically move a batch of items, retaining their original stack limits.
    ///
    /// Equivalent to [`Self::exchange`] with an empty `take` list. On failure,
    /// neither inventory changes, even if earlier entries would have fitted.
    ///
    /// ```
    /// use ferravex_play::gameplay::inventory::{Inventory, Stack};
    /// let mut bag = Inventory::<2>::new().unwrap();
    /// let mut chest = Inventory::<1>::new().unwrap();
    /// bag.add(Stack { item: 7, amount: 3, limit: 5 }).unwrap();
    /// bag.transfer_to(&mut chest, &[(7, 1), (7, 1)]).unwrap();
    /// assert_eq!((bag.count(7), chest.count(7)), (1, 2));
    /// assert_eq!(chest.stacks().next().unwrap().limit, 5);
    /// ```
    pub fn transfer_to<const M: usize>(
        &mut self,
        destination: &mut Inventory<M>,
        items: &[(u32, u32)],
    ) -> Result<(), ExchangeError> {
        self.exchange(destination, items, &[])
    }

    fn prepare_exchange<const M: usize>(
        &self,
        other: &Inventory<M>,
        give: &[(u32, u32)],
        take: &[(u32, u32)],
    ) -> Result<(Self, Inventory<M>), ExchangeError> {
        for (items, side) in [(give, ExchangeSide::This), (take, ExchangeSide::Other)] {
            if items.len() > MAX_EXCHANGE_ITEMS {
                return Err(ExchangeError::TooManyItems {
                    side,
                    count: items.len(),
                });
            }
        }
        let mut next = *self;
        let mut other_next = *other;
        let given = next.withdraw(give, ExchangeSide::This)?;
        let taken = other_next.withdraw(take, ExchangeSide::Other)?;
        for stack in taken.into_iter().flatten() {
            next.receive(stack, ExchangeSide::This)?;
        }
        for stack in given.into_iter().flatten() {
            other_next.receive(stack, ExchangeSide::Other)?;
        }
        Ok((next, other_next))
    }

    // Called only after both list lengths are bounded by MAX_EXCHANGE_ITEMS.
    fn withdraw(
        &mut self,
        items: &[(u32, u32)],
        side: ExchangeSide,
    ) -> Result<[Option<Stack>; MAX_EXCHANGE_ITEMS], ExchangeError> {
        for &(item, amount) in items {
            if amount == 0 {
                return Err(ExchangeError::ZeroAmount { side, item });
            }
        }
        let mut outgoing = [None; MAX_EXCHANGE_ITEMS];
        for (index, &(item, _)) in items.iter().enumerate() {
            if items[..index].iter().any(|&(earlier, _)| earlier == item) {
                continue;
            }
            // At most 32 u32 quantities: the exact sum always fits in u64.
            let requested: u64 = items[index..]
                .iter()
                .filter(|&&(id, _)| id == item)
                .map(|&(_, amount)| u64::from(amount))
                .sum();
            let available = self.count(item);
            let missing = ExchangeError::MissingItem {
                side,
                item,
                requested,
                available,
            };
            if requested > u64::from(available) {
                return Err(missing);
            }
            let stack = self.stacks().find(|s| s.item == item).ok_or(missing)?;
            let amount = requested as u32; // Bounded by the available u32 quantity.
            self.remove(item, amount).map_err(|_| missing)?;
            outgoing[index] = Some(Stack { amount, ..stack });
        }
        Ok(outgoing)
    }

    fn receive(&mut self, stack: Stack, side: ExchangeSide) -> Result<(), ExchangeError> {
        let existing = self.stacks().find(|s| s.item == stack.item);
        // Withdrawn stacks are valid; add can fail only on a limit conflict or
        // capacity. Retain add's rules and enrich its errors with recipient data.
        self.add(stack).map_err(|_| match existing {
            Some(existing) if existing.limit != stack.limit => ExchangeError::LimitMismatch {
                side,
                item: stack.item,
                expected: existing.limit,
                actual: stack.limit,
            },
            Some(existing) => ExchangeError::StackCapacity {
                side,
                item: stack.item,
                requested: stack.amount,
                available: existing.limit - existing.amount,
            },
            None => ExchangeError::NoFreeSlot {
                side,
                item: stack.item,
            },
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: u32, amount: u32, limit: u32) -> Stack {
        Stack {
            item,
            amount,
            limit,
        }
    }

    fn inventory<const N: usize>(stacks: &[Stack]) -> Inventory<N> {
        let mut inventory = Inventory::new().unwrap();
        for &stack in stacks {
            inventory.add(stack).unwrap();
        }
        inventory
    }

    fn assert_rejected<const N: usize, const M: usize>(
        mut this: Inventory<N>,
        mut other: Inventory<M>,
        give: &[(u32, u32)],
        take: &[(u32, u32)],
        error: ExchangeError,
    ) {
        let before = (this.slots, other.slots);
        assert_eq!(this.check_exchange(&other, give, take), Err(error));
        assert_eq!((this.slots, other.slots), before);
        assert_eq!(this.exchange(&mut other, give, take), Err(error));
        assert_eq!((this.slots, other.slots), before);
    }

    #[test]
    fn edits_preserve_items_on_failed_capacity_or_spending() {
        let mut i = Inventory::<1>::new().unwrap();
        let s = Stack {
            item: 7,
            amount: 3,
            limit: 5,
        };
        i.add(s).unwrap();
        assert_eq!(i.add(s), Err(LoopError::Capacity));
        assert_eq!(i.count(7), 3);
        assert_eq!(i.remove(7, 4), Err(LoopError::Unavailable));
        assert_eq!(i.count(7), 3);
        assert_eq!(i.add(Stack { item: 8, ..s }), Err(LoopError::Capacity));
        i.remove(7, 3).unwrap();
        i.add(Stack { item: 8, ..s }).unwrap();
        assert_eq!(i.count(7), 0);
        assert_eq!(i.stacks().count(), 1);
    }

    #[test]
    fn full_inventories_swap_using_slots_released_on_both_sides() {
        let mut bag = inventory::<1>(&[stack(1, 3, 10)]);
        let mut trader = inventory::<2>(&[stack(2, 1, 5), stack(3, 2, 2)]);
        let before = (bag.slots, trader.slots);
        assert_eq!(bag.check_exchange(&trader, &[(1, 3)], &[(2, 1)]), Ok(()));
        assert_eq!((bag.slots, trader.slots), before);
        bag.exchange(&mut trader, &[(1, 3)], &[(2, 1)]).unwrap();
        assert_eq!(bag.slots, [Some(stack(2, 1, 5))]);
        assert_eq!(trader.slots, [Some(stack(1, 3, 10)), Some(stack(3, 2, 2))]);
    }

    #[test]
    fn batch_transfer_merges_repeats_and_preserves_source_limits() {
        let mut bag = inventory::<2>(&[stack(0, 6, 10), stack(9, 2, 2)]);
        let mut chest = inventory::<3>(&[stack(0, 1, 10)]);
        let items = [(0, 1), (9, 2), (0, 2)];
        let before = (bag.slots, chest.slots);
        assert_eq!(bag.check_transfer_to(&chest, &items), Ok(()));
        assert_eq!((bag.slots, chest.slots), before);
        bag.transfer_to(&mut chest, &items).unwrap();
        assert_eq!(bag.slots, [Some(stack(0, 3, 10)), None]);
        assert_eq!(
            chest.slots,
            [Some(stack(0, 4, 10)), Some(stack(9, 2, 2)), None]
        );
    }

    #[test]
    fn missing_diagnostics_sum_repeats_and_preserve_earlier_withdrawals() {
        let bag = inventory::<2>(&[stack(1, 2, 5), stack(2, 3, 5)]);
        let chest = inventory::<1>(&[stack(3, 1, 5)]);
        assert_rejected(
            bag,
            chest,
            &[(1, 2), (2, 2), (2, 2)],
            &[],
            ExchangeError::MissingItem {
                side: ExchangeSide::This,
                item: 2,
                requested: 4,
                available: 3,
            },
        );
        assert_rejected(
            bag,
            chest,
            &[(1, 2)],
            &[(3, 1), (4, 1)],
            ExchangeError::MissingItem {
                side: ExchangeSide::Other,
                item: 4,
                requested: 1,
                available: 0,
            },
        );
    }

    #[test]
    fn incoming_items_cannot_pay_for_outgoing_requirements() {
        assert_rejected(
            inventory::<1>(&[stack(1, 5, 10)]),
            inventory::<1>(&[stack(1, 1, 10)]),
            &[(1, 4)],
            &[(1, 3)],
            ExchangeError::MissingItem {
                side: ExchangeSide::Other,
                item: 1,
                requested: 3,
                available: 1,
            },
        );
    }

    #[test]
    fn either_recipient_running_out_of_slots_rolls_back_both_inventories() {
        assert_rejected(
            inventory::<1>(&[stack(1, 2, 5)]),
            inventory::<1>(&[stack(2, 2, 5)]),
            &[(1, 1)],
            &[(2, 1)],
            ExchangeError::NoFreeSlot {
                side: ExchangeSide::This,
                item: 2,
            },
        );
        // This receives its entire output successfully before Other rejects.
        assert_rejected(
            inventory::<1>(&[stack(1, 2, 5)]),
            inventory::<2>(&[stack(2, 2, 5), stack(3, 1, 5)]),
            &[(1, 2)],
            &[(2, 1)],
            ExchangeError::NoFreeSlot {
                side: ExchangeSide::Other,
                item: 1,
            },
        );
        // The first output fits; the second cannot create another stack.
        assert_rejected(
            inventory::<2>(&[stack(1, 1, 5), stack(2, 1, 5)]),
            inventory::<1>(&[]),
            &[(1, 1), (2, 1)],
            &[],
            ExchangeError::NoFreeSlot {
                side: ExchangeSide::Other,
                item: 2,
            },
        );
    }

    #[test]
    fn remaining_stack_capacity_is_checked_after_outgoing_items_are_removed() {
        let mut bag = inventory::<1>(&[stack(1, 8, 10)]);
        let mut other = inventory::<1>(&[stack(1, 7, 10)]);
        bag.exchange(&mut other, &[(1, 5)], &[(1, 6)]).unwrap();
        assert_eq!((bag.count(1), other.count(1)), (9, 6));
        assert_rejected(
            bag,
            other,
            &[(1, 1)],
            &[(1, 3)],
            ExchangeError::StackCapacity {
                side: ExchangeSide::This,
                item: 1,
                requested: 3,
                available: 2,
            },
        );
    }

    #[test]
    fn overflow_reports_exact_missing_or_capacity_quantities() {
        let bag = inventory::<1>(&[stack(1, u32::MAX, u32::MAX)]);
        assert_rejected(
            bag,
            inventory::<1>(&[]),
            &[(1, u32::MAX), (1, u32::MAX)],
            &[],
            ExchangeError::MissingItem {
                side: ExchangeSide::This,
                item: 1,
                requested: 2 * u64::from(u32::MAX),
                available: u32::MAX,
            },
        );
        assert_rejected(
            bag,
            inventory::<1>(&[stack(1, 1, u32::MAX)]),
            &[(1, u32::MAX)],
            &[],
            ExchangeError::StackCapacity {
                side: ExchangeSide::Other,
                item: 1,
                requested: u32::MAX,
                available: u32::MAX - 1,
            },
        );
        let mut bag = bag;
        let mut other = inventory::<1>(&[]);
        bag.transfer_to(&mut other, &[(1, u32::MAX)]).unwrap();
        assert_eq!((bag.count(1), other.count(1)), (0, u32::MAX));
    }

    #[test]
    fn stack_limits_must_match_only_when_merging_remaining_items() {
        let bag = inventory::<1>(&[stack(1, 3, 7)]);
        let other = inventory::<1>(&[stack(1, 2, 10)]);
        assert_rejected(
            bag,
            other,
            &[(1, 1)],
            &[],
            ExchangeError::LimitMismatch {
                side: ExchangeSide::Other,
                item: 1,
                expected: 10,
                actual: 7,
            },
        );
        assert_rejected(
            bag,
            other,
            &[],
            &[(1, 1)],
            ExchangeError::LimitMismatch {
                side: ExchangeSide::This,
                item: 1,
                expected: 7,
                actual: 10,
            },
        );
        let (mut bag, mut other) = (bag, other);
        bag.exchange(&mut other, &[(1, 3)], &[(1, 2)]).unwrap();
        assert_eq!(bag.slots, [Some(stack(1, 2, 10))]);
        assert_eq!(other.slots, [Some(stack(1, 3, 7))]);
    }

    #[test]
    fn zero_amount_and_entry_bounds_are_checked_on_both_sides() {
        let bag = inventory::<1>(&[stack(1, 32, 32)]);
        let other = inventory::<1>(&[stack(1, 32, 32)]);
        for side in [ExchangeSide::This, ExchangeSide::Other] {
            let items = [(1, 1), (1, 0)];
            let (give, take) = match side {
                ExchangeSide::This => (items.as_slice(), &[][..]),
                ExchangeSide::Other => (&[][..], items.as_slice()),
            };
            assert_rejected(
                bag,
                other,
                give,
                take,
                ExchangeError::ZeroAmount { side, item: 1 },
            );
            let too_many = [(1, 1); MAX_EXCHANGE_ITEMS + 1];
            let (give, take) = match side {
                ExchangeSide::This => (too_many.as_slice(), &[][..]),
                ExchangeSide::Other => (&[][..], too_many.as_slice()),
            };
            assert_rejected(
                bag,
                other,
                give,
                take,
                ExchangeError::TooManyItems {
                    side,
                    count: MAX_EXCHANGE_ITEMS + 1,
                },
            );
        }
        let mut bag = bag;
        let mut other = inventory::<1>(&[]);
        bag.transfer_to(&mut other, &[(1, 1); MAX_EXCHANGE_ITEMS])
            .unwrap();
        assert_eq!((bag.count(1), other.count(1)), (0, 32));
    }

    #[test]
    fn empty_exchange_and_transfer_preserve_slot_order_and_holes() {
        let mut bag = inventory::<3>(&[stack(1, 1, 5), stack(2, 2, 5)]);
        bag.remove(1, 1).unwrap();
        let mut other = inventory::<1>(&[]);
        let before = (bag.slots, other.slots);
        assert_eq!(bag.check_exchange(&other, &[], &[]), Ok(()));
        assert_eq!(bag.exchange(&mut other, &[], &[]), Ok(()));
        assert_eq!(bag.check_transfer_to(&other, &[]), Ok(()));
        assert_eq!(bag.transfer_to(&mut other, &[]), Ok(()));
        assert_eq!((bag.slots, other.slots), before);
    }

    #[test]
    fn transfer_rechecks_capacity_after_a_successful_preview() {
        let mut bag = inventory::<1>(&[stack(1, 2, 5)]);
        let mut other = inventory::<1>(&[]);
        bag.check_transfer_to(&other, &[(1, 1)]).unwrap();
        other.add(stack(2, 1, 5)).unwrap();
        let before = (bag.slots, other.slots);
        let error = ExchangeError::NoFreeSlot {
            side: ExchangeSide::Other,
            item: 1,
        };
        assert_eq!(bag.check_transfer_to(&other, &[(1, 1)]), Err(error));
        assert_eq!(bag.transfer_to(&mut other, &[(1, 1)]), Err(error));
        assert_eq!((bag.slots, other.slots), before);
    }

    #[test]
    fn maximum_inventory_capacity_can_exchange_its_last_slot() {
        let stacks = std::array::from_fn::<_, 256, _>(|item| stack(item as u32, 1, 1));
        let mut bag = inventory::<256>(&stacks);
        let mut other = inventory::<1>(&[stack(256, 1, 1)]);
        bag.exchange(&mut other, &[(255, 1)], &[(256, 1)]).unwrap();
        assert_eq!(bag.stacks().count(), 256);
        assert_eq!(bag.slots[255], Some(stack(256, 1, 1)));
        assert_eq!(other.slots, [Some(stack(255, 1, 1))]);
    }

    #[test]
    fn small_exchanges_conserve_totals_and_every_error_preserves_exact_state() {
        for this_item in 0..2 {
            for other_item in 0..2 {
                for this_amount in 0..=3 {
                    for other_amount in 0..=3 {
                        for give_amount in 0..=4 {
                            for take_amount in 0..=4 {
                                let mut this = inventory::<1>(&[]);
                                let mut other = inventory::<1>(&[]);
                                if this_amount > 0 {
                                    this.add(stack(this_item, this_amount, 3)).unwrap();
                                }
                                if other_amount > 0 {
                                    other.add(stack(other_item, other_amount, 3)).unwrap();
                                }
                                let before = (this, other);
                                let give = [(this_item, give_amount)];
                                let take = [(other_item, take_amount)];
                                let preview = this.check_exchange(&other, &give, &take);
                                let result = this.exchange(&mut other, &give, &take);
                                assert_eq!(result, preview);
                                for item in 0..2 {
                                    assert_eq!(
                                        this.count(item) + other.count(item),
                                        before.0.count(item) + before.1.count(item)
                                    );
                                }
                                if result.is_err() {
                                    assert_eq!(
                                        (this.slots, other.slots),
                                        (before.0.slots, before.1.slots)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
