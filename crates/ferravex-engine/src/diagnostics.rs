//! Bounded local engine diagnostics with severity filtering and consecutive dedup.
//! Codes are 1..=64 bytes, messages <=1024 bytes; no transport or logging backend.
//! Capacity counts records. Repeated equal records retain first/last host tick.
//!
//! Attach source coordinates, an operation and a suggested fix with
//! [`Diagnostics::record_with_context`](crate::diagnostics::Diagnostics::record_with_context). Context is copied only when a new record
//! is retained and participates in deduplication. All text limits count UTF-8
//! bytes, and caller-provided diagnostic codes are preserved verbatim.
//!
//! ```
//! use ferravex_engine::diagnostics::{
//!     DiagnosticContext, DiagnosticSource, Diagnostics, Severity,
//! };
//!
//! let mut diagnostics = Diagnostics::new(32, Severity::Warning);
//! diagnostics.record_with_context(12, Severity::Error, "scene.invalid_mass",
//!     "mass must be positive", DiagnosticContext {
//!         source: Some(DiagnosticSource { file: "levels/arena.scene", line: 8, column: 5 }),
//!         operation: Some("load_scene"),
//!         help: Some("Set mass to a finite number greater than zero."),
//!     })?;
//! let json = diagnostics.to_json();
//! assert!(json.starts_with("{\"schema_version\":1,"));
//! // Formatting streams the same JSON without an intermediate String.
//! assert_eq!(format!("{}", diagnostics.json()), json);
//! # Ok::<(), ferravex_engine::diagnostics::DiagnosticInputError>(())
//! ```
//!
//! [`Diagnostics::write_ndjson`] exports individual records followed by a counter
//! summary, with one versioned JSON object per line. [`Diagnostics::summary`]
//! provides the same counts without formatting or allocating. These additions
//! leave the existing version 1 JSON snapshot unchanged.
use std::collections::VecDeque;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Trace,
    Info,
    Warning,
    Error,
}
impl Severity {
    /// Stable lowercase label used by the JSON schema.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
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

/// Retained record and occurrence counts, returned by [`Diagnostics::summary`].
/// See that method's example for total and per-severity queries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticCounts {
    /// Number of retained records after consecutive deduplication.
    pub records: usize,
    /// Sum of retained `repeats`, including each record's first occurrence.
    /// Saturates at `u64::MAX`; does not include evicted occurrences.
    pub occurrences: u64,
}
impl DiagnosticCounts {
    fn add(&mut self, record: &Diagnostic) {
        self.records += 1;
        self.occurrences = self.occurrences.saturating_add(record.repeats);
    }

    fn write_json<W: fmt::Write + ?Sized>(&self, output: &mut W) -> fmt::Result {
        write!(
            output,
            "{{\"records\":{},\"occurrences\":{}}}",
            self.records, self.occurrences
        )
    }
}

/// Fixed-size snapshot of collector configuration and counters.
/// Constructed without allocation by [`Diagnostics::summary`]. Counts describe
/// retained records, not a lifetime event total; filtered/rejected input is absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticSummary {
    pub capacity: usize,
    /// Current recording threshold; changing it does not remove older records.
    pub minimum: Severity,
    pub retained: DiagnosticCounts,
    /// Records removed by capacity pressure since the last [`Diagnostics::clear`].
    /// This counts records, not occurrences, and saturates at `u64::MAX`.
    pub evicted: u64,
    by_severity: [DiagnosticCounts; 4],
}
impl DiagnosticSummary {
    /// Counts for one severity, including zero counts when it has no records.
    ///
    /// ```
    /// use ferravex_engine::diagnostics::{DiagnosticCounts, Diagnostics, Severity};
    /// let mut diagnostics = Diagnostics::new(2, Severity::Info);
    /// diagnostics.record(1, Severity::Error, "load.failed", "Missing scene")?;
    /// assert_eq!(diagnostics.summary().for_severity(Severity::Error),
    ///     DiagnosticCounts { records: 1, occurrences: 1 });
    /// # Ok::<(), ferravex_engine::diagnostics::DiagnosticError>(())
    /// ```
    pub const fn for_severity(&self, severity: Severity) -> DiagnosticCounts {
        self.by_severity[severity_index(severity)]
    }

    fn write_ndjson<W: fmt::Write + ?Sized>(&self, output: &mut W) -> fmt::Result {
        write!(
            output,
            "{{\"stream_version\":1,\"type\":\"summary\",\"capacity\":{},\"minimum\":\"{}\",\"retained\":",
            self.capacity,
            self.minimum.as_str()
        )?;
        self.retained.write_json(output)?;
        write!(output, ",\"evicted\":{},\"by_severity\":{{", self.evicted)?;
        for (index, severity) in [
            Severity::Trace,
            Severity::Info,
            Severity::Warning,
            Severity::Error,
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                output.write_char(',')?;
            }
            write!(output, "\"{}\":", severity.as_str())?;
            self.for_severity(severity).write_json(output)?;
        }
        output.write_str("}}\n")
    }
}

const fn severity_index(severity: Severity) -> usize {
    match severity {
        Severity::Trace => 0,
        Severity::Info => 1,
        Severity::Warning => 2,
        Severity::Error => 3,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticError {
    InvalidText,
}
impl fmt::Display for DiagnosticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "diagnostic code must be 1..=64 UTF-8 bytes and message at most 1024 UTF-8 bytes",
        )
    }
}
impl std::error::Error for DiagnosticError {}

/// Source coordinates supplied by the producer; both line and column are 1-based.
/// Columns count Unicode scalar values, not UTF-8 bytes or display cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticSource<'a> {
    /// A path or source identifier of 1..=512 UTF-8 bytes; never opened by this module.
    pub file: &'a str,
    pub line: u32,
    pub column: u32,
}

/// Optional bounded context. `None` and `Some("")` remain distinct in the export.
/// Borrowed strings are copied into the collector only for a new retained record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticContext<'a> {
    pub source: Option<DiagnosticSource<'a>>,
    /// The operation that failed, at most 128 UTF-8 bytes.
    pub operation: Option<&'a str>,
    /// An actionable remedy, at most 1024 UTF-8 bytes.
    pub help: Option<&'a str>,
}

