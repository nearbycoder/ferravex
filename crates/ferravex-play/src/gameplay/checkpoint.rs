//! Ordered, authored respawn anchors with rules-checked compact snapshots.
//!
//! [`Checkpoints::recovery`] describes where a host can respawn and which
//! checkpoint comes next. It does not teleport entities or reset world state.
//! Activation is monotonic; trusted save restoration may deliberately rewind.
use super::LoopError;

/// Compact save state. The host owns the fingerprint of the authored rules.
///
/// There is no timestamp or generation: two saves with the same rules and index
/// are indistinguishable. A host that needs freshness checks must store them in
/// its own save envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointSnapshot {
    pub rules: u64,
    pub index: usize,
}

/// An authored anchor paired with its route index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointAnchor<T> {
    pub index: usize,
    pub anchor: T,
}

/// A recovery target and the next eligible checkpoint, without changing state.
///
/// `next == None` means the current anchor is the last authored anchor. It does
/// not declare the game won or prevent respawning there. Hosts apply the actual
/// recovery and emit feedback only after accepting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointRecovery<T> {
    pub current: CheckpointAnchor<T>,
    pub next: Option<CheckpointAnchor<T>>,
}

/// The result of an in-order checkpoint trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointActivation {
    /// The next authored anchor became current; checkpoint feedback is eligible.
    Activated { previous_index: usize, index: usize },
    /// A duplicate trigger at the current anchor made no change.
    Current { index: usize },
    /// A trigger behind the current anchor made no change.
    Stale { index: usize, current_index: usize },
}

/// How a successful trusted restore changed checkpoint progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointRestore {
    /// The save already identifies the current anchor.
    Unchanged { index: usize },
    /// An earlier anchor was restored, intentionally discarding later progress.
    Rewound { previous_index: usize, index: usize },
    /// A later saved anchor was restored, possibly beyond the next trigger.
    Advanced { previous_index: usize, index: usize },
}

