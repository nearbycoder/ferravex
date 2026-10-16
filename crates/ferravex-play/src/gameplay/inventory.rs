//! Fixed-capacity inventories with one bounded stack per item and atomic edits.
use super::LoopError;
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
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