/// Detailed validation failure returned by [`Diagnostics::record_with_context`].
/// Rejected input never changes retained records or eviction counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticInputError {
    TextLength {
        /// Schema field path, e.g. `code` or `context.source.file`.
        field: &'static str,
        bytes: usize,
        min: usize,
        max: usize,
    },
    InvalidLocation {
        line: u32,
        column: u32,
    },
}
impl DiagnosticInputError {
    /// Stable machine-readable code, independent of the human error message.
    pub const fn code(self) -> &'static str {
        match self {
            Self::TextLength { .. } => "diagnostics.text_length",
            Self::InvalidLocation { .. } => "diagnostics.source_location",
        }
    }
}
impl fmt::Display for DiagnosticInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextLength {
                field,
                bytes,
                min,
                max,
            } => write!(
                f,
                "{}: {field} has {bytes} UTF-8 bytes; expected {min}..={max}",
                self.code()
            ),
            Self::InvalidLocation { line, column } => write!(
                f,
                "{}: source line and column must be 1-based; got line {line}, column {column}",
                self.code()
            ),
        }
    }
}
impl std::error::Error for DiagnosticInputError {}

#[derive(Default)]
struct StoredContext {
    source: Option<(String, u32, u32)>,
    operation: Option<String>,
    help: Option<String>,
}
impl StoredContext {
    fn borrowed(&self) -> DiagnosticContext<'_> {
        DiagnosticContext {
            source: self
                .source
                .as_ref()
                .map(|(file, line, column)| DiagnosticSource {
                    file,
                    line: *line,
                    column: *column,
                }),
            operation: self.operation.as_deref(),
            help: self.help.as_deref(),
        }
    }
}
impl From<DiagnosticContext<'_>> for StoredContext {
    fn from(context: DiagnosticContext<'_>) -> Self {
        Self {
            source: context
                .source
                .map(|source| (source.file.into(), source.line, source.column)),
            operation: context.operation.map(str::to_owned),
            help: context.help.map(str::to_owned),
        }
    }
}
struct StoredDiagnostic {
    diagnostic: Diagnostic,
    context: StoredContext,
}
impl StoredDiagnostic {
    fn write_json<W: fmt::Write + ?Sized>(&self, output: &mut W) -> fmt::Result {
        let record = &self.diagnostic;
        write!(
            output,
            "{{\"first_tick\":{},\"last_tick\":{},\"severity\":\"{}\",\"code\":",
            record.first_tick,
            record.last_tick,
            record.severity.as_str()
        )?;
        write_json_string(output, &record.code)?;
        output.write_str(",\"message\":")?;
        write_json_string(output, &record.message)?;
        write!(
            output,
            ",\"repeats\":{},\"context\":{{\"source\":",
            record.repeats
        )?;
        let context = self.context.borrowed();
        if let Some(source) = context.source {
            output.write_str("{\"file\":")?;
            write_json_string(output, source.file)?;
            write!(
                output,
                ",\"line\":{},\"column\":{}}}",
                source.line, source.column
            )?;
        } else {
            output.write_str("null")?;
        }
        output.write_str(",\"operation\":")?;
        write_json_optional_string(output, context.operation)?;
        output.write_str(",\"help\":")?;
        write_json_optional_string(output, context.help)?;
        output.write_str("}}")
    }
}

