use super::*;
pub use ferravex_play::persistence::{Record, RecordBook, run_micros};
/// Per-game records cached in memory. Failed persistence still updates the session
/// best; the error remains available and retries occur only on explicit attempts.
pub struct Records {
    slot: Slot<RecordBook>,
    book: RecordBook,
    error: Option<StorageError>,
}
impl Records {
    pub fn new(owner: &str) -> Result<Self, StorageError> {
        let slot = Slot::open(owner, "records", RecordBook::default())?;
        let book = slot.value().clone();
        let error = if let LoadStatus::Rejected(e) = slot.status() {
            Some(e)
        } else {
            None
        };
        Ok(Self { slot, book, error })
    }
    pub fn get(&self, id: &str, context: u64) -> Option<&Record> {
        self.book.get(id, context)
    }
    pub fn error(&self) -> Option<StorageError> {
        self.error
    }
    pub fn attempt(
        &mut self,
        id: &str,
        context: u64,
        score: u64,
        time_us: Option<u64>,
    ) -> Result<(), StorageError> {
        self.book.record(id, context, score, time_us)?;
        if self.slot.status() == LoadStatus::Transient {
            return Ok(());
        }
        let result = self.slot.commit(self.book.clone());
        self.error = result.err();
        result
    }
}
