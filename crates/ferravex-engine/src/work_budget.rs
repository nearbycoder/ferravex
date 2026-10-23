//! Deterministic shared work quotas for AI, streaming and incremental rebuilds.
//! Units are host-defined work items, not inferred CPU time. Requests never partially
//! consume unless the caller explicitly uses [`WorkBudget::grant_up_to`].
//!
//! A reservation prepays a checked `items * cost_per_item` in call order. Refund
//! unused units explicitly; the budget does not queue requests or track individual
//! reservations. Diagnostics cover the period since construction, reset or a limit
//! change. All operations use constant space and allocate no storage.
//!
//! ```
//! use ferravex_engine::work_budget::{WorkBudget, WorkBudgetError};
//!
//! let mut budget = WorkBudget::new(10);
//! let reserved = budget.try_reserve(3, 2)?; // Three jobs, two units each.
//! assert_eq!(reserved, 6);
//! budget.refund(2)?; // One job did not need to run.
//! assert!(budget.try_consume(5)); // Later systems see the returned allowance.
//! assert!(!budget.try_consume(2));
//! let report = budget.diagnostics();
//! assert_eq!((report.limit, report.remaining, report.consumed), (10, 1, 9));
//! assert_eq!((report.denied_requests, report.denied_units), (1, 2));
//! budget.reset();
//! assert_eq!(budget.remaining(), budget.limit());
//! assert_eq!(budget.diagnostics().denied_requests, 0);
//! # Ok::<(), WorkBudgetError>(())
//! ```

use std::fmt;

/// A checked reservation or refund could not be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkBudgetError {
    /// The requested total cost cannot be represented as `u64`.
    CostOverflow { items: u64, cost_per_item: u64 },
    /// An atomic request exceeds the currently available allowance.
    Exhausted { requested: u64, remaining: u64 },
    /// A refund exceeds the net spend in the current budget period.
    RefundExceedsSpent { requested: u64, spent: u64 },
}

impl fmt::Display for WorkBudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CostOverflow {
                items,
                cost_per_item,
            } => write!(
                f,
                "work cost overflows u64: {items} items at {cost_per_item} units each"
            ),
            Self::Exhausted {
                requested,
                remaining,
            } => write!(
                f,
                "work budget exhausted: requested {requested} units, {remaining} remaining"
            ),
            Self::RefundExceedsSpent { requested, spent } => write!(
                f,
                "cannot refund {requested} units: only {spent} units are spent"
            ),
        }
    }
}

impl std::error::Error for WorkBudgetError {}

/// Snapshot of the current budget period, including legacy consumption methods.
///
/// `limit`, `remaining` and `consumed` are exact. Denial counters saturate at
/// `u64::MAX` and set `saturated` if any count exceeds that value. Diagnostics
/// never affect whether a request can be granted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkBudgetDiagnostics {
    /// Maximum allowance for this period.
    pub limit: u64,
    /// Units available to subsequent requests.
    pub remaining: u64,
    /// Net units charged, including reservations and excluding refunds.
    /// Equals [`WorkBudget::spent`], not a cumulative total across refunds.
    pub consumed: u64,
    /// Requests denied in whole or part, including cost overflows.
    pub denied_requests: u64,
    /// Ungranted units: the entire cost of an atomic denial, or the ungranted
    /// portion of a partial grant. Unrepresentable costs are excluded and
    /// counted separately in `cost_overflows`.
    pub denied_units: u64,
    /// Reservation requests rejected because their total cost exceeded `u64::MAX`.
    pub cost_overflows: u64,
    /// At least one denial counter exceeded `u64::MAX` during this period.
    pub saturated: bool,
}