pub struct Diagnostics {
    records: VecDeque<StoredDiagnostic>,
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
        self.record_with_context(tick, severity, code, message, DiagnosticContext::default())
            .map_err(|_| DiagnosticError::InvalidText)
    }

    /// Record a diagnostic with bounded structured context and detailed errors.
    /// Validation occurs even when filtered out or disabled, before any mutation.
    /// Consecutive retained records deduplicate only when severity, code, message
    /// and context match and `tick` is nondecreasing. Filtered/rejected calls do
    /// not interrupt that sequence. Repeat and eviction counts saturate at `u64::MAX`.
    /// See the module example for source coordinates and suggested fixes.
    pub fn record_with_context(
        &mut self,
        tick: u64,
        severity: Severity,
        code: &str,
        message: &str,
        context: DiagnosticContext<'_>,
    ) -> Result<bool, DiagnosticInputError> {
        validate_text("code", code, 1, 64)?;
        validate_text("message", message, 0, 1024)?;
        if let Some(source) = context.source {
            validate_text("context.source.file", source.file, 1, 512)?;
            if source.line == 0 || source.column == 0 {
                return Err(DiagnosticInputError::InvalidLocation {
                    line: source.line,
                    column: source.column,
                });
            }
        }
        if let Some(operation) = context.operation {
            validate_text("context.operation", operation, 0, 128)?;
        }
        if let Some(help) = context.help {
            validate_text("context.help", help, 0, 1024)?;
        }
        if severity < self.minimum || self.capacity == 0 {
            return Ok(false);
        }
        if let Some(last) = self.records.back_mut().filter(|last| {
            last.diagnostic.severity == severity
                && last.diagnostic.code == code
                && last.diagnostic.message == message
                && tick >= last.diagnostic.last_tick
                && last.context.borrowed() == context
        }) {
            last.diagnostic.last_tick = tick;
            last.diagnostic.repeats = last.diagnostic.repeats.saturating_add(1);
            return Ok(true);
        }
        if self.records.len() == self.capacity {
            self.records.pop_front();
            self.evicted = self.evicted.saturating_add(1);
        }
        self.records.push_back(StoredDiagnostic {
            diagnostic: Diagnostic {
                first_tick: tick,
                last_tick: tick,
                severity,
                code: code.into(),
                message: message.into(),
                repeats: 1,
            },
            context: context.into(),
        });
        Ok(true)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.records.iter().map(|entry| &entry.diagnostic)
    }
    /// Retained records with their context, in insertion order (not sorted by tick).
    pub fn iter_with_context(&self) -> impl Iterator<Item = (&Diagnostic, DiagnosticContext<'_>)> {
        self.records
            .iter()
            .map(|entry| (&entry.diagnostic, entry.context.borrowed()))
    }
    pub fn evicted(&self) -> u64 {
        self.evicted
    }
    pub fn clear(&mut self) {
        self.records.clear();
        self.evicted = 0;
    }

    /// Count retained records and occurrences in one pass, without allocation.
    ///
    /// Per-severity and total occurrences saturate independently at `u64::MAX`.
    /// Evicted records are excluded from retained counts. The eviction counter
    /// counts records removed since [`Self::clear`], not lost occurrences.
    /// Changing the minimum affects future recording only; old severities still
    /// appear in the summary. The returned value remains a snapshot after edits.
    ///
    /// ```
    /// use ferravex_engine::diagnostics::{DiagnosticCounts, Diagnostics, Severity};
    /// let mut diagnostics = Diagnostics::new(2, Severity::Warning);
    /// for tick in 1..=3 {
    ///     diagnostics.record(tick, Severity::Warning, "frame.slow", "Over budget")?;
    /// }
    /// let summary = diagnostics.summary();
    /// assert_eq!(summary.retained, DiagnosticCounts { records: 1, occurrences: 3 });
    /// assert_eq!(summary.for_severity(Severity::Warning), summary.retained);
    /// assert_eq!(summary.for_severity(Severity::Error), DiagnosticCounts::default());
    /// assert_eq!(summary.evicted, 0);
    /// # Ok::<(), ferravex_engine::diagnostics::DiagnosticError>(())
    /// ```
    pub fn summary(&self) -> DiagnosticSummary {
        let mut summary = DiagnosticSummary {
            capacity: self.capacity,
            minimum: self.minimum,
            retained: DiagnosticCounts::default(),
            evicted: self.evicted,
            by_severity: [DiagnosticCounts::default(); 4],
        };
        for record in self.iter() {
            summary.retained.add(record);
            summary.by_severity[severity_index(record.severity)].add(record);
        }
        summary
    }

    /// Append an NDJSON snapshot: one line per retained record, then a summary.
    ///
    /// The stream format is versioned independently of [`Self::write_json`].
    /// Each diagnostic line has fixed fields `stream_version` (1), `type`
    /// (`"diagnostic"`), and `record` (the unchanged version 1 snapshot record).
    /// The final line has `stream_version` (1), `type` (`"summary"`), `capacity`,
    /// `minimum`, `retained`, `evicted`, and `by_severity`, in that order.
    /// `retained` and each `by_severity` value contain `records` and `occurrences`;
    /// all four severities appear in trace/info/warning/error order, even if zero.
    /// Counters have the semantics of [`Self::summary`]. An empty or disabled
    /// collector still writes the summary line.
    ///
    /// Every object ends with a single LF, including the summary. Text escaping,
    /// exact decimal integers and insertion order match [`Self::write_json`].
    /// Export neither allocates a snapshot, drains records nor flushes the writer.
    /// This is a complete retained snapshot, not an incremental event subscription:
    /// repeated calls include existing records and their updated repeat counts.
    /// A summary line marks the end of each snapshot when appending multiple calls.
    /// On failure the writer may contain complete lines and an incomplete last
    /// line; the first error is propagated immediately, without collector mutation.
    ///
    /// ```
    /// use ferravex_engine::diagnostics::{Diagnostics, Severity};
    /// let mut diagnostics = Diagnostics::new(2, Severity::Info);
    /// diagnostics.record(9, Severity::Error, "scene.invalid", "First line\nSecond line")?;
    /// let mut text = String::new();
    /// diagnostics.write_ndjson(&mut text)?;
    /// assert_eq!(text.lines().count(), 2); // Diagnostic followed by summary.
    /// assert!(text.contains("First line\\nSecond line"));
    /// assert!(text.ends_with('\n'));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn write_ndjson<W: fmt::Write + ?Sized>(&self, output: &mut W) -> fmt::Result {
        for entry in &self.records {
            output.write_str("{\"stream_version\":1,\"type\":\"diagnostic\",\"record\":")?;
            entry.write_json(output)?;
            output.write_str("}\n")?;
        }
        self.summary().write_ndjson(output)
    }

    /// Allocate the NDJSON snapshot documented by [`Self::write_ndjson`].
    ///
    /// ```
    /// use ferravex_engine::diagnostics::{Diagnostics, Severity};
    /// let diagnostics = Diagnostics::new(0, Severity::Info);
    /// let text = diagnostics.to_ndjson();
    /// assert_eq!(text.lines().count(), 1); // Empty collectors still have counters.
    /// assert!(text.contains("\"type\":\"summary\""));
    /// ```
    pub fn to_ndjson(&self) -> String {
        let mut output = String::new();
        self.write_ndjson(&mut output)
            .expect("writing NDJSON to a String cannot fail");
        output
    }

    /// Borrowing NDJSON display adapter; no intermediate snapshot allocation.
    /// This also streams to [`std::io::Write`] through its `write!` support.
    ///
    /// ```
    /// use std::io::Write;
    /// use ferravex_engine::diagnostics::{Diagnostics, Severity};
    /// let mut diagnostics = Diagnostics::new(2, Severity::Info);
    /// diagnostics.record(1, Severity::Info, "scene.ready", "Loaded")?;
    /// let mut bytes = Vec::new(); // A file or socket writer also works.
    /// write!(&mut bytes, "{}", diagnostics.ndjson())?;
    /// assert_eq!(bytes, diagnostics.to_ndjson().as_bytes());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn ndjson(&self) -> DiagnosticsNdjson<'_> {
        DiagnosticsNdjson(self)
    }

    /// Stream a deterministic, compact JSON snapshot to a text writer.
    ///
    /// Schema version 1 has fixed field order: `schema_version`, `capacity`,
    /// `minimum`, `record_count`, `evicted`, `records`. Records contain
    /// `first_tick`, `last_tick`, `severity`, `code`, `message`, `repeats`,
    /// `context`; context contains `source` (`null` or `{file,line,column}`),
    /// `operation` and `help` (strings or `null`). Severity labels are lowercase.
    /// `repeats` includes the first occurrence; `evicted` counts removed records,
    /// not occurrences. [`Self::clear`] resets both records and the eviction count.
    ///
    /// Records retain insertion order, even if ticks go backwards. Integer values
    /// are emitted exactly in decimal; consumers needing all 64 bits must use a
    /// lossless JSON integer reader. Unicode is preserved; quotes, backslashes,
    /// control characters and U+2028/U+2029 are escaped. Output has no newline.
    /// No snapshot allocation or mutation occurs. On writer failure, the error is
    /// propagated and the writer may contain a partial document.
    ///
    /// ```
    /// use ferravex_engine::diagnostics::{Diagnostics, Severity};
    /// let mut diagnostics = Diagnostics::new(4, Severity::Info);
    /// diagnostics.record(1, Severity::Warning, "asset.missing", "Missing player mesh")?;
    /// let mut buffer = String::new();
    /// diagnostics.write_json(&mut buffer)?;
    /// assert_eq!(buffer, diagnostics.to_json());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn write_json<W: fmt::Write + ?Sized>(&self, output: &mut W) -> fmt::Result {
        write!(
            output,
            "{{\"schema_version\":1,\"capacity\":{},\"minimum\":\"{}\",\"record_count\":{},\"evicted\":{},\"records\":[",
            self.capacity,
            self.minimum.as_str(),
            self.records.len(),
            self.evicted
        )?;
        for (index, entry) in self.records.iter().enumerate() {
            if index > 0 {
                output.write_char(',')?;
            }
            entry.write_json(output)?;
        }
        output.write_str("]}")
    }

    /// Allocate a JSON snapshot in the schema documented by [`Self::write_json`].
    pub fn to_json(&self) -> String {
        let mut output = String::new();
        // String's fmt::Write implementation never reports a formatting error.
        self.write_json(&mut output)
            .expect("writing JSON to a String cannot fail");
        output
    }

    /// A borrowing JSON [`fmt::Display`] adapter; see the module usage example.
    pub fn json(&self) -> DiagnosticsJson<'_> {
        DiagnosticsJson(self)
    }
}

