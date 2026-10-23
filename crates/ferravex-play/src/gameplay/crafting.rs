//! Recipe preview and execution on a copy, committed only after all costs and outputs fit.
use super::{
    LoopError,
    inventory::{Inventory, Stack},
};

const MAX_RECIPE_STACKS: usize = 32;

/// One missing ingredient, aggregated across every input with the same item ID.
/// See [`preview`] for an example. Amounts use `u64` so up to 32 `u32` costs
/// remain exact even when the required total exceeds inventory stack limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingIngredient {
    /// The recipe's item ID.
    pub item: u32,
    /// Total input cost, before any outputs are produced.
    pub required: u64,
    /// Amount currently held in the inventory.
    pub available: u32,
    /// Exact shortfall: `required - u64::from(available)`.
    pub missing: u64,
}

/// An owned snapshot of crafting availability and missing ingredients.
///
/// Fixed storage holds at most 32 missing items without heap allocation.
/// This does not reserve inventory; call [`craft`] to check again and commit.
/// See [`preview`] for an example.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct CraftPreview {
    result: Result<(), LoopError>,
    missing: [MissingIngredient; MAX_RECIPE_STACKS],
    missing_len: usize,
}

impl CraftPreview {
    /// Whether the entire recipe, including its outputs, can be crafted.
    pub fn is_available(&self) -> bool {
        self.result.is_ok()
    }

    /// The exact result [`craft`] would return for the previewed inventory.
    ///
    /// Error ordering is unchanged: recipe lengths first, then input removals
    /// in order, then output additions in order. A later invalid entry can be
    /// masked by an earlier unavailable input or full output stack.
    pub fn result(&self) -> Result<(), LoopError> {
        self.result
    }

    /// Every input shortfall, in first-appearance order, with duplicates summed.
    ///
    /// For recipe lengths within 1..=32, this describes all input costs even
    /// if another entry is invalid. Zero costs contribute nothing. Out-of-range
    /// recipe lengths yield an empty slice and [`LoopError::Invalid`].
    /// An empty slice alone does not imply availability: outputs must also fit.
    pub fn missing_ingredients(&self) -> &[MissingIngredient] {
        &self.missing[..self.missing_len]
    }
}

/// Inspect the same trusted recipe accepted by [`craft`] without changing inventory.
///
/// Availability includes consuming all inputs before checking outputs, so freed
/// slots can be reused and outputs cannot pay for missing inputs. All ingredient
/// shortfalls are reported, including those after the first failed removal.
/// Duplicate input totals use `u64`; overflowing output amounts yield
/// [`LoopError::Capacity`], just as they do in [`craft`].
///
/// ```
/// use ferravex_play::gameplay::{
///     LoopError,
///     crafting::{craft, preview, MissingIngredient},
///     inventory::{Inventory, Stack},
/// };
///
/// let mut inventory = Inventory::<1>::new()?;
/// inventory.add(Stack { item: 1, amount: 3, limit: 10 })?;
/// let inputs = [(1, 2), (1, 3)];
/// let outputs = [Stack { item: 2, amount: 1, limit: 5 }];
/// let report = preview(&inventory, &inputs, &outputs);
/// assert!(!report.is_available());
/// assert_eq!(report.result(), Err(LoopError::Unavailable));
/// assert_eq!(report.missing_ingredients(), &[MissingIngredient {
///     item: 1, required: 5, available: 3, missing: 2,
/// }]);
/// assert_eq!(inventory.count(1), 3);
///
/// inventory.add(Stack { item: 1, amount: 2, limit: 10 })?;
/// let ready = preview(&inventory, &inputs, &outputs);
/// assert!(ready.is_available()); // Spending all ore frees the only slot.
/// assert!(ready.missing_ingredients().is_empty());
/// assert_eq!(craft(&mut inventory, &inputs, &outputs), ready.result());
/// assert_eq!(inventory.count(2), 1);
/// # Ok::<(), LoopError>(())
/// ```
pub fn preview<const N: usize>(
    inventory: &Inventory<N>,
    inputs: &[(u32, u32)],
    outputs: &[Stack],
) -> CraftPreview {
    let mut report = CraftPreview {
        result: prepare(inventory, inputs, outputs).map(|_| ()),
        missing: [MissingIngredient {
            item: 0,
            required: 0,
            available: 0,
            missing: 0,
        }; MAX_RECIPE_STACKS],
        missing_len: 0,
    };
    if !valid_lengths(inputs, outputs) {
        return report;
    }
    for (index, &(item, _)) in inputs.iter().enumerate() {
        if inputs[..index].iter().any(|&(id, _)| id == item) {
            continue;
        }
        // At most 32 u32 values: the sum always fits in u64.
        let required: u64 = inputs[index..]
            .iter()
            .filter(|&&(id, _)| id == item)
            .map(|&(_, amount)| u64::from(amount))
            .sum();
        let available = inventory.count(item);
        if required > u64::from(available) {
            report.missing[report.missing_len] = MissingIngredient {
                item,
                required,
                available,
                missing: required - u64::from(available),
            };
            report.missing_len += 1;
        }
    }
    report
}