/// Shared allowance consumed synchronously in the caller's deterministic order.
///
/// Copies are independent snapshots, not handles to a shared quota. Share one
/// mutable budget among systems to enforce a common limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkBudget {
    limit: u64,
    remaining: u64,
    denied_requests: u64,
    denied_units: u64,
    cost_overflows: u64,
    saturated: bool,
}
impl WorkBudget {
    /// Start a fresh budget period. Zero and `u64::MAX` are valid limits.
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            remaining: limit,
            denied_requests: 0,
            denied_units: 0,
            cost_overflows: 0,
            saturated: false,
        }
    }

    /// Charge all units or return `false` without spending any allowance.
    /// Failed requests contribute to the denial diagnostics; zero always succeeds.
    pub fn try_consume(&mut self, units: u64) -> bool {
        self.reserve_units(units).is_ok()
    }

    /// Prepay all `items * cost_per_item` units and return the charged cost.
    ///
    /// Check multiplication before availability. Either zero operand requests
    /// zero units and succeeds even when exhausted. Errors leave allowance
    /// unchanged but record a denied request. No partial reservation is made.
    /// Use [`Self::refund`] to return unused units; there is no automatic refund.
    ///
    /// ```
    /// use ferravex_engine::work_budget::{WorkBudget, WorkBudgetError};
    /// let mut budget = WorkBudget::new(7);
    /// assert_eq!(budget.try_reserve(2, 3), Ok(6));
    /// assert_eq!(budget.try_reserve(1, 2), Err(WorkBudgetError::Exhausted {
    ///     requested: 2, remaining: 1,
    /// }));
    /// assert_eq!(budget.try_reserve(u64::MAX, 2), Err(WorkBudgetError::CostOverflow {
    ///     items: u64::MAX, cost_per_item: 2,
    /// }));
    /// assert_eq!(budget.remaining(), 1);
    /// ```
    pub fn try_reserve(&mut self, items: u64, cost_per_item: u64) -> Result<u64, WorkBudgetError> {
        let Some(units) = items.checked_mul(cost_per_item) else {
            self.record_denial(None);
            return Err(WorkBudgetError::CostOverflow {
                items,
                cost_per_item,
            });
        };
        self.reserve_units(units)?;
        Ok(units)
    }

    /// Return unused units to the current period's pooled allowance.
    ///
    /// Refunds cannot exceed [`Self::spent`] or increase remaining allowance above
    /// the original limit. An invalid refund changes neither quota nor diagnostics.
    /// Valid refunds decrease net consumption but do not erase denied requests.
    /// The caller must return only unused units from this period: reservations
    /// are not individually tracked, and reset/limit changes start a new period.
    ///
    /// ```
    /// use ferravex_engine::work_budget::{WorkBudget, WorkBudgetError};
    /// let mut budget = WorkBudget::new(8);
    /// let reserved = budget.try_reserve(2, 3)?;
    /// budget.refund(reserved)?; // Cancel both jobs before running them.
    /// assert_eq!(budget.remaining(), 8);
    /// assert_eq!(budget.refund(1), Err(WorkBudgetError::RefundExceedsSpent {
    ///     requested: 1, spent: 0,
    /// }));
    /// # Ok::<(), WorkBudgetError>(())
    /// ```
    pub fn refund(&mut self, units: u64) -> Result<(), WorkBudgetError> {
        let spent = self.spent();
        if units > spent {
            return Err(WorkBudgetError::RefundExceedsSpent {
                requested: units,
                spent,
            });
        }
        // units <= limit - remaining proves this sum cannot overflow.
        self.remaining += units;
        Ok(())
    }

    fn reserve_units(&mut self, units: u64) -> Result<(), WorkBudgetError> {
        if units > self.remaining {
            self.record_denial(Some(units));
            return Err(WorkBudgetError::Exhausted {
                requested: units,
                remaining: self.remaining,
            });
        }
        self.remaining -= units;
        Ok(())
    }

    /// Charge the largest grant no greater than `requested` or the allowance.
    /// An ungranted remainder counts as one denied request and that many denied
    /// units. A zero request succeeds without recording a denial.
    pub fn grant_up_to(&mut self, requested: u64) -> u64 {
        let grant = requested.min(self.remaining);
        self.remaining -= grant;
        if grant < requested {
            self.record_denial(Some(requested - grant));
        }
        grant
    }

    /// Maximum allowance in the current period.
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Units available to subsequent requests.
    pub fn remaining(&self) -> u64 {
        self.remaining
    }

    /// Net units charged in the current period, after refunds.
    pub fn spent(&self) -> u64 {
        self.limit - self.remaining
    }

    /// Inspect exact net consumption and bounded denial counters without mutation.
    ///
    /// ```
    /// use ferravex_engine::work_budget::WorkBudget;
    /// let mut budget = WorkBudget::new(5);
    /// assert_eq!(budget.grant_up_to(8), 5);
    /// let report = budget.diagnostics();
    /// assert_eq!(report.consumed, 5);
    /// assert_eq!(report.denied_requests, 1);
    /// assert_eq!(report.denied_units, 3);
    /// assert_eq!(report.cost_overflows, 0);
    /// assert!(!report.saturated);
    /// ```
    pub fn diagnostics(&self) -> WorkBudgetDiagnostics {
        WorkBudgetDiagnostics {
            limit: self.limit,
            remaining: self.remaining,
            consumed: self.spent(),
            denied_requests: self.denied_requests,
            denied_units: self.denied_units,
            cost_overflows: self.cost_overflows,
            saturated: self.saturated,
        }
    }

    /// Restore the current limit and clear this period's diagnostics.
    pub fn reset(&mut self) {
        *self = Self::new(self.limit);
    }

    /// Start a fresh period with a new limit and cleared diagnostics.
    pub fn set_limit(&mut self, limit: u64) {
        *self = Self::new(limit);
    }

    fn record_denial(&mut self, units: Option<u64>) {
        fn add(counter: &mut u64, amount: u64, saturated: &mut bool) {
            match counter.checked_add(amount) {
                Some(total) => *counter = total,
                None => {
                    *counter = u64::MAX;
                    *saturated = true;
                }
            }
        }
        add(&mut self.denied_requests, 1, &mut self.saturated);
        match units {
            Some(units) => add(&mut self.denied_units, units, &mut self.saturated),
            None => add(&mut self.cost_overflows, 1, &mut self.saturated),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn systems_share_quota_and_failed_reservations_are_atomic() {
        let mut b = WorkBudget::new(100);
        assert!(b.try_consume(60));
        assert!(!b.try_consume(50));
        assert_eq!(b.remaining(), 40);
        assert_eq!(b.grant_up_to(90), 40);
        assert_eq!(b.spent(), 100);
        assert!(b.try_consume(0));
        assert!(!b.try_consume(1));
        b.reset();
        assert_eq!(b.remaining(), 100);
        b.set_limit(u64::MAX);
        assert!(b.try_consume(u64::MAX));
        assert_eq!(b.spent(), u64::MAX);
    }

    #[test]
    fn reservations_charge_checked_cost_atomically_in_call_order() {
        let mut b = WorkBudget::new(10);
        assert_eq!(b.try_reserve(3, 3), Ok(9));
        assert_eq!(
            b.try_reserve(1, 2),
            Err(WorkBudgetError::Exhausted {
                requested: 2,
                remaining: 1,
            })
        );
        assert_eq!((b.spent(), b.remaining()), (9, 1));
        // A failed large request does not block a later request that fits.
        assert_eq!(b.try_reserve(1, 1), Ok(1));
        assert_eq!(
            b.diagnostics(),
            WorkBudgetDiagnostics {
                limit: 10,
                remaining: 0,
                consumed: 10,
                denied_requests: 1,
                denied_units: 2,
                cost_overflows: 0,
                saturated: false,
            }
        );
    }

    #[test]
    fn overflow_is_rejected_before_availability_without_charging() {
        for limit in [0, 7, u64::MAX] {
            let mut b = WorkBudget::new(limit);
            assert_eq!(
                b.try_reserve(u64::MAX, 2),
                Err(WorkBudgetError::CostOverflow {
                    items: u64::MAX,
                    cost_per_item: 2,
                })
            );
            assert_eq!((b.remaining(), b.spent()), (limit, 0));
            let report = b.diagnostics();
            assert_eq!(report.denied_requests, 1);
            assert_eq!(report.cost_overflows, 1);
            assert_eq!(report.denied_units, 0);
            assert!(!report.saturated);
        }
        let mut b = WorkBudget::new(u64::MAX);
        assert_eq!(b.try_reserve(u64::MAX, 1), Ok(u64::MAX));
        assert_eq!(b.remaining(), 0);
        b.refund(u64::MAX).unwrap();
        assert_eq!(b.try_reserve(1, u64::MAX), Ok(u64::MAX));
        assert_eq!(b.spent(), u64::MAX);
    }

    #[test]
    fn zero_cost_requests_and_refunds_do_not_record_denials() {
        let mut b = WorkBudget::new(0);
        let before = b;
        assert_eq!(b.try_reserve(u64::MAX, 0), Ok(0));
        assert_eq!(b.try_reserve(0, u64::MAX), Ok(0));
        assert_eq!(b.try_reserve(0, 0), Ok(0));
        assert!(b.try_consume(0));
        assert_eq!(b.grant_up_to(0), 0);
        assert_eq!(b.refund(0), Ok(()));
        assert_eq!(b, before);
    }

    #[test]
    fn refunds_restore_only_spent_allowance_and_keep_denial_history() {
        let mut b = WorkBudget::new(12);
        assert_eq!(b.try_reserve(4, 3), Ok(12));
        assert!(!b.try_consume(3));
        b.refund(6).unwrap();
        assert_eq!((b.remaining(), b.spent()), (6, 6));
        let before = b;
        assert_eq!(
            b.refund(7),
            Err(WorkBudgetError::RefundExceedsSpent {
                requested: 7,
                spent: 6,
            })
        );
        assert_eq!(b, before);
        assert_eq!(b.try_reserve(2, 3), Ok(6));
        b.refund(12).unwrap();
        assert_eq!((b.remaining(), b.spent()), (12, 0));
        let report = b.diagnostics();
        assert_eq!(report.consumed, 0);
        assert_eq!(report.denied_requests, 1);
        assert_eq!(report.denied_units, 3);
        let before = b;
        assert!(b.refund(1).is_err());
        assert_eq!(b, before);
    }

    #[test]
    fn legacy_consumption_and_partial_grants_share_diagnostics() {
        let mut b = WorkBudget::new(10);
        assert!(b.try_consume(6));
        assert!(!b.try_consume(5)); // Entire atomic request denied, not just 1 unit.
        assert_eq!(b.grant_up_to(7), 4); // Only the missing 3 units are denied.
        assert_eq!(b.grant_up_to(2), 0);
        b.refund(3).unwrap();
        assert_eq!(b.grant_up_to(2), 2);
        assert_eq!(
            b.diagnostics(),
            WorkBudgetDiagnostics {
                limit: 10,
                remaining: 1,
                consumed: 9,
                denied_requests: 3,
                denied_units: 10,
                cost_overflows: 0,
                saturated: false,
            }
        );
    }

    #[test]
    fn refund_cannot_overflow_at_maximum_limit() {
        let mut b = WorkBudget::new(u64::MAX);
        assert!(b.try_consume(1));
        let before = b;
        assert_eq!(
            b.refund(u64::MAX),
            Err(WorkBudgetError::RefundExceedsSpent {
                requested: u64::MAX,
                spent: 1,
            })
        );
        assert_eq!(b, before);
        b.refund(1).unwrap();
        assert_eq!(b.remaining(), u64::MAX);
    }

    #[test]
    fn counters_saturate_visibly_without_changing_admission() {
        let mut b = WorkBudget::new(1);
        assert!(!b.try_consume(u64::MAX));
        assert_eq!(b.diagnostics().denied_units, u64::MAX);
        assert!(!b.diagnostics().saturated); // Exactly MAX is still exact.
        assert!(!b.try_consume(2));
        assert_eq!(b.diagnostics().denied_units, u64::MAX);
        assert!(b.diagnostics().saturated);
        assert_eq!(b.try_reserve(1, 1), Ok(1));
        b.refund(1).unwrap();
        assert_eq!(b.remaining(), 1);

        // Exercise unreachable-in-practice counter boundaries directly.
        b.denied_requests = u64::MAX - 1;
        b.cost_overflows = u64::MAX - 1;
        b.saturated = false;
        assert!(b.try_reserve(u64::MAX, 2).is_err());
        assert!(!b.diagnostics().saturated);
        assert!(b.try_reserve(u64::MAX, 2).is_err());
        assert_eq!(b.diagnostics().denied_requests, u64::MAX);
        assert_eq!(b.diagnostics().cost_overflows, u64::MAX);
        assert!(b.diagnostics().saturated);
        assert_eq!(b.grant_up_to(1), 1);
    }

    #[test]
    fn resets_and_limit_changes_start_fresh_periods() {
        let mut b = WorkBudget::new(8);
        assert!(b.try_consume(4));
        assert!(!b.try_consume(u64::MAX));
        assert!(!b.try_consume(u64::MAX));
        assert!(b.try_reserve(u64::MAX, 2).is_err());
        assert!(b.diagnostics().saturated);
        b.reset();
        assert_eq!(b, WorkBudget::new(8));
        assert!(b.refund(4).is_err()); // Previous-period spend is no longer refundable.
        assert!(b.try_consume(6));
        b.set_limit(2);
        assert_eq!(b, WorkBudget::new(2));
        assert!(b.refund(6).is_err());
        assert_eq!(b.grant_up_to(3), 2);
        b.set_limit(u64::MAX);
        assert_eq!(b, WorkBudget::new(u64::MAX));
        b.set_limit(0);
        assert_eq!(b, WorkBudget::new(0));
    }

    #[test]
    fn mixed_request_sequences_replay_identically_and_preserve_bounds() {
        fn run() -> (WorkBudget, [u64; 5]) {
            let mut b = WorkBudget::new(11);
            let first = b.try_reserve(2, 3).unwrap();
            let second = u64::from(b.try_consume(6));
            b.refund(3).unwrap();
            let third = b.try_reserve(2, 4).unwrap();
            let fourth = b.grant_up_to(7);
            b.refund(2).unwrap();
            let fifth = b.grant_up_to(1);
            (b, [first, second, third, fourth, fifth])
        }
        let (budget, outcomes) = run();
        assert_eq!(outcomes, [6, 0, 8, 0, 1]);
        assert_eq!((budget, outcomes), run());
        assert_eq!(budget.remaining() + budget.spent(), budget.limit());
        assert_eq!(budget.diagnostics().denied_units, 13);
    }

    #[test]
    fn small_cost_and_refund_combinations_preserve_exact_bounds() {
        for limit in 0..=8 {
            for spent in 0..=limit {
                for items in 0..=5 {
                    for cost in 0..=5 {
                        let mut b = WorkBudget::new(limit);
                        assert!(b.try_consume(spent));
                        let units = items * cost;
                        let before = b.remaining();
                        let result = b.try_reserve(items, cost);
                        if units <= before {
                            assert_eq!(result, Ok(units));
                            assert_eq!(b.remaining(), before - units);
                            b.refund(units).unwrap();
                            assert_eq!(b.remaining(), before);
                        } else {
                            assert_eq!(
                                result,
                                Err(WorkBudgetError::Exhausted {
                                    requested: units,
                                    remaining: before,
                                })
                            );
                            assert_eq!(b.remaining(), before);
                        }
                        assert_eq!(b.spent(), spent);
                        assert_eq!(b.remaining() + b.spent(), limit);
                    }
                }
            }
        }
    }
}