/// A rejected restore. Neither error changes the current checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointRestoreError {
    /// Rules are checked before the saved index, including when both are invalid.
    RulesMismatch { expected: u64, actual: u64 },
    /// The rules match, but the saved index is outside the authored route.
    InvalidIndex { index: usize, anchor_count: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct Checkpoints<T: Copy, const N: usize> {
    anchors: [T; N],
    index: usize,
    rules: u64,
}
impl<T: Copy, const N: usize> Checkpoints<T, N> {
    /// Hosts validate anchor geometry and supply a stable nonzero rules ID.
    /// Change that ID whenever the route's anchors, order or recovery rules change;
    /// this generic container cannot derive a fingerprint from arbitrary `T`.
    pub fn new(rules: u64, anchors: [T; N]) -> Result<Self, LoopError> {
        if N == 0 || N > 256 || rules == 0 {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            anchors,
            index: 0,
            rules,
        })
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn respawn(&self) -> T {
        self.anchors[self.index]
    }

    /// Inspect the current respawn anchor with its authored index.
    ///
    /// ```
    /// use ferravex_play::gameplay::checkpoint::{CheckpointAnchor, Checkpoints};
    /// let checkpoints = Checkpoints::new(4, [10, 20]).unwrap();
    /// assert_eq!(checkpoints.current_anchor(), CheckpointAnchor { index: 0, anchor: 10 });
    /// ```
    pub fn current_anchor(&self) -> CheckpointAnchor<T> {
        CheckpointAnchor {
            index: self.index,
            anchor: self.respawn(),
        }
    }

    /// Inspect the next eligible anchor, or `None` at the last authored anchor.
    /// This never wraps to the start or advances progress.
    ///
    /// ```
    /// use ferravex_play::gameplay::checkpoint::{CheckpointAnchor, Checkpoints};
    /// let mut checkpoints = Checkpoints::new(4, [10, 20]).unwrap();
    /// assert_eq!(checkpoints.next_anchor(), Some(CheckpointAnchor { index: 1, anchor: 20 }));
    /// checkpoints.activate(1).unwrap();
    /// assert_eq!(checkpoints.next_anchor(), None);
    /// ```
    pub fn next_anchor(&self) -> Option<CheckpointAnchor<T>> {
        let index = self.index + 1;
        self.anchors
            .get(index)
            .copied()
            .map(|anchor| CheckpointAnchor { index, anchor })
    }

    /// Describe recovery without consuming it or mutating checkpoint/world state.
    /// Repeated recovery remains available, including at a terminal anchor.
    /// The host decides whether to respawn and resets entities, velocity, camera
    /// history and any other world state in its own recovery transaction.
    ///
    /// ```
    /// use ferravex_play::gameplay::checkpoint::{CheckpointRecovery, Checkpoints};
    /// let checkpoints = Checkpoints::new(4, [(10, 5)]).unwrap();
    /// let recovery: CheckpointRecovery<(i32, i32)> = checkpoints.recovery();
    /// assert_eq!(recovery.current.index, 0);
    /// assert_eq!(recovery.current.anchor, (10, 5));
    /// assert_eq!(recovery.next, None);
    /// assert_eq!(checkpoints.recovery(), recovery);
    /// ```
    pub fn recovery(&self) -> CheckpointRecovery<T> {
        CheckpointRecovery {
            current: self.current_anchor(),
            next: self.next_anchor(),
        }
    }

    /// Old/duplicate checkpoints cannot move the respawn backward. Skipping an
    /// authored checkpoint fails, preventing exit triggers from bypassing a route.
    pub fn activate(&mut self, index: usize) -> Result<bool, LoopError> {
        self.activate_with_outcome(index)
            .map(|outcome| matches!(outcome, CheckpointActivation::Activated { .. }))
    }

    /// Activate the next anchor, distinguishing accepted, duplicate and stale
    /// triggers. Only `Activated` changes state or warrants checkpoint feedback.
    /// Out-of-range indices return `Invalid`; skipping ahead returns `Unavailable`.
    /// Both errors leave progress unchanged.
    ///
    /// ```
    /// use ferravex_play::gameplay::checkpoint::{CheckpointActivation, Checkpoints};
    /// let mut checkpoints = Checkpoints::new(4, [10, 20, 30]).unwrap();
    /// assert_eq!(checkpoints.activate_with_outcome(1), Ok(CheckpointActivation::Activated {
    ///     previous_index: 0, index: 1,
    /// }));
    /// assert_eq!(checkpoints.activate_with_outcome(1), Ok(CheckpointActivation::Current { index: 1 }));
    /// assert_eq!(checkpoints.activate_with_outcome(0), Ok(CheckpointActivation::Stale {
    ///     index: 0, current_index: 1,
    /// }));
    /// ```
    pub fn activate_with_outcome(
        &mut self,
        index: usize,
    ) -> Result<CheckpointActivation, LoopError> {
        if index >= N {
            return Err(LoopError::Invalid);
        }
        if index < self.index {
            return Ok(CheckpointActivation::Stale {
                index,
                current_index: self.index,
            });
        }
        if index == self.index {
            return Ok(CheckpointActivation::Current { index });
        }
        if index != self.index + 1 {
            return Err(LoopError::Unavailable);
        }
        let previous_index = self.index;
        self.index = index;
        Ok(CheckpointActivation::Activated {
            previous_index,
            index,
        })
    }
    pub fn snapshot(&self) -> CheckpointSnapshot {
        CheckpointSnapshot {
            rules: self.rules,
            index: self.index,
        }
    }
    /// Trusted save restoration only. This changes respawn state, not world state;
    /// the host restores inventory/objectives with its existing save transaction.
    pub fn restore(&mut self, saved: CheckpointSnapshot) -> Result<(), LoopError> {
        self.restore_with_outcome(saved)
            .map(|_| ())
            .map_err(|_| LoopError::Invalid)
    }

    /// Restore a trusted snapshot, reporting a rewind, advance or unchanged index.
    /// Rules and index are validated before mutation; either error preserves all
    /// checkpoint state. A matching old save may rewind and a later save may skip
    /// anchors, as with [`Self::restore`]. This is not a gameplay trigger or a
    /// network freshness check; [`CheckpointSnapshot`] has no generation field.
    ///
    /// Only checkpoint progress is restored. The host must coordinate its world
    /// transaction before using [`Self::recovery`] to apply the resulting anchor.
    ///
    /// ```
    /// use ferravex_play::gameplay::checkpoint::{
    ///     CheckpointRestore, CheckpointRestoreError, CheckpointSnapshot, Checkpoints,
    /// };
    /// let mut checkpoints = Checkpoints::new(4, [10, 20, 30]).unwrap();
    /// let start = checkpoints.snapshot();
    /// checkpoints.activate(1).unwrap();
    /// assert_eq!(checkpoints.restore_with_outcome(start), Ok(CheckpointRestore::Rewound {
    ///     previous_index: 1, index: 0,
    /// }));
    /// let bad = CheckpointSnapshot { rules: 5, index: 2 };
    /// assert_eq!(checkpoints.restore_with_outcome(bad), Err(CheckpointRestoreError::RulesMismatch {
    ///     expected: 4, actual: 5,
    /// }));
    /// assert_eq!(checkpoints.snapshot(), start);
    /// ```
    pub fn restore_with_outcome(
        &mut self,
        saved: CheckpointSnapshot,
    ) -> Result<CheckpointRestore, CheckpointRestoreError> {
        if saved.rules != self.rules {
            return Err(CheckpointRestoreError::RulesMismatch {
                expected: self.rules,
                actual: saved.rules,
            });
        }
        if saved.index >= N {
            return Err(CheckpointRestoreError::InvalidIndex {
                index: saved.index,
                anchor_count: N,
            });
        }
        let previous_index = self.index;
        let index = saved.index;
        let outcome = match index.cmp(&previous_index) {
            std::cmp::Ordering::Less => CheckpointRestore::Rewound {
                previous_index,
                index,
            },
            std::cmp::Ordering::Equal => CheckpointRestore::Unchanged { index },
            std::cmp::Ordering::Greater => CheckpointRestore::Advanced {
                previous_index,
                index,
            },
        };
        self.index = saved.index;
        Ok(outcome)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoints_are_ordered_and_snapshots_validate_before_mutation() {
        let mut c = Checkpoints::new(4, [10, 20, 30]).unwrap();
        assert_eq!(c.respawn(), 10);
        assert_eq!(c.activate(2), Err(LoopError::Unavailable));
        assert!(c.activate(1).unwrap());
        assert!(!c.activate(0).unwrap());
        let saved = c.snapshot();
        assert!(c.activate(2).unwrap());
        c.restore(saved).unwrap();
        assert_eq!(c.respawn(), 20);
        assert!(
            c.restore(CheckpointSnapshot { rules: 5, index: 0 })
                .is_err()
        );
        assert_eq!(c.index(), 1);
        assert!(
            c.restore(CheckpointSnapshot { rules: 4, index: 3 })
                .is_err()
        );
        assert_eq!(c.index(), 1);
    }

    #[test]
    fn recovery_inspects_current_and_next_without_consuming_or_advancing() {
        // Equal values remain distinguishable by authored index.
        let mut c = Checkpoints::new(4, [10, 10, 30]).unwrap();
        for index in 0..3 {
            c.activate(index).unwrap();
            let before = c.snapshot();
            let current = CheckpointAnchor {
                index,
                anchor: [10, 10, 30][index],
            };
            let next = [10, 10, 30]
                .get(index + 1)
                .copied()
                .map(|anchor| CheckpointAnchor {
                    index: index + 1,
                    anchor,
                });
            let expected = CheckpointRecovery { current, next };
            assert_eq!(c.current_anchor(), current);
            assert_eq!(c.next_anchor(), next);
            assert_eq!(c.recovery(), expected);
            assert_eq!(c.recovery(), expected);
            assert_eq!(c.respawn(), current.anchor);
            assert_eq!(c.snapshot(), before);
        }
        assert_eq!(c.recovery().current.anchor, 30);
        assert_eq!(c.recovery().next, None);
    }

    #[test]
    fn activation_outcomes_cover_stale_duplicate_skipped_and_terminal_triggers() {
        for current_index in 0..4 {
            for index in [0, 1, 2, 3, 4, usize::MAX] {
                let mut c = Checkpoints::new(4, [10, 20, 30, 40]).unwrap();
                c.restore(CheckpointSnapshot {
                    rules: 4,
                    index: current_index,
                })
                .unwrap();
                let mut legacy = c;
                let before = c.snapshot();
                let expected = if index >= 4 {
                    Err(LoopError::Invalid)
                } else if index < current_index {
                    Ok(CheckpointActivation::Stale {
                        index,
                        current_index,
                    })
                } else if index == current_index {
                    Ok(CheckpointActivation::Current { index })
                } else if index == current_index + 1 {
                    Ok(CheckpointActivation::Activated {
                        previous_index: current_index,
                        index,
                    })
                } else {
                    Err(LoopError::Unavailable)
                };
                let accepted = expected
                    .map(|outcome| matches!(outcome, CheckpointActivation::Activated { .. }));
                assert_eq!(c.activate_with_outcome(index), expected);
                assert_eq!(legacy.activate(index), accepted);
                assert_eq!(c.snapshot(), legacy.snapshot());
                if accepted == Ok(true) {
                    assert_eq!(c.index(), index);
                    assert_eq!(c.respawn(), [10, 20, 30, 40][index]);
                } else {
                    assert_eq!(c.snapshot(), before);
                }
            }
        }
    }

    #[test]
    fn trusted_restore_reports_rewinds_advances_and_unchanged_including_terminal() {
        for previous_index in 0..4 {
            for index in 0..4 {
                let mut c = Checkpoints::new(4, [10, 20, 30, 40]).unwrap();
                c.restore(CheckpointSnapshot {
                    rules: 4,
                    index: previous_index,
                })
                .unwrap();
                let mut legacy = c;
                let saved = CheckpointSnapshot { rules: 4, index };
                let expected = if index < previous_index {
                    CheckpointRestore::Rewound {
                        previous_index,
                        index,
                    }
                } else if index == previous_index {
                    CheckpointRestore::Unchanged { index }
                } else {
                    CheckpointRestore::Advanced {
                        previous_index,
                        index,
                    }
                };
                assert_eq!(c.restore_with_outcome(saved), Ok(expected));
                assert_eq!(legacy.restore(saved), Ok(()));
                assert_eq!(c.snapshot(), saved);
                assert_eq!(legacy.snapshot(), saved);
                assert_eq!(c.recovery(), legacy.recovery());
                assert_eq!(c.current_anchor().anchor, [10, 20, 30, 40][index]);
                assert_eq!(
                    c.next_anchor().map(|a| a.index),
                    (index < 3).then_some(index + 1)
                );
                assert_eq!(
                    c.restore_with_outcome(saved),
                    Ok(CheckpointRestore::Unchanged { index })
                );
            }
        }
    }

    #[test]
    fn restore_errors_preserve_progress_anchors_and_fingerprint() {
        for current_index in 0..3 {
            let mut c = Checkpoints::new(4, [10, 20, 30]).unwrap();
            c.restore(CheckpointSnapshot {
                rules: 4,
                index: current_index,
            })
            .unwrap();
            let before = c.snapshot();
            let recovery = c.recovery();
            for saved in [
                CheckpointSnapshot { rules: 0, index: 0 },
                CheckpointSnapshot { rules: 5, index: 2 },
                CheckpointSnapshot {
                    rules: 5,
                    index: usize::MAX,
                },
                CheckpointSnapshot { rules: 4, index: 3 },
                CheckpointSnapshot {
                    rules: 4,
                    index: usize::MAX,
                },
            ] {
                let expected = if saved.rules == 4 {
                    CheckpointRestoreError::InvalidIndex {
                        index: saved.index,
                        anchor_count: 3,
                    }
                } else {
                    CheckpointRestoreError::RulesMismatch {
                        expected: 4,
                        actual: saved.rules,
                    }
                };
                assert_eq!(c.restore_with_outcome(saved), Err(expected));
                assert_eq!(c.snapshot(), before);
                assert_eq!(c.recovery(), recovery);
                assert_eq!(c.restore(saved), Err(LoopError::Invalid));
                assert_eq!(c.snapshot(), before);
                assert_eq!(c.recovery(), recovery);
            }
        }
    }

    #[test]
    fn restoring_an_earlier_save_reopens_the_next_anchor() {
        let mut c = Checkpoints::new(4, [10, 20, 30]).unwrap();
        c.activate(1).unwrap();
        let old_save = c.snapshot();
        c.activate(2).unwrap();
        assert_eq!(c.next_anchor(), None);
        assert_eq!(
            c.activate_with_outcome(old_save.index),
            Ok(CheckpointActivation::Stale {
                index: 1,
                current_index: 2,
            })
        );
        assert_eq!(c.index(), 2);
        assert_eq!(
            c.restore_with_outcome(old_save),
            Ok(CheckpointRestore::Rewound {
                previous_index: 2,
                index: 1,
            })
        );
        assert_eq!(
            c.next_anchor(),
            Some(CheckpointAnchor {
                index: 2,
                anchor: 30
            })
        );
        assert_eq!(
            c.activate_with_outcome(2),
            Ok(CheckpointActivation::Activated {
                previous_index: 1,
                index: 2,
            })
        );
    }

    #[test]
    fn single_anchor_and_maximum_route_recover_at_terminal_without_wraparound() {
        let mut single = Checkpoints::new(4, [10]).unwrap();
        assert_eq!(single.next_anchor(), None);
        assert_eq!(
            single.recovery().current,
            CheckpointAnchor {
                index: 0,
                anchor: 10
            }
        );
        assert_eq!(
            single.activate_with_outcome(0),
            Ok(CheckpointActivation::Current { index: 0 })
        );
        assert_eq!(single.activate_with_outcome(1), Err(LoopError::Invalid));
        assert_eq!(
            single.restore_with_outcome(single.snapshot()),
            Ok(CheckpointRestore::Unchanged { index: 0 })
        );

        let mut maximum = Checkpoints::new(4, std::array::from_fn::<_, 256, _>(|i| i)).unwrap();
        for index in 1..256 {
            assert_eq!(
                maximum.next_anchor(),
                Some(CheckpointAnchor {
                    index,
                    anchor: index
                })
            );
            assert!(maximum.activate(index).unwrap());
        }
        assert_eq!(maximum.recovery().current.index, 255);
        assert_eq!(maximum.respawn(), 255);
        assert_eq!(maximum.next_anchor(), None);
        assert_eq!(maximum.activate_with_outcome(256), Err(LoopError::Invalid));
        assert_eq!(maximum.index(), 255);
        assert!(matches!(
            Checkpoints::<u8, 0>::new(4, []),
            Err(LoopError::Invalid)
        ));
        assert!(matches!(
            Checkpoints::new(4, [0; 257]),
            Err(LoopError::Invalid)
        ));
        assert!(matches!(Checkpoints::new(0, [10]), Err(LoopError::Invalid)));
    }
}
