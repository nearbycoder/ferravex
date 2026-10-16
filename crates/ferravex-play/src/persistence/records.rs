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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordBook {
    entries: Vec<Record>,
}
impl RecordBook {
    pub fn entries(&self) -> &[Record] {
        &self.entries
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
        if !key_valid(id) || id.len() > 48 || time_us.is_some_and(|v| v == 0 || v > MAX_RUN_US) {
            return Err(SaveError::InvalidData);
        }
        if let Some(v) = self
            .entries
            .iter_mut()
            .find(|v| v.id == id && v.context == context)
        {
            let runs = v.runs.checked_add(1).ok_or(SaveError::Capacity)?;
            v.runs = runs;
            v.score = v.score.max(score);
            if let Some(time) = time_us {
                v.fastest_us = Some(v.fastest_us.map_or(time, |old| old.min(time)));
            }
        } else {
            if self.entries.len() == MAX_RECORDS {
                return Err(SaveError::Capacity);
            }
            self.entries.push(Record {
                id: id.into(),
                context,
                score,
                fastest_us: time_us,
                runs: 1,
            });
        }
        Ok(())
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
