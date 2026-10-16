//! Bounded local engine diagnostics with severity filtering and consecutive dedup.
//! Codes are 1..=64 bytes, messages <=1024 bytes; no transport or logging backend.
//! Capacity counts records. Repeated equal records retain first/last host tick.
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Trace,
    Info,
    Warning,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub first_tick: u64,
    pub last_tick: u64,
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub repeats: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticError {
    InvalidText,
}
pub struct Diagnostics {
    records: VecDeque<Diagnostic>,
    capacity: usize,
    minimum: Severity,
    evicted: u64,
}
impl Diagnostics {
    pub fn new(capacity: usize, minimum: Severity) -> Self {
        Self {
            records: VecDeque::new(),
            capacity,
            minimum,
            evicted: 0,
        }
    }
    pub fn set_minimum(&mut self, minimum: Severity) {
        self.minimum = minimum;
    }
    /// False means filtered out or disabled (zero capacity); invalid text is rejected.
    pub fn record(
        &mut self,
        tick: u64,
        severity: Severity,
        code: &str,
        message: &str,
    ) -> Result<bool, DiagnosticError> {
        if code.is_empty() || code.len() > 64 || message.len() > 1024 {
            return Err(DiagnosticError::InvalidText);
        }
        if severity < self.minimum || self.capacity == 0 {
            return Ok(false);
        }
        if let Some(last) = self.records.back_mut().filter(|last| {
            last.severity == severity
                && last.code == code
                && last.message == message
                && tick >= last.last_tick
        }) {
            last.last_tick = tick;
            last.repeats = last.repeats.saturating_add(1);
            return Ok(true);
        }
        if self.records.len() == self.capacity {
            self.records.pop_front();
            self.evicted = self.evicted.saturating_add(1);
        }
        self.records.push_back(Diagnostic {
            first_tick: tick,
            last_tick: tick,
            severity,
            code: code.into(),
            message: message.into(),
            repeats: 1,
        });
        Ok(true)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.records.iter()
    }
    pub fn evicted(&self) -> u64 {
        self.evicted
    }
    pub fn clear(&mut self) {
        self.records.clear();
        self.evicted = 0;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filtering_dedup_eviction_and_bounded_text() {
        let mut d = Diagnostics::new(2, Severity::Warning);
        assert_eq!(d.record(0, Severity::Info, "i", "hidden"), Ok(false));
        d.record(1, Severity::Warning, "w", "slow").unwrap();
        d.record(2, Severity::Warning, "w", "slow").unwrap();
        let r = d.iter().next().unwrap();
        assert_eq!((r.first_tick, r.last_tick, r.repeats), (1, 2, 2));
        assert_eq!(
            d.record(3, Severity::Error, "", "invalid"),
            Err(DiagnosticError::InvalidText)
        );
        assert_eq!(d.iter().count(), 1);
        d.record(3, Severity::Error, "e", "failed").unwrap();
        d.record(4, Severity::Warning, "w", "slow").unwrap();
        assert_eq!(d.evicted(), 1);
        assert_eq!(
            d.iter().map(|r| r.code.as_str()).collect::<Vec<_>>(),
            ["e", "w"]
        );
        d.clear();
        assert_eq!(d.iter().count(), 0);
        assert_eq!(d.evicted(), 0);
        assert_eq!(
            Diagnostics::new(0, Severity::Trace).record(0, Severity::Error, "e", "x"),
            Ok(false)
        );
    }
}
