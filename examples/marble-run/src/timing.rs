//! Trial comparisons use the existing record schema. Checkpoint references are
//! bounded session data, never added to saved records or guessed from total time.
use crate::{courses::COURSES, level::MAX_CHECKPOINTS, sim::Run};
use ferravex_engine::persistence::{RecordBook, RecordComparison, RecordSummary};
use ferravex_render::storage::{LoadStatus, Slot, StorageError, run_micros};

#[derive(Clone, Copy)]
struct SplitRecord {
    context: u64,
    finish_us: u64,
    splits: [Option<u64>; MAX_CHECKPOINTS],
}

pub(crate) struct TrialRecords {
    slot: Slot<RecordBook>,
    pub book: RecordBook,
    error: Option<StorageError>,
    history: [Option<SplitRecord>; COURSES.len()],
    reference: Option<SplitRecord>,
    result: Option<RecordSummary>,
}

impl TrialRecords {
    pub fn new() -> Self {
        // Same owner, slot, schema and version as the previous Records adapter.
        let slot = Slot::open("marble-run", "records", RecordBook::default()).unwrap();
        let error = match slot.status() {
            LoadStatus::Rejected(error) => Some(error),
            _ => None,
        };
        Self {
            book: slot.value().clone(),
            slot,
            error,
            history: [None; COURSES.len()],
            reference: None,
            result: None,
        }
    }

    pub fn begin(&mut self, course: usize, context: u64) {
        self.reference = self.history[course].filter(|r| r.context == context);
        self.result = None;
    }

    pub fn error(&self) -> Option<StorageError> {
        self.error
    }

    pub fn finish(&mut self, course: usize, context: u64, run: &Run) {
        if !run.finished || self.result.is_some() {
            return;
        }
        let c = &COURSES[course];
        let score = match c.medal(run.elapsed, run.falls) {
            "GOLD" => 3,
            "SILVER" => 2,
            _ => 1,
        };
        match self
            .book
            .record_with_summary(c.id, context, score, run_micros(run.elapsed))
        {
            Ok(summary) => {
                self.result = Some(summary);
                if self.slot.status() != LoadStatus::Transient {
                    // Keep the session best when storage is unavailable/protected.
                    // Writes happen only for accepted finishes, never each frame.
                    self.error = self.slot.commit(self.book.clone()).err();
                }
            }
            Err(error) => self.error = Some(error.into()),
        }
        if let Some(finish_us) = run_micros(run.elapsed)
            && self.history[course].is_none_or(|r| r.context != context || finish_us < r.finish_us)
        {
            self.history[course] = Some(SplitRecord {
                context,
                finish_us,
                splits: run.splits,
            });
        }
    }

    pub fn split_label(&self, run: &Run) -> Option<String> {
        let index = run.splits.iter().rposition(Option::is_some)?;
        let time = run.splits[index]?;
        let mut label = format!("CP {} SPLIT {:.2}s", index + 1, time as f64 / 1e6);
        if let Some(previous) = self.reference.and_then(|r| r.splits[index]) {
            label.push_str(&format!(" / {} SESSION", delta(time, previous)));
        }
        Some(label)
    }

    pub fn finish_label(&self) -> String {
        let Some(time) = self.result.and_then(|r| r.time_us) else {
            return "TIME COMPARISON UNAVAILABLE".into();
        };
        match (time.comparison, time.previous) {
            (_, None) => "FIRST RECORDED TIME".into(),
            (RecordComparison::Tie, _) => "MATCHED BEST TIME".into(),
            (comparison, Some(previous)) => format!(
                "{} / {}",
                if comparison.is_new_best() {
                    "NEW BEST"
                } else {
                    "VS BEST"
                },
                delta(time.value, previous),
            ),
        }
    }
}