/// Inputs/outputs are host-authored trusted recipe data, never client prices.
/// Repeated ingredients are summed by sequential removal. At most 32 of each.
/// Output capacity is evaluated after consuming inputs, allowing slot reuse.
/// See [`preview`] for a non-mutating availability check and usage example.
pub fn craft<const N: usize>(
    inventory: &mut Inventory<N>,
    inputs: &[(u32, u32)],
    outputs: &[Stack],
) -> Result<(), LoopError> {
    *inventory = prepare(inventory, inputs, outputs)?;
    Ok(())
}

fn valid_lengths(inputs: &[(u32, u32)], outputs: &[Stack]) -> bool {
    (1..=MAX_RECIPE_STACKS).contains(&inputs.len())
        && (1..=MAX_RECIPE_STACKS).contains(&outputs.len())
}

fn prepare<const N: usize>(
    inventory: &Inventory<N>,
    inputs: &[(u32, u32)],
    outputs: &[Stack],
) -> Result<Inventory<N>, LoopError> {
    if !valid_lengths(inputs, outputs) {
        return Err(LoopError::Invalid);
    }
    let mut next = *inventory;
    for &(item, amount) in inputs {
        next.remove(item, amount)?;
    }
    for &stack in outputs {
        next.add(stack)?;
    }
    Ok(next)
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

    fn check_preview<const N: usize>(
        inventory: &Inventory<N>,
        inputs: &[(u32, u32)],
        outputs: &[Stack],
        expected: Result<(), LoopError>,
    ) -> CraftPreview {
        let before: Vec<_> = inventory.stacks().collect();
        let report = preview(inventory, inputs, outputs);
        assert_eq!(inventory.stacks().collect::<Vec<_>>(), before);
        assert_eq!(report.result(), expected);
        assert_eq!(report.is_available(), expected.is_ok());
        let mut actual = *inventory;
        assert_eq!(craft(&mut actual, inputs, outputs), expected);
        if expected.is_err() {
            assert_eq!(actual.stacks().collect::<Vec<_>>(), before);
        }
        report
    }

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

    #[test]
    fn preview_reports_all_shortfalls_once_in_first_input_order() {
        let mut inventory = Inventory::<3>::new().unwrap();
        inventory.add(stack(9, 1, 10)).unwrap();
        inventory.add(stack(1, 5, 10)).unwrap();
        inventory.add(stack(0, 2, 10)).unwrap();
        let report = check_preview(
            &inventory,
            &[(9, 2), (1, 2), (9, 5), (0, 4), (1, 3), (7, 1)],
            &[stack(8, 1, 10)],
            Err(LoopError::Unavailable),
        );
        assert_eq!(
            report.missing_ingredients(),
            &[
                MissingIngredient {
                    item: 9,
                    required: 7,
                    available: 1,
                    missing: 6,
                },
                MissingIngredient {
                    item: 0,
                    required: 4,
                    available: 2,
                    missing: 2,
                },
                MissingIngredient {
                    item: 7,
                    required: 1,
                    available: 0,
                    missing: 1,
                },
            ]
        );
    }

    #[test]
    fn repeated_costs_above_u32_max_remain_exact() {
        let mut inventory = Inventory::<1>::new().unwrap();
        inventory.add(stack(1, u32::MAX, u32::MAX)).unwrap();
        let report = check_preview(
            &inventory,
            &[(1, u32::MAX); 32],
            &[stack(2, 1, 1)],
            Err(LoopError::Unavailable),
        );
        assert_eq!(
            report.missing_ingredients(),
            &[MissingIngredient {
                item: 1,
                required: 32 * u64::from(u32::MAX),
                available: u32::MAX,
                missing: 31 * u64::from(u32::MAX),
            }]
        );
    }

    #[test]
    fn preview_reuses_consumed_slots_and_sums_outputs() {
        let mut inventory = Inventory::<1>::new().unwrap();
        inventory.add(stack(1, 4, 10)).unwrap();
        let inputs = [(1, 2), (1, 2)];
        let outputs = [stack(2, 1, 5), stack(2, 2, 5)];
        let ready = check_preview(&inventory, &inputs, &outputs, Ok(()));
        assert!(ready.missing_ingredients().is_empty());
        let full = check_preview(&inventory, &[(1, 3)], &outputs, Err(LoopError::Capacity));
        assert!(full.missing_ingredients().is_empty());
        craft(&mut inventory, &inputs, &outputs).unwrap();
        assert_eq!(inventory.stacks().collect::<Vec<_>>(), [stack(2, 3, 5)]);
    }

    #[test]
    fn output_overflow_and_limit_errors_match_atomic_craft() {
        let mut inventory = Inventory::<2>::new().unwrap();
        inventory.add(stack(1, 1, 1)).unwrap();
        inventory.add(stack(2, u32::MAX - 1, u32::MAX)).unwrap();
        let report = check_preview(
            &inventory,
            &[(1, 1)],
            &[stack(2, 1, u32::MAX), stack(2, 1, u32::MAX)],
            Err(LoopError::Capacity),
        );
        assert!(report.missing_ingredients().is_empty());
        let _ = check_preview(
            &inventory,
            &[(1, 1)],
            &[stack(2, 1, 5)],
            Err(LoopError::Invalid),
        );
        for invalid in [stack(3, 0, 1), stack(3, 1, 0), stack(3, 2, 1)] {
            let _ = check_preview(&inventory, &[(1, 1)], &[invalid], Err(LoopError::Invalid));
        }
        let _ = check_preview(
            &inventory,
            &[(2, u32::MAX - 1)],
            &[stack(3, 3, 5), stack(3, 3, 5)],
            Err(LoopError::Capacity),
        );
    }

    #[test]
    fn same_item_outputs_follow_consumption_and_do_not_pay_costs() {
        let mut inventory = Inventory::<1>::new().unwrap();
        inventory.add(stack(1, u32::MAX, u32::MAX)).unwrap();
        let _ = check_preview(&inventory, &[(1, 1)], &[stack(1, 1, u32::MAX)], Ok(()));
        // Consuming the whole stack permits changing its limit; partial removal does not.
        let _ = check_preview(&inventory, &[(1, u32::MAX)], &[stack(1, 2, 5)], Ok(()));
        let _ = check_preview(
            &inventory,
            &[(1, 1)],
            &[stack(1, 2, 5)],
            Err(LoopError::Invalid),
        );
        let empty = Inventory::<1>::new().unwrap();
        let report = check_preview(
            &empty,
            &[(1, 1)],
            &[stack(1, 1, 1)],
            Err(LoopError::Unavailable),
        );
        assert_eq!(report.missing_ingredients()[0].missing, 1);
    }

    #[test]
    fn preview_preserves_first_failure_order_in_malformed_recipes() {
        let mut inventory = Inventory::<1>::new().unwrap();
        inventory.add(stack(1, 1, 1)).unwrap();
        let report = check_preview(
            &inventory,
            &[(2, 1), (1, 0)],
            &[stack(3, 0, 1)],
            Err(LoopError::Unavailable),
        );
        assert_eq!(report.missing_ingredients()[0].item, 2);
        let report = check_preview(
            &inventory,
            &[(1, 0), (2, 1)],
            &[stack(3, 1, 1)],
            Err(LoopError::Invalid),
        );
        assert_eq!(report.missing_ingredients()[0].item, 2);
        let _ = check_preview(
            &inventory,
            &[(1, 1)],
            &[stack(2, 1, 1), stack(3, 1, 1), stack(4, 0, 1)],
            Err(LoopError::Capacity),
        );
        let _ = check_preview(
            &inventory,
            &[(1, 1)],
            &[stack(4, 0, 1), stack(2, 1, 1), stack(3, 1, 1)],
            Err(LoopError::Invalid),
        );
    }

    #[test]
    fn recipe_length_bounds_apply_before_stack_operations() {
        let mut inventory = Inventory::<1>::new().unwrap();
        inventory.add(stack(1, 32, 32)).unwrap();
        let _ = check_preview(&inventory, &[(1, 1); 32], &[stack(2, 1, 32); 32], Ok(()));
        for inputs in [&[][..], &[(2, 1); 33][..]] {
            let report = check_preview(
                &inventory,
                inputs,
                &[stack(3, 1, 1)],
                Err(LoopError::Invalid),
            );
            assert!(report.missing_ingredients().is_empty());
        }
        for outputs in [&[][..], &[stack(3, 1, 1); 33][..]] {
            let report = check_preview(&inventory, &[(2, 1)], outputs, Err(LoopError::Invalid));
            assert!(report.missing_ingredients().is_empty());
        }
    }

    #[test]
    fn all_32_distinct_shortfalls_fit_without_truncation() {
        let inventory = Inventory::<1>::new().unwrap();
        let inputs: [(u32, u32); 32] = std::array::from_fn(|index| (31 - index as u32, 1));
        let report = check_preview(
            &inventory,
            &inputs,
            &[stack(32, 1, 1)],
            Err(LoopError::Unavailable),
        );
        assert_eq!(report.missing_ingredients().len(), 32);
        for (entry, &(item, _)) in report.missing_ingredients().iter().zip(&inputs) {
            assert_eq!(
                *entry,
                MissingIngredient {
                    item,
                    required: 1,
                    available: 0,
                    missing: 1,
                }
            );
        }
    }

    #[test]
    fn preview_is_a_snapshot_and_craft_rechecks_changed_inventory() {
        let mut inventory = Inventory::<1>::new().unwrap();
        inventory.add(stack(1, 1, 1)).unwrap();
        let ready = preview(&inventory, &[(1, 1)], &[stack(2, 1, 1)]);
        inventory.remove(1, 1).unwrap();
        assert!(ready.is_available());
        assert_eq!(
            craft(&mut inventory, &[(1, 1)], &[stack(2, 1, 1)]),
            Err(LoopError::Unavailable)
        );
        assert_eq!(inventory.stacks().count(), 0);
    }
}