/// Display adapter created by [`Diagnostics::json`]. No intermediate allocation.
pub struct DiagnosticsJson<'a>(&'a Diagnostics);
impl fmt::Display for DiagnosticsJson<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.write_json(f)
    }
}

/// Display adapter created by [`Diagnostics::ndjson`]. See that method's example
/// for streaming to an I/O writer without an intermediate allocation.
pub struct DiagnosticsNdjson<'a>(&'a Diagnostics);
impl fmt::Display for DiagnosticsNdjson<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.write_ndjson(f)
    }
}

fn validate_text(
    field: &'static str,
    text: &str,
    min: usize,
    max: usize,
) -> Result<(), DiagnosticInputError> {
    let bytes = text.len();
    if (min..=max).contains(&bytes) {
        Ok(())
    } else {
        Err(DiagnosticInputError::TextLength {
            field,
            bytes,
            min,
            max,
        })
    }
}

fn write_json_optional_string<W: fmt::Write + ?Sized>(
    output: &mut W,
    value: Option<&str>,
) -> fmt::Result {
    match value {
        Some(value) => write_json_string(output, value),
        None => output.write_str("null"),
    }
}

fn write_json_string<W: fmt::Write + ?Sized>(output: &mut W, value: &str) -> fmt::Result {
    output.write_char('"')?;
    for ch in value.chars() {
        match ch {
            '"' => output.write_str("\\\"")?,
            '\\' => output.write_str("\\\\")?,
            '\n' => output.write_str("\\n")?,
            '\r' => output.write_str("\\r")?,
            '\t' => output.write_str("\\t")?,
            '\u{08}' => output.write_str("\\b")?,
            '\u{0c}' => output.write_str("\\f")?,
            ch if ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}' => {
                write!(output, "\\u{:04x}", ch as u32)?;
            }
            ch => output.write_char(ch)?,
        }
    }
    output.write_char('"')
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

    #[test]
    fn json_schema_order_context_and_counters() {
        let mut diagnostics = Diagnostics::new(2, Severity::Warning);
        diagnostics
            .record(0, Severity::Error, "old", "evicted")
            .unwrap();
        let context = DiagnosticContext {
            source: Some(DiagnosticSource {
                file: "scenes/雪.scene",
                line: 7,
                column: 3,
            }),
            operation: Some("parse_scene"),
            help: Some("Use a positive mass."),
        };
        for tick in [1, 4] {
            diagnostics
                .record_with_context(tick, Severity::Error, "scene.mass", "Invalid mass", context)
                .unwrap();
        }
        assert_eq!(
            diagnostics.record(5, Severity::Info, "filtered", "hidden"),
            Ok(false)
        );
        assert_eq!(
            diagnostics.record(5, Severity::Error, "", "rejected"),
            Err(DiagnosticError::InvalidText)
        );
        diagnostics
            .record(5, Severity::Warning, "slow", "")
            .unwrap();

        // An exact schema fixture also pins key order, nulls and integer types.
        let expected = concat!(
            r#"{"schema_version":1,"capacity":2,"minimum":"warning","record_count":2,"evicted":1,"records":["#,
            r#"{"first_tick":1,"last_tick":4,"severity":"error","code":"scene.mass","message":"Invalid mass","repeats":2,"context":{"source":{"file":"scenes/雪.scene","line":7,"column":3},"operation":"parse_scene","help":"Use a positive mass."}},"#,
            r#"{"first_tick":5,"last_tick":5,"severity":"warning","code":"slow","message":"","repeats":1,"context":{"source":null,"operation":null,"help":null}}]}"#,
        );
        assert_eq!(diagnostics.to_json(), expected);
        assert_eq!(diagnostics.to_json(), expected); // Export does not consume or mutate.
        assert_eq!(format!("{}", diagnostics.json()), expected);
        let mut reused = String::from("prefix:");
        diagnostics.write_json(&mut reused).unwrap();
        assert_eq!(reused, format!("prefix:{expected}"));
        assert_eq!(diagnostics.evicted(), 1);

        diagnostics.clear();
        assert_eq!(
            diagnostics.to_json(),
            r#"{"schema_version":1,"capacity":2,"minimum":"warning","record_count":0,"evicted":0,"records":[]}"#
        );
    }

    #[test]
    fn json_escapes_controls_and_preserves_unicode_in_every_text_field() {
        let controls: String = (0..=31).map(char::from).collect();
        let mut encoded = String::new();
        write_json_string(&mut encoded, &controls).unwrap();
        assert_eq!(
            encoded,
            concat!(
                "\"",
                r#"\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\b\t\n\u000b\f\r\u000e\u000f"#,
                r#"\u0010\u0011\u0012\u0013\u0014\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f"#,
                "\"",
            )
        );

        let text = "\"\\/\0\n\t\r\u{08}\u{0c}\u{7f}\u{85}\u{9f}\u{2028}\u{2029}é雪🦀e\u{301}";
        let escaped = r#"\"\\/\u0000\n\t\r\b\f\u007f\u0085\u009f\u2028\u2029é雪🦀é"#;
        let mut diagnostics = Diagnostics::new(1, Severity::Trace);
        diagnostics
            .record_with_context(
                0,
                Severity::Trace,
                text,
                text,
                DiagnosticContext {
                    source: Some(DiagnosticSource {
                        file: text,
                        line: 1,
                        column: 1,
                    }),
                    operation: Some(text),
                    help: Some(text),
                },
            )
            .unwrap();
        let json = diagnostics.to_json();
        assert_eq!(json.matches(escaped).count(), 5);
        assert!(!json.chars().any(char::is_control));
        for field in ["code", "message", "file", "operation", "help"] {
            assert!(json.contains(&format!("\"{field}\":\"{escaped}\"")));
        }
        let ndjson = diagnostics.to_ndjson();
        assert_eq!(ndjson.lines().count(), 2);
        assert_eq!(ndjson.matches(escaped).count(), 5);
        assert!(!ndjson.chars().any(|ch| ch.is_control() && ch != '\n'));
        for field in ["code", "message", "file", "operation", "help"] {
            assert!(ndjson.contains(&format!("\"{field}\":\"{escaped}\"")));
        }
        assert_eq!(diagnostics.iter().next().unwrap().code, text);

        // Text that looks like schema punctuation stays inside a JSON string.
        diagnostics
            .record(
                1,
                Severity::Info,
                "code",
                "\"},\"records\":[],\"forged\":\"yes",
            )
            .unwrap();
        assert!(
            diagnostics
                .to_json()
                .contains(r#""message":"\"},\"records\":[],\"forged\":\"yes","repeats":1"#)
        );
    }

    #[test]
    fn dedup_includes_context_and_keeps_backward_ticks_in_insertion_order() {
        let mut diagnostics = Diagnostics::new(8, Severity::Warning);
        let base = DiagnosticContext {
            source: Some(DiagnosticSource {
                file: "game.rs",
                line: 4,
                column: 9,
            }),
            operation: Some("spawn"),
            help: Some("Supply a mesh."),
        };
        let other_source = DiagnosticContext {
            source: Some(DiagnosticSource {
                line: 5,
                ..base.source.unwrap()
            }),
            ..base
        };
        let other_operation = DiagnosticContext {
            operation: Some("draw"),
            ..base
        };
        let other_help = DiagnosticContext {
            help: Some("Supply a material."),
            ..base
        };
        for (tick, context) in [
            (5, base),
            (5, base),
            (7, base),
            (8, other_source),
            (9, other_operation),
            (10, other_help),
            (11, base),
            (3, base),
        ] {
            diagnostics
                .record_with_context(tick, Severity::Error, "asset.missing", "Missing", context)
                .unwrap();
        }
        let records: Vec<_> = diagnostics.iter_with_context().collect();
        assert_eq!(records.len(), 6);
        assert_eq!(
            (
                records[0].0.first_tick,
                records[0].0.last_tick,
                records[0].0.repeats
            ),
            (5, 7, 3)
        );
        assert_eq!(
            records
                .iter()
                .map(|(record, _)| record.first_tick)
                .collect::<Vec<_>>(),
            [5, 8, 9, 10, 11, 3]
        );
        assert_eq!(records[1].1, other_source);
        assert_eq!(records[2].1, other_operation);
        assert_eq!(records[3].1, other_help);

        // Rejected/filtered input does not split consecutive retained records.
        assert_eq!(
            diagnostics.record(4, Severity::Info, "filtered", ""),
            Ok(false)
        );
        assert!(
            diagnostics
                .record(4, Severity::Error, "", "invalid")
                .is_err()
        );
        diagnostics
            .record_with_context(4, Severity::Error, "asset.missing", "Missing", base)
            .unwrap();
        assert_eq!(diagnostics.iter().last().unwrap().repeats, 2);
    }

    #[test]
    fn context_is_owned_bounded_and_evicted_with_its_record() {
        let mut diagnostics = Diagnostics::new(1, Severity::Trace);
        {
            let file = String::from("dynamic.scene");
            let operation = String::from("load");
            let help = String::from("Correct the path.");
            diagnostics
                .record_with_context(
                    1,
                    Severity::Error,
                    "asset.missing",
                    "Missing",
                    DiagnosticContext {
                        source: Some(DiagnosticSource {
                            file: &file,
                            line: 1,
                            column: 1,
                        }),
                        operation: Some(&operation),
                        help: Some(&help),
                    },
                )
                .unwrap();
        }
        assert_eq!(
            diagnostics.iter_with_context().next().unwrap().1.help,
            Some("Correct the path.")
        );
        diagnostics
            .record(2, Severity::Error, "asset.missing", "Missing")
            .unwrap();
        assert_eq!(diagnostics.evicted(), 1);
        assert_eq!(
            diagnostics.iter_with_context().next().unwrap().1,
            DiagnosticContext::default()
        );
        assert!(!diagnostics.to_json().contains("dynamic.scene"));
        diagnostics
            .record_with_context(
                3,
                Severity::Error,
                "asset.missing",
                "Missing",
                DiagnosticContext {
                    help: Some(""),
                    ..DiagnosticContext::default()
                },
            )
            .unwrap();
        assert_eq!(diagnostics.evicted(), 2);
        assert!(diagnostics.to_json().contains(r#""help":"""#));
    }

    #[test]
    fn validation_reports_field_byte_limits_and_is_transactional() {
        let mut diagnostics = Diagnostics::new(1, Severity::Trace);
        let code = "é".repeat(32);
        let message = "🦀".repeat(256);
        let file = "雪".repeat(170) + "ab";
        let operation = "é".repeat(64);
        let help = "🦀".repeat(256);
        let context = DiagnosticContext {
            source: Some(DiagnosticSource {
                file: &file,
                line: u32::MAX,
                column: u32::MAX,
            }),
            operation: Some(&operation),
            help: Some(&help),
        };
        diagnostics
            .record_with_context(1, Severity::Error, &code, &message, context)
            .unwrap();
        let before = diagnostics.to_json();
        let long_code = code.clone() + "é";
        let long_message = message.clone() + "x";
        let long_file = file.clone() + "x";
        let long_operation = operation.clone() + "x";
        let long_help = help.clone() + "x";
        let cases = [
            ("", message.as_str(), context, "code", 0, 1, 64),
            (
                long_code.as_str(),
                message.as_str(),
                context,
                "code",
                66,
                1,
                64,
            ),
            (
                code.as_str(),
                long_message.as_str(),
                context,
                "message",
                1025,
                0,
                1024,
            ),
            (
                code.as_str(),
                message.as_str(),
                DiagnosticContext {
                    source: Some(DiagnosticSource {
                        file: "",
                        ..context.source.unwrap()
                    }),
                    ..context
                },
                "context.source.file",
                0,
                1,
                512,
            ),
            (
                code.as_str(),
                message.as_str(),
                DiagnosticContext {
                    source: Some(DiagnosticSource {
                        file: &long_file,
                        ..context.source.unwrap()
                    }),
                    ..context
                },
                "context.source.file",
                513,
                1,
                512,
            ),
            (
                code.as_str(),
                message.as_str(),
                DiagnosticContext {
                    operation: Some(&long_operation),
                    ..context
                },
                "context.operation",
                129,
                0,
                128,
            ),
            (
                code.as_str(),
                message.as_str(),
                DiagnosticContext {
                    help: Some(&long_help),
                    ..context
                },
                "context.help",
                1025,
                0,
                1024,
            ),
        ];
        for (code, message, context, field, bytes, min, max) in cases {
            let error = diagnostics
                .record_with_context(2, Severity::Error, code, message, context)
                .unwrap_err();
            assert_eq!(
                error,
                DiagnosticInputError::TextLength {
                    field,
                    bytes,
                    min,
                    max
                }
            );
            assert_eq!(error.code(), "diagnostics.text_length");
            assert!(error.to_string().contains(field));
            assert!(error.to_string().contains(&format!("{bytes} UTF-8 bytes")));
            assert_eq!(diagnostics.to_json(), before);
        }
        for (line, column) in [(0, 1), (1, 0), (0, 0)] {
            let error = diagnostics
                .record_with_context(
                    2,
                    Severity::Error,
                    &code,
                    &message,
                    DiagnosticContext {
                        source: Some(DiagnosticSource {
                            line,
                            column,
                            ..context.source.unwrap()
                        }),
                        ..context
                    },
                )
                .unwrap_err();
            assert_eq!(
                error,
                DiagnosticInputError::InvalidLocation { line, column }
            );
            assert_eq!(error.code(), "diagnostics.source_location");
            assert!(error.to_string().contains("must be 1-based"));
            assert_eq!(diagnostics.to_json(), before);
        }
        assert_eq!(
            diagnostics.record(2, Severity::Error, &long_code, ""),
            Err(DiagnosticError::InvalidText)
        );
    }

    #[test]
    fn disabled_and_filtered_recording_still_validates_and_preserves_counters() {
        for capacity in [0, 2] {
            let mut diagnostics = Diagnostics::new(capacity, Severity::Error);
            let before = diagnostics.to_json();
            assert_eq!(
                diagnostics.record_with_context(
                    1,
                    Severity::Info,
                    "code",
                    "",
                    DiagnosticContext::default()
                ),
                Ok(false)
            );
            assert!(
                diagnostics
                    .record_with_context(
                        1,
                        Severity::Info,
                        "code",
                        "",
                        DiagnosticContext {
                            source: Some(DiagnosticSource {
                                file: "source",
                                line: 0,
                                column: 1
                            }),
                            ..DiagnosticContext::default()
                        }
                    )
                    .is_err()
            );
            assert_eq!(diagnostics.to_json(), before);
            diagnostics.set_minimum(Severity::Trace);
            assert_eq!(
                diagnostics.record(2, Severity::Trace, "trace", ""),
                Ok(capacity != 0)
            );
        }
        assert_eq!(
            [
                Severity::Trace,
                Severity::Info,
                Severity::Warning,
                Severity::Error
            ]
            .map(Severity::as_str),
            ["trace", "info", "warning", "error"]
        );
    }

    #[test]
    fn exact_u64_values_and_saturating_counters() {
        let mut diagnostics = Diagnostics::new(1, Severity::Error);
        diagnostics
            .record(u64::MAX, Severity::Error, "code", "")
            .unwrap();
        diagnostics.records.back_mut().unwrap().diagnostic.repeats = u64::MAX;
        diagnostics
            .record(u64::MAX, Severity::Error, "code", "")
            .unwrap();
        assert_eq!(diagnostics.iter().next().unwrap().repeats, u64::MAX);
        for field in ["first_tick", "last_tick", "repeats"] {
            assert!(
                diagnostics
                    .to_json()
                    .contains(&format!("\"{field}\":18446744073709551615"))
            );
        }
        diagnostics.evicted = u64::MAX;
        diagnostics.record(0, Severity::Error, "code", "").unwrap();
        assert_eq!(diagnostics.evicted(), u64::MAX);
        assert_eq!(diagnostics.iter().next().unwrap().repeats, 1);
        assert!(
            diagnostics
                .to_json()
                .contains("\"evicted\":18446744073709551615")
        );
        diagnostics.clear();
        assert_eq!(diagnostics.evicted(), 0);
    }

    #[test]
    fn writer_failure_is_propagated_without_mutating_diagnostics() {
        struct FailingWriter {
            remaining: usize,
        }
        impl fmt::Write for FailingWriter {
            fn write_str(&mut self, text: &str) -> fmt::Result {
                self.remaining = self.remaining.checked_sub(text.len()).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let mut diagnostics = Diagnostics::new(1, Severity::Info);
        diagnostics
            .record(1, Severity::Error, "code", "quote: \"; snow: 雪")
            .unwrap();
        let before = diagnostics.to_json();
        for remaining in 0..before.len() {
            assert!(
                diagnostics
                    .write_json(&mut FailingWriter { remaining })
                    .is_err()
            );
            assert!(
                fmt::write(
                    &mut FailingWriter { remaining },
                    format_args!("{}", diagnostics.json())
                )
                .is_err()
            );
        }
        assert!(
            diagnostics
                .write_json(&mut FailingWriter {
                    remaining: before.len()
                })
                .is_ok()
        );
        assert_eq!(diagnostics.to_json(), before);
    }

    #[test]
    fn summary_counts_retained_occurrences_by_severity_and_resets() {
        let mut diagnostics = Diagnostics::new(4, Severity::Trace);
        for severity in [
            Severity::Trace,
            Severity::Info,
            Severity::Warning,
            Severity::Error,
        ] {
            for tick in 1..=3 {
                diagnostics.record(tick, severity, "code", "").unwrap();
            }
        }
        let initial = diagnostics.summary();
        assert_eq!(initial.capacity, 4);
        assert_eq!(initial.minimum, Severity::Trace);
        assert_eq!(initial.evicted, 0);
        assert_eq!(
            initial.retained,
            DiagnosticCounts {
                records: 4,
                occurrences: 12
            }
        );
        for severity in [
            Severity::Trace,
            Severity::Info,
            Severity::Warning,
            Severity::Error,
        ] {
            assert_eq!(
                initial.for_severity(severity),
                DiagnosticCounts {
                    records: 1,
                    occurrences: 3
                }
            );
        }

        // The same diagnostic with different context retains a new record.
        diagnostics
            .record_with_context(
                4,
                Severity::Error,
                "code",
                "",
                DiagnosticContext {
                    help: Some(""),
                    ..DiagnosticContext::default()
                },
            )
            .unwrap();
        let after_eviction = diagnostics.summary();
        assert_eq!(after_eviction.evicted, 1); // Three occurrences left together.
        assert_eq!(after_eviction.retained.records, 4);
        assert_eq!(after_eviction.retained.occurrences, 10);
        assert_eq!(
            after_eviction.for_severity(Severity::Trace),
            DiagnosticCounts::default()
        );
        assert_eq!(after_eviction.for_severity(Severity::Error).records, 2);
        assert_eq!(after_eviction.for_severity(Severity::Error).occurrences, 4);

        diagnostics.set_minimum(Severity::Error);
        let before_rejection = diagnostics.summary();
        assert_eq!(before_rejection.minimum, Severity::Error);
        assert_eq!(before_rejection.retained, after_eviction.retained);
        assert_eq!(
            diagnostics.record(5, Severity::Info, "filtered", ""),
            Ok(false)
        );
        assert!(diagnostics.record(5, Severity::Error, "", "").is_err());
        assert_eq!(diagnostics.summary(), before_rejection);

        diagnostics.clear();
        let cleared = diagnostics.summary();
        assert_eq!(cleared.capacity, 4);
        assert_eq!(cleared.minimum, Severity::Error);
        assert_eq!(cleared.evicted, 0);
        assert_eq!(cleared.retained, DiagnosticCounts::default());
        assert_eq!(cleared.by_severity, [DiagnosticCounts::default(); 4]);
        assert_eq!(initial.retained.occurrences, 12); // A detached snapshot.
    }

    #[test]
    fn summary_and_ndjson_saturate_occurrences_without_losing_record_counts() {
        let mut diagnostics = Diagnostics::new(3, Severity::Trace);
        for (code, severity) in [
            ("a", Severity::Error),
            ("b", Severity::Error),
            ("c", Severity::Warning),
        ] {
            diagnostics.record(u64::MAX, severity, code, "").unwrap();
            diagnostics.records.back_mut().unwrap().diagnostic.repeats = u64::MAX;
        }
        diagnostics.evicted = u64::MAX;
        let summary = diagnostics.summary();
        assert_eq!(summary.retained.records, 3);
        assert_eq!(summary.retained.occurrences, u64::MAX);
        assert_eq!(summary.for_severity(Severity::Error).records, 2);
        assert_eq!(summary.for_severity(Severity::Error).occurrences, u64::MAX);
        assert_eq!(
            summary.for_severity(Severity::Warning).occurrences,
            u64::MAX
        );
        assert_eq!(summary.evicted, u64::MAX);
        let ndjson = diagnostics.to_ndjson();
        assert_eq!(
            ndjson
                .matches("\"first_tick\":18446744073709551615")
                .count(),
            3
        );
        assert_eq!(
            ndjson.matches("\"last_tick\":18446744073709551615").count(),
            3
        );
        assert_eq!(
            ndjson.matches("\"repeats\":18446744073709551615").count(),
            3
        );
        assert_eq!(
            ndjson
                .matches("\"occurrences\":18446744073709551615")
                .count(),
            3
        );
        assert!(ndjson.contains("\"evicted\":18446744073709551615"));
    }

    #[test]
    fn ndjson_schema_preserves_context_order_and_snapshot_record_encoding() {
        let mut diagnostics = Diagnostics::new(2, Severity::Warning);
        diagnostics.record(0, Severity::Error, "old", "").unwrap();
        let context = DiagnosticContext {
            source: Some(DiagnosticSource {
                file: "levels/雪.scene",
                line: 7,
                column: 2,
            }),
            operation: Some("load\nscene"),
            help: Some(""),
        };
        for tick in [8, 10] {
            diagnostics
                .record_with_context(tick, Severity::Warning, "slow", "quote: \"", context)
                .unwrap();
        }
        diagnostics.record(2, Severity::Error, "bad", "").unwrap();
        let expected = concat!(
            r#"{"stream_version":1,"type":"diagnostic","record":{"first_tick":8,"last_tick":10,"severity":"warning","code":"slow","message":"quote: \"","repeats":2,"context":{"source":{"file":"levels/雪.scene","line":7,"column":2},"operation":"load\nscene","help":""}}}"#,
            "\n",
            r#"{"stream_version":1,"type":"diagnostic","record":{"first_tick":2,"last_tick":2,"severity":"error","code":"bad","message":"","repeats":1,"context":{"source":null,"operation":null,"help":null}}}"#,
            "\n",
            r#"{"stream_version":1,"type":"summary","capacity":2,"minimum":"warning","retained":{"records":2,"occurrences":3},"evicted":1,"by_severity":{"trace":{"records":0,"occurrences":0},"info":{"records":0,"occurrences":0},"warning":{"records":1,"occurrences":2},"error":{"records":1,"occurrences":1}}}"#,
            "\n",
        );
        let before = diagnostics.to_json();
        assert_eq!(diagnostics.to_ndjson(), expected);
        assert_eq!(format!("{}", diagnostics.ndjson()), expected);
        let mut appended = String::from("prefix\n");
        diagnostics.write_ndjson(&mut appended).unwrap();
        diagnostics.write_ndjson(&mut appended).unwrap();
        assert_eq!(appended, format!("prefix\n{expected}{expected}"));
        assert_eq!(diagnostics.to_json(), before);

        // The version 1 snapshot's record array uses exactly the same objects.
        let record_json = expected
            .lines()
            .take(2)
            .map(|line| {
                line.strip_prefix("{\"stream_version\":1,\"type\":\"diagnostic\",\"record\":")
                    .unwrap()
                    .strip_suffix('}')
                    .unwrap()
            })
            .collect::<Vec<_>>()
            .join(",");
        assert!(before.ends_with(&format!("\"records\":[{record_json}]}}")));
    }

    #[test]
    fn empty_disabled_and_cleared_ndjson_have_a_summary_terminator() {
        let mut diagnostics = Diagnostics::new(0, Severity::Info);
        assert_eq!(
            diagnostics.record(1, Severity::Error, "code", ""),
            Ok(false)
        );
        assert!(diagnostics.record(1, Severity::Error, "", "").is_err());
        assert_eq!(
            diagnostics.to_ndjson(),
            concat!(
                r#"{"stream_version":1,"type":"summary","capacity":0,"minimum":"info","retained":{"records":0,"occurrences":0},"evicted":0,"by_severity":{"trace":{"records":0,"occurrences":0},"info":{"records":0,"occurrences":0},"warning":{"records":0,"occurrences":0},"error":{"records":0,"occurrences":0}}}"#,
                "\n"
            )
        );
        let mut diagnostics = Diagnostics::new(1, Severity::Trace);
        let empty = diagnostics.to_ndjson();
        diagnostics.record(1, Severity::Info, "one", "").unwrap();
        diagnostics.record(2, Severity::Error, "two", "").unwrap();
        diagnostics.clear();
        assert_eq!(diagnostics.to_ndjson(), empty);
        assert_eq!(empty.lines().count(), 1);
    }

    #[test]
    fn ndjson_partial_writer_failures_stop_at_every_byte_and_preserve_state() {
        struct FailingWriter {
            remaining: usize,
            bytes: Vec<u8>,
            failed: bool,
        }
        impl fmt::Write for FailingWriter {
            fn write_str(&mut self, text: &str) -> fmt::Result {
                assert!(!self.failed, "export must stop at the first error");
                let accepted = self.remaining.min(text.len());
                self.bytes.extend_from_slice(&text.as_bytes()[..accepted]);
                self.remaining -= accepted;
                if accepted < text.len() {
                    self.failed = true;
                    Err(fmt::Error)
                } else {
                    Ok(())
                }
            }
        }
        let mut diagnostics = Diagnostics::new(2, Severity::Trace);
        diagnostics
            .record_with_context(
                7,
                Severity::Warning,
                "escaped",
                "snow: 雪; quote: \";\nnext line",
                DiagnosticContext {
                    source: Some(DiagnosticSource {
                        file: "path\\file",
                        line: 8,
                        column: 2,
                    }),
                    operation: Some("read\nscene"),
                    help: Some("Fix 🦀"),
                },
            )
            .unwrap();
        diagnostics.record(3, Severity::Error, "last", "").unwrap();
        let expected = diagnostics.to_ndjson();
        let before = diagnostics.to_json();
        let summary = diagnostics.summary();
        for remaining in 0..=expected.len() {
            for display in [false, true] {
                let mut writer = FailingWriter {
                    remaining,
                    bytes: Vec::new(),
                    failed: false,
                };
                let result = if display {
                    fmt::write(&mut writer, format_args!("{}", diagnostics.ndjson()))
                } else {
                    // A dynamically sized text writer is supported as well.
                    diagnostics.write_ndjson(&mut writer as &mut dyn fmt::Write)
                };
                assert_eq!(result.is_ok(), remaining == expected.len());
                assert_eq!(writer.bytes, expected.as_bytes()[..remaining]);
                assert_eq!(diagnostics.summary(), summary);
            }
        }
        assert_eq!(diagnostics.to_json(), before);
    }

    #[test]
    fn ndjson_display_propagates_original_io_error_after_short_writes() {
        use std::io::{self, Write};

        struct ShortWriter {
            remaining: usize,
            bytes: Vec<u8>,
        }
        impl io::Write for ShortWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.remaining == 0 {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed sink"));
                }
                let accepted = bytes.len().min(self.remaining).min(3);
                self.bytes.extend_from_slice(&bytes[..accepted]);
                self.remaining -= accepted;
                Ok(accepted)
            }
            fn flush(&mut self) -> io::Result<()> {
                panic!("export does not flush the caller's writer")
            }
        }
        let mut diagnostics = Diagnostics::new(1, Severity::Info);
        diagnostics.record(2, Severity::Info, "code", "雪").unwrap();
        let expected = diagnostics.to_ndjson();
        for remaining in [0, 5, expected.find('\n').unwrap() + 1, expected.len() - 1] {
            let mut writer = ShortWriter {
                remaining,
                bytes: Vec::new(),
            };
            let error = write!(&mut writer, "{}", diagnostics.ndjson()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
            assert_eq!(error.to_string(), "closed sink");
            assert_eq!(writer.bytes, expected.as_bytes()[..remaining]);
        }
        let mut writer = ShortWriter {
            remaining: expected.len(),
            bytes: Vec::new(),
        };
        write!(&mut writer, "{}", diagnostics.ndjson()).unwrap();
        assert_eq!(writer.bytes, expected.as_bytes());
    }
}