fn delta(time: u64, previous: u64) -> String {
    // Widen before subtraction, preserving both signs without unsigned wrap.
    format!(
        "{:+.2}s",
        (i128::from(time) - i128::from(previous)) as f64 / 1e6
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Slot::with_backend with an explicitly transient store avoids host files.
    fn records() -> TrialRecords {
        let slot = Slot::with_backend(
            "marble-run-test",
            "records",
            RecordBook::default(),
            ferravex_render::storage::PlatformStore::transient(),
        )
        .unwrap();
        TrialRecords {
            book: RecordBook::default(),
            slot,
            error: None,
            history: [None; COURSES.len()],
            reference: None,
            result: None,
        }
    }

    #[test]
    fn finish_compares_prior_best_once_and_keeps_score_and_time_independent() {
        let mut records = records();
        let mut run = Run::new();
        run.elapsed = 30.;
        records.begin(0, 1);
        records.finish(0, 1, &run);
        assert!(records.book.entries().is_empty());
        run.finished = true;
        records.finish(0, 1, &run);
        records.finish(0, 1, &run);
        assert_eq!(records.finish_label(), "FIRST RECORDED TIME");
        assert_eq!(records.book.get("aqueduct", 1).unwrap().runs, 1);
        run.elapsed = 29.;
        run.falls = 3;
        records.begin(0, 1);
        records.finish(0, 1, &run);
        assert_eq!(records.finish_label(), "NEW BEST / -1.00s");
        assert_eq!(records.book.get("aqueduct", 1).unwrap().score, 3);
        records.begin(0, 1);
        records.finish(0, 1, &run);
        assert_eq!(records.finish_label(), "MATCHED BEST TIME");
        run.elapsed = 31.;
        records.begin(0, 1);
        records.finish(0, 1, &run);
        assert_eq!(records.finish_label(), "VS BEST / +2.00s");
        assert_eq!(
            records.book.get("aqueduct", 1).unwrap().fastest_us,
            Some(29_000_000)
        );
    }

    #[test]
    fn splits_use_completed_session_best_and_never_cross_content_versions() {
        let mut records = records();
        let mut run = Run::new();
        run.splits[0] = Some(10_000_000);
        run.elapsed = 30.;
        run.finished = true;
        records.begin(0, 1);
        assert_eq!(records.split_label(&run).unwrap(), "CP 1 SPLIT 10.00s");
        records.finish(0, 1, &run);
        records.begin(0, 1);
        run.splits[0] = Some(9_000_000);
        assert_eq!(
            records.split_label(&run).unwrap(),
            "CP 1 SPLIT 9.00s / -1.00s SESSION"
        );
        run.elapsed = 35.; // Faster first split in a slower run is not the reference.
        records.finish(0, 1, &run);
        records.begin(0, 1);
        assert!(
            records
                .split_label(&run)
                .unwrap()
                .contains("-1.00s SESSION")
        );
        run.splits[1] = Some(20_000_000); // No invented reference for a skipped gate.
        assert_eq!(records.split_label(&run).unwrap(), "CP 2 SPLIT 20.00s");
        records.begin(0, 2);
        run.splits[1] = None;
        assert_eq!(records.split_label(&run).unwrap(), "CP 1 SPLIT 9.00s");
        records.begin(1, 1);
        assert_eq!(records.split_label(&run).unwrap(), "CP 1 SPLIT 9.00s");
    }

    #[test]
    fn summaries_keep_legacy_save_bytes_and_protected_storage_keeps_session_best() {
        use ferravex_engine::persistence::{SaveData, SaveError};
        let mut records = records();
        let mut run = Run::new();
        run.finished = true;
        run.elapsed = 30.;
        run.splits[0] = Some(10_000_000);
        records.begin(0, 1);
        records.finish(0, 1, &run);
        let mut legacy = RecordBook::default();
        legacy.record("aqueduct", 1, 3, Some(30_000_000)).unwrap();
        assert_eq!(records.book.encode(7).unwrap(), legacy.encode(7).unwrap());
        // Rejected files remain protected; the same Slot API gates real writes.
        records
            .slot
            .validate_loaded::<()>(|_| Err(SaveError::InvalidData))
            .unwrap_err();
        records.begin(0, 1);
        run.elapsed = 29.;
        records.finish(0, 1, &run);
        assert_eq!(records.error(), Some(StorageError::Protected));
        assert_eq!(records.slot.generation(), 0);
        assert_eq!(
            records.book.get("aqueduct", 1).unwrap().fastest_us,
            Some(29_000_000)
        );
        assert_eq!(records.finish_label(), "NEW BEST / -1.00s");
    }
}
