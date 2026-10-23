use super::*;
pub const MAX_RECORDS: usize = 24;
pub const MAX_RUN_US: u64 = 7 * 24 * 60 * 60 * 1_000_000;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub id: String,
    pub context: u64,
    pub score: u64,
    pub fastest_us: Option<u64>,
    pub runs: u64,
}
/// An attempt's relation to the previous best for one metric.
///
/// Scores prefer larger values; successful times prefer smaller values. A first
/// score or first successful time is `NewBest`, even when the score is zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordComparison {
    NewBest,
    Tie,
    Worse,
}
impl RecordComparison {
    /// Whether this metric establishes or improves a best.
    ///
    /// ```
    /// use ferravex_play::persistence::RecordBook;
    /// let summary = RecordBook::default().compare("course", 1, 0, None)?;
    /// assert!(summary.score.comparison.is_new_best());
    /// # Ok::<(), ferravex_play::persistence::SaveError>(())
    /// ```
    pub const fn is_new_best(self) -> bool {
        matches!(self, Self::NewBest)
    }
}
/// Comparison of an attempted value with its previous best, before acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordDelta {
    pub comparison: RecordComparison,
    /// `None` means that this metric has no previous best.
    pub previous: Option<u64>,
    /// The attempted score or successful time in microseconds.
    pub value: u64,
    /// Exact `value - previous`, or `None` without a previous best.
    ///
    /// Positive means a higher score or a slower time; negative means a lower
    /// score or a faster time. The subtraction widens to `i128` and is checked,
    /// so scores across the entire `u64` range are supported without saturation.
    pub delta: Option<i128>,
}
impl RecordDelta {
    fn compare(
        value: u64,
        previous: Option<u64>,
        lower_is_better: bool,
    ) -> Result<Self, SaveError> {
        let comparison = match previous {
            None => RecordComparison::NewBest,
            Some(old) if value == old => RecordComparison::Tie,
            Some(old) if (value < old) == lower_is_better => RecordComparison::NewBest,
            Some(_) => RecordComparison::Worse,
        };
        let delta = previous
            .map(|old| {
                i128::from(value)
                    .checked_sub(i128::from(old))
                    .ok_or(SaveError::Capacity)
            })
            .transpose()?;
        Ok(Self {
            comparison,
            previous,
            value,
            delta,
        })
    }
}
/// Independent score/time comparisons for one accepted or previewed attempt.
///
/// Best score and fastest time may come from different attempts. No combined
/// ranking is implied, and this summary is never serialized into a record book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordSummary {
    pub score: RecordDelta,
    /// `None` when the attempt supplies no successful finish time. This leaves
    /// any existing fastest time unchanged and does not count as a tie or worse.
    pub time_us: Option<RecordDelta>,
    /// Attempt count after accepting this attempt, including ties and worse runs.
    pub runs: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordBook {
    entries: Vec<Record>,
}
impl RecordBook {
    /// Entries in insertion order, which is also their serialized index order.
    pub fn entries(&self) -> &[Record] {
        &self.entries
    }
    /// A view ordered lexicographically by ID, then by ascending numeric context.
    ///
    /// Allocates at most [`MAX_RECORDS`] references. Does not reorder the book or
    /// its serialized entries; score/time ties do not affect identity ordering.
    ///
    /// ```
    /// use ferravex_play::persistence::RecordBook;
    /// let mut book = RecordBook::default();
    /// book.record("z-course", 1, 10, None)?;
    /// book.record("a-course", 2, 10, None)?;
    /// book.record("a-course", 1, 10, None)?;
    /// let keys: Vec<_> = book.ordered_entries().into_iter()
    ///     .map(|record| (record.id.as_str(), record.context)).collect();
    /// assert_eq!(keys, [("a-course", 1), ("a-course", 2), ("z-course", 1)]);
    /// assert_eq!(book.entries()[0].id, "z-course");
    /// # Ok::<(), ferravex_play::persistence::SaveError>(())
    /// ```
    pub fn ordered_entries(&self) -> Vec<&Record> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_unstable_by(|a, b| a.id.cmp(&b.id).then_with(|| a.context.cmp(&b.context)));
        entries
    }
    pub fn get(&self, id: &str, context: u64) -> Option<&Record> {
        self.entries
            .iter()
            .find(|v| v.id == id && v.context == context)
    }
    /// Record one completed attempt. Only supply time for a successful finish.
    /// Score/time maxima and context identity never pass through floating point.
    pub fn record(
        &mut self,
        id: &str,
        context: u64,
        score: u64,
        time_us: Option<u64>,
    ) -> Result<(), SaveError> {
        self.record_with_summary(id, context, score, time_us)
            .map(|_| ())
    }
    /// Preview an attempt against the current bests without mutation or allocation.
    ///
    /// Uses exactly the acceptance checks of [`Self::record`]: malformed IDs and
    /// zero/out-of-range times return [`SaveError::InvalidData`]; a new identity
    /// in a full book or an overflowing run count returns [`SaveError::Capacity`].
    /// A full book can still accept attempts for an existing identity. `None`
    /// supplies no successful time; the score and attempt count still apply.
    ///
    /// A preview does not reserve an attempt. Use [`Self::record_with_summary`]
    /// to emit feedback from the summary of an actually accepted attempt.
    ///
    /// ```
    /// use ferravex_play::persistence::RecordBook;
    /// let mut book = RecordBook::default();
    /// book.record("course", 1, 100, Some(2_000_000))?;
    /// let preview = book.compare("course", 1, 90, Some(1_500_000))?;
    /// assert_eq!(preview.score.delta, Some(-10));
    /// assert!(!preview.score.comparison.is_new_best());
    /// assert_eq!(preview.time_us.unwrap().delta, Some(-500_000));
    /// assert!(preview.time_us.unwrap().comparison.is_new_best());
    /// assert_eq!(preview.runs, 2);
    /// assert_eq!(book.get("course", 1).unwrap().runs, 1);
    /// # Ok::<(), ferravex_play::persistence::SaveError>(())
    /// ```
    pub fn compare(
        &self,
        id: &str,
        context: u64,
        score: u64,
        time_us: Option<u64>,
    ) -> Result<RecordSummary, SaveError> {
        if !key_valid(id) || id.len() > 48 || time_us.is_some_and(|v| v == 0 || v > MAX_RUN_US) {
            return Err(SaveError::InvalidData);
        }
        let previous = self.get(id, context);
        let runs = match previous {
            Some(record) => record.runs.checked_add(1).ok_or(SaveError::Capacity)?,
            None if self.entries.len() == MAX_RECORDS => return Err(SaveError::Capacity),
            None => 1,
        };
        Ok(RecordSummary {
            score: RecordDelta::compare(score, previous.map(|record| record.score), false)?,
            time_us: time_us
                .map(|time| {
                    RecordDelta::compare(time, previous.and_then(|record| record.fastest_us), true)
                })
                .transpose()?,
            runs,
        })
    }
    /// Accept one attempt and return its comparisons against the previous bests.
    ///
    /// Uses the same validation and independent best updates as [`Self::record`].
    /// All checks finish before mutation: errors leave the entire book unchanged.
    /// Ties and worse runs are accepted and increment `runs` without replacing
    /// their respective bests. Summary calculation allocates no storage; a new
    /// identity retains the existing bounded entry/ID allocation behavior.
    ///
    /// ```
    /// use ferravex_play::persistence::RecordBook;
    /// let mut book = RecordBook::default();
    /// let first = book.record_with_summary("course", 1, 0, None)?;
    /// assert!(first.score.comparison.is_new_best());
    /// assert_eq!(first.score.delta, None);
    /// let next = book.record_with_summary("course", 1, u64::MAX, Some(1))?;
    /// assert_eq!(next.score.delta, Some(i128::from(u64::MAX)));
    /// assert_eq!(next.time_us.unwrap().previous, None);
    /// assert_eq!(book.get("course", 1).unwrap().runs, 2);
    /// # Ok::<(), ferravex_play::persistence::SaveError>(())
    /// ```
    pub fn record_with_summary(
        &mut self,
        id: &str,
        context: u64,
        score: u64,
        time_us: Option<u64>,
    ) -> Result<RecordSummary, SaveError> {
        let summary = self.compare(id, context, score, time_us)?;
        if let Some(v) = self
            .entries
            .iter_mut()
            .find(|v| v.id == id && v.context == context)
        {
            v.runs = summary.runs;
            v.score = v.score.max(score);
            if let Some(time) = time_us {
                v.fastest_us = Some(v.fastest_us.map_or(time, |old| old.min(time)));
            }
        } else {
            self.entries.push(Record {
                id: id.into(),
                context,
                score,
                fastest_us: time_us,
                runs: summary.runs,
            });
        }
        Ok(summary)
    }
}
impl SaveData for RecordBook {
    const SCHEMA: u64 = 0x46585245434f5244;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 16384;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        if self.entries.len() > MAX_RECORDS {
            return Err(SaveError::Capacity);
        }
        d.set("count", Value::U64(self.entries.len() as u64))?;
        for (i, v) in self.entries.iter().enumerate() {
            d.set(&format!("id.{i}"), Value::Text(v.id.clone()))?;
            d.set(&format!("context.{i}"), Value::U64(v.context))?;
            d.set(&format!("score.{i}"), Value::U64(v.score))?;
            d.set(&format!("time.{i}"), Value::U64(v.fastest_us.unwrap_or(0)))?;
            d.set(&format!("runs.{i}"), Value::U64(v.runs))?;
        }
        Ok(())
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let count = d.u64("count")?;
        if count > MAX_RECORDS as u64 {
            return Err(SaveError::Capacity);
        }
        let mut book = Self {
            entries: Vec::with_capacity(count as usize),
        };
        for i in 0..count {
            let id = d.text(&format!("id.{i}"))?;
            let context = d.u64(&format!("context.{i}"))?;
            let score = d.u64(&format!("score.{i}"))?;
            let time = d.u64(&format!("time.{i}"))?;
            let runs = d.u64(&format!("runs.{i}"))?;
            if runs == 0 || book.get(id, context).is_some() {
                return Err(SaveError::InvalidData);
            }
            book.record(id, context, score, (time != 0).then_some(time))?;
            book.entries.last_mut().unwrap().runs = runs;
        }
        Ok(book)
    }
}
/// Convert a bounded finite game timer without accepting NaN/negative/zero runs.
pub fn run_micros(seconds: f32) -> Option<u64> {
    (seconds.is_finite() && seconds > 0. && seconds <= 604800.)
        .then(|| ((seconds as f64 * 1_000_000.).round() as u64).max(1))
}
#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rejected(
        book: &mut RecordBook,
        id: &str,
        context: u64,
        score: u64,
        time_us: Option<u64>,
        error: SaveError,
    ) {
        let before = book.clone();
        let bytes = book.encode(7).unwrap();
        assert_eq!(book.compare(id, context, score, time_us), Err(error));
        assert_eq!(
            book.record_with_summary(id, context, score, time_us),
            Err(error)
        );
        assert_eq!(book.record(id, context, score, time_us), Err(error));
        assert_eq!(*book, before);
        assert_eq!(book.encode(7).unwrap(), bytes);
    }

    #[test]
    fn first_bests_ties_and_absent_times_have_distinct_summaries() {
        let mut book = RecordBook::default();
        let first = book.record_with_summary("course", 1, 0, None).unwrap();
        assert_eq!(
            first,
            RecordSummary {
                score: RecordDelta {
                    comparison: RecordComparison::NewBest,
                    previous: None,
                    value: 0,
                    delta: None,
                },
                time_us: None,
                runs: 1,
            }
        );
        let tie = book.record_with_summary("course", 1, 0, None).unwrap();
        assert_eq!(tie.score.comparison, RecordComparison::Tie);
        assert!(!tie.score.comparison.is_new_best());
        assert_eq!(tie.score.previous, Some(0));
        assert_eq!(tie.score.delta, Some(0));
        assert_eq!(tie.time_us, None);
        assert_eq!(tie.runs, 2);

        let finish = book.record_with_summary("course", 1, 1, Some(10)).unwrap();
        assert_eq!(
            finish.time_us,
            Some(RecordDelta {
                comparison: RecordComparison::NewBest,
                previous: None,
                value: 10,
                delta: None,
            })
        );
        let no_finish = book.record_with_summary("course", 1, 0, None).unwrap();
        assert_eq!(no_finish.score.comparison, RecordComparison::Worse);
        assert!(!no_finish.score.comparison.is_new_best());
        assert_eq!(no_finish.time_us, None);
        let stored = book.get("course", 1).unwrap();
        assert_eq!(
            (stored.score, stored.fastest_us, stored.runs),
            (1, Some(10), 4)
        );

        for (id, context) in [("course", 2), ("other", 1)] {
            let separate = book.compare(id, context, 0, Some(20)).unwrap();
            assert!(separate.score.comparison.is_new_best());
            assert!(separate.time_us.unwrap().comparison.is_new_best());
            assert_eq!(separate.score.previous, None);
            assert_eq!(separate.time_us.unwrap().delta, None);
            assert_eq!(separate.runs, 1);
        }
    }

    #[test]
    fn preview_and_acceptance_compare_score_and_time_independently() {
        let mut baseline = RecordBook::default();
        baseline.record("course", 1, 100, Some(50)).unwrap();
        let before = baseline.encode(7).unwrap();
        for (score, score_comparison) in [
            (90, RecordComparison::Worse),
            (100, RecordComparison::Tie),
            (110, RecordComparison::NewBest),
        ] {
            for (time, time_comparison) in [
                (40, RecordComparison::NewBest),
                (50, RecordComparison::Tie),
                (60, RecordComparison::Worse),
            ] {
                let preview = baseline.compare("course", 1, score, Some(time)).unwrap();
                assert_eq!(
                    preview.score,
                    RecordDelta {
                        comparison: score_comparison,
                        previous: Some(100),
                        value: score,
                        delta: Some(i128::from(score) - 100),
                    }
                );
                assert_eq!(
                    preview.time_us,
                    Some(RecordDelta {
                        comparison: time_comparison,
                        previous: Some(50),
                        value: time,
                        delta: Some(i128::from(time) - 50),
                    })
                );
                assert_eq!(preview.runs, 2);
                assert_eq!(baseline.encode(7).unwrap(), before);

                let mut accepted = baseline.clone();
                assert_eq!(
                    accepted.record_with_summary("course", 1, score, Some(time)),
                    Ok(preview)
                );
                let stored = accepted.get("course", 1).unwrap();
                assert_eq!(stored.score, 100.max(score));
                assert_eq!(stored.fastest_us, Some(50.min(time)));
                assert_eq!(stored.runs, 2);
                let mut legacy = baseline.clone();
                legacy.record("course", 1, score, Some(time)).unwrap();
                assert_eq!(legacy, accepted);
            }
        }
    }

    #[test]
    fn deltas_cover_integer_extremes_without_loss_or_wraparound() {
        let mut book = RecordBook::default();
        book.record("course", u64::MAX, 0, Some(MAX_RUN_US))
            .unwrap();
        let largest = book
            .record_with_summary("course", u64::MAX, u64::MAX, Some(1))
            .unwrap();
        assert_eq!(largest.score.delta, Some(i128::from(u64::MAX)));
        assert!(largest.score.comparison.is_new_best());
        assert_eq!(
            largest.time_us.unwrap().delta,
            Some(1 - i128::from(MAX_RUN_US))
        );
        assert!(largest.time_us.unwrap().comparison.is_new_best());
        let smallest = book
            .compare("course", u64::MAX, 0, Some(MAX_RUN_US))
            .unwrap();
        assert_eq!(smallest.score.delta, Some(-i128::from(u64::MAX)));
        assert_eq!(smallest.score.comparison, RecordComparison::Worse);
        assert_eq!(
            smallest.time_us.unwrap().delta,
            Some(i128::from(MAX_RUN_US) - 1)
        );
        assert_eq!(
            smallest.time_us.unwrap().comparison,
            RecordComparison::Worse
        );

        let precise = (1_u64 << 53) + 1;
        book.record("large-score", 0, precise, None).unwrap();
        assert_eq!(
            book.compare("large-score", 0, precise + 1, None)
                .unwrap()
                .score
                .delta,
            Some(1)
        );
        assert_eq!(
            book.compare("large-score", 0, precise - 1, None)
                .unwrap()
                .score
                .delta,
            Some(-1)
        );
    }

    #[test]
    fn invalid_attempts_and_full_books_preserve_acceptance_rules() {
        let mut book = RecordBook::default();
        book.record("course", 1, 10, Some(10)).unwrap();
        for id in ["", "Course", "a/b", "a b", "café", &"a".repeat(49)] {
            assert_rejected(&mut book, id, 1, 20, Some(5), SaveError::InvalidData);
        }
        for time in [0, MAX_RUN_US + 1, u64::MAX] {
            assert_rejected(
                &mut book,
                "course",
                1,
                20,
                Some(time),
                SaveError::InvalidData,
            );
        }
        book.record_with_summary(&"a".repeat(48), 0, u64::MAX, Some(MAX_RUN_US))
            .unwrap();
        for context in 2..MAX_RECORDS as u64 {
            book.record("course", context, 10, None).unwrap();
        }
        assert_eq!(book.entries().len(), MAX_RECORDS);
        assert_rejected(&mut book, "extra", 0, 0, None, SaveError::Capacity);
        assert_rejected(&mut book, "course", u64::MAX, 0, None, SaveError::Capacity);
        // Input errors retain precedence over capacity failures.
        assert_rejected(&mut book, "extra", 0, 0, Some(0), SaveError::InvalidData);
        let accepted = book.record_with_summary("course", 1, 20, Some(5)).unwrap();
        assert_eq!(accepted.runs, 2);
        assert_eq!(book.get("course", 1).unwrap().score, 20);
    }

    #[test]
    fn decoded_run_count_exhaustion_is_checked_before_any_best_changes() {
        let mut book = RecordBook::default();
        book.record("course", 1, 1, Some(10)).unwrap();
        let mut document = Document::decode(&book.encode(1).unwrap()).unwrap();
        document.set("runs.0", Value::U64(u64::MAX - 1)).unwrap();
        let mut book = RecordBook::decode(&document.encode()).unwrap().0;
        let last = book.record_with_summary("course", 1, 2, Some(9)).unwrap();
        assert_eq!(last.runs, u64::MAX);
        assert_rejected(&mut book, "course", 1, 3, Some(8), SaveError::Capacity);
        assert_rejected(&mut book, "course", 1, 3, Some(0), SaveError::InvalidData);
        // Exhaustion is per identity, not global to the book.
        assert_eq!(
            book.record_with_summary("course", 2, 3, Some(8))
                .unwrap()
                .runs,
            1
        );
    }

    #[test]
    fn ordered_view_is_canonical_without_changing_version_one_bytes() {
        let attempts = [
            ("z", u64::MAX, 0, None),
            ("a", 10, 100, Some(20)),
            ("a", 2, u64::MAX, Some(MAX_RUN_US)),
            ("a", 0, 0, Some(1)),
        ];
        let mut book = RecordBook::default();
        let mut reversed = RecordBook::default();
        // Construct the pre-summary v1 document independently: no new fields.
        let mut expected = Document::new(0x46585245434f5244, 1, 9).unwrap();
        expected
            .set("count", Value::U64(attempts.len() as u64))
            .unwrap();
        for (i, &(id, context, score, time)) in attempts.iter().enumerate() {
            book.record_with_summary(id, context, score, time).unwrap();
            expected
                .set(&format!("id.{i}"), Value::Text(id.into()))
                .unwrap();
            expected
                .set(&format!("context.{i}"), Value::U64(context))
                .unwrap();
            expected
                .set(&format!("score.{i}"), Value::U64(score))
                .unwrap();
            expected
                .set(&format!("time.{i}"), Value::U64(time.unwrap_or(0)))
                .unwrap();
            expected.set(&format!("runs.{i}"), Value::U64(1)).unwrap();
        }
        for &(id, context, score, time) in attempts.iter().rev() {
            reversed.record(id, context, score, time).unwrap();
        }
        assert_eq!(book.encode(9).unwrap(), expected.encode());
        assert_eq!(book.ordered_entries(), reversed.ordered_entries());
        let keys: Vec<_> = book
            .ordered_entries()
            .into_iter()
            .map(|record| (record.id.as_str(), record.context))
            .collect();
        assert_eq!(keys, [("a", 0), ("a", 2), ("a", 10), ("z", u64::MAX)]);
        assert_eq!(book.entries()[0].id, "z");
        assert_eq!(book.encode(9).unwrap(), expected.encode());
        let decoded = RecordBook::decode(&expected.encode()).unwrap().0;
        assert_eq!(decoded, book);
        assert_eq!(decoded.ordered_entries(), book.ordered_entries());
        assert_eq!(
            decoded.compare("a", 2, 0, None),
            book.compare("a", 2, 0, None)
        );
        assert!(RecordBook::default().ordered_entries().is_empty());
    }

    #[test]
    fn contexts_scores_and_fastest_times_survive_save_and_worse_runs() {
        let mut b = RecordBook::default();
        b.record("track-one", 1, 50, Some(20_000_000)).unwrap();
        b.record("track-one", 1, 40, Some(21_000_000)).unwrap();
        b.record("track-one", 2, 2, Some(30_000_000)).unwrap();
        assert_eq!(b.get("track-one", 1).unwrap().score, 50);
        assert_eq!(b.get("track-one", 1).unwrap().fastest_us, Some(20_000_000));
        assert_eq!(b.get("track-one", 1).unwrap().runs, 2);
        assert_eq!(RecordBook::decode(&b.encode(1).unwrap()).unwrap().0, b);
        let before = b.clone();
        assert!(b.record("track-one", 1, 100, Some(MAX_RUN_US + 1)).is_err());
        assert_eq!(b, before);
        assert!(run_micros(f32::NAN).is_none());
        assert!(run_micros(-1.).is_none());
        for i in 0..MAX_RECORDS - 2 {
            b.record(&format!("level-{i}"), 1, u64::MAX, None).unwrap();
        }
        assert!(b.record("extra", 1, 0, None).is_err());
        assert_eq!(RecordBook::decode(&b.encode(2).unwrap()).unwrap().0, b);
    }
}
