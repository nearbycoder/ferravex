//! Recipe execution on a copy, committed only after all costs and outputs fit.
use super::{
    LoopError,
    inventory::{Inventory, Stack},
};
/// Inputs/outputs are host-authored trusted recipe data, never client prices.
/// Repeated ingredients are summed by sequential removal. At most 32 of each.
/// Output capacity is evaluated after consuming inputs, allowing slot reuse.
pub fn craft<const N: usize>(
    inventory: &mut Inventory<N>,
    inputs: &[(u32, u32)],
    outputs: &[Stack],
) -> Result<(), LoopError> {
    if inputs.is_empty() || outputs.is_empty() || inputs.len() > 32 || outputs.len() > 32 {
        return Err(LoopError::Invalid);
    }
    let mut next = *inventory;
    for &(item, amount) in inputs {
        next.remove(item, amount)?;
    }
    for &stack in outputs {
        next.add(stack)?;
    }
    *inventory = next;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_output_rolls_back_costs_and_consumed_slots_can_be_reused() {
        let mut i = Inventory::<1>::new().unwrap();
        let ore = Stack {
            item: 1,
            amount: 4,
            limit: 10,
        };
        i.add(ore).unwrap();
        let bar = Stack {
            item: 2,
            amount: 1,
            limit: 5,
        };
        assert_eq!(craft(&mut i, &[(1, 2)], &[bar]), Err(LoopError::Capacity));
        assert_eq!(i.count(1), 4);
        craft(&mut i, &[(1, 2), (1, 2)], &[bar]).unwrap();
        assert_eq!(i.count(2), 1);
        assert_eq!(
            craft(&mut i, &[(2, 2)], &[ore]),
            Err(LoopError::Unavailable)
        );
        assert_eq!(i.count(2), 1);
    }
}
