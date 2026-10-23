//! Optional authoring bridge. A Source polls only explicit local files or queued
//! browser edits; ordinary frames do no filesystem work or source allocation.
//!
//! [`reload_state`] inspects the calling thread's bridge without consuming edits.
//! The bridge serves one selected document: constructing a [`ContentSource`]
//! selects it and resets its diagnostics. Poll that source on the same thread as
//! [`queue`] and inspection. The existing Wasm status codes/exports are unchanged.
use ferravex_play::authoring::{ContentError, MAX_SOURCE_BYTES};
use std::cell::RefCell;

/// Maximum UTF-8 bytes retained in a reload failure's error context.
pub const MAX_RELOAD_CONTEXT_BYTES: usize = 4096;

/// Latest bridge operation, using the existing browser status codes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum ReloadStatus {
    /// No document has been selected on this thread.
    #[default]
    Uninitialized = 0,
    /// Embedded content, a successful replacement, or an unchanged edit.
    Ready = 1,
    /// An edit was queued for the game to validate.
    Pending = 2,
    /// An edit, file read, or replacement was rejected.
    Rejected = 3,
}

/// Where a rejected reload originated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadOrigin {
    /// [`queue`] or the browser edit buffer.
    QueuedEdit,
    /// The explicitly configured native content file.
    NativeFile,
}

/// The boundary that rejected a reload, independent of the domain error kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadStage {
    /// Source length or NUL validation, including browser buffer length checks.
    Input,
    /// Opening or reading the native file.
    Read,
    /// Decoding UTF-8 bytes.
    Decode,
    /// The game's builder returned an error (including parsing/validation).
    Build,
    /// The accepted generation counter cannot advance without overflow.
    Generation,
}

/// One retained failure; the diagnostic stores no failed source text or history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReloadFailure {
    /// The input channel responsible for the failure.
    pub origin: ReloadOrigin,
    /// The operation that failed.
    pub stage: ReloadStage,
    /// Original error kind and source span. Context is clipped on a UTF-8
    /// boundary to [`MAX_RELOAD_CONTEXT_BYTES`]; native read errors include the
    /// path in this context. Use `context_truncated` when displaying it.
    pub error: ContentError,
    /// Whether any error context was omitted from this diagnostic.
    pub context_truncated: bool,
}
impl ReloadFailure {
    fn new(origin: ReloadOrigin, stage: ReloadStage, mut error: ContentError) -> Self {
        let context_truncated = error.context.len() > MAX_RELOAD_CONTEXT_BYTES;
        if context_truncated {
            let mut end = MAX_RELOAD_CONTEXT_BYTES;
            while !error.context.is_char_boundary(end) {
                end -= 1;
            }
            error.context.truncate(end);
        }
        // Do not retain a builder's oversized backing allocation after clipping.
        error.context = error.context.into_boxed_str().into_string();
        Self {
            origin,
            stage,
            error,
            context_truncated,
        }
    }
}

/// An owned diagnostic snapshot of the calling thread's selected document.
/// Source text is not copied. Inspection allocates only to clone retained error
/// context when a failure is present; it never reads a file or consumes an edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReloadState {
    /// Document selection counter, matching the existing Wasm export. Wraps.
    pub document: u32,
    /// Latest operation; pending work is reported separately below.
    pub status: ReloadStatus,
    /// Last-good content generation; starts at zero for embedded content and
    /// advances once per successfully built, different source. Never wraps.
    pub generation: u32,
    /// Status change counter, matching the existing Wasm revision export. Wraps;
    /// compare for inequality within a document, not for numeric ordering.
    pub revision: u32,
    /// Status revision at the last accepted replacement, or zero for embedded
    /// content. Queues, failures and unchanged edits leave this value intact.
    /// It has the same wrapping semantics as `revision`.
    pub last_good_revision: u32,
    /// An edit still awaits polling. This may be true even after rejection of a
    /// newer invalid submission, or if a builder queues another edit.
    pub has_pending_edit: bool,
    /// Failure for the latest rejected operation; cleared by the next successful
    /// queue, unchanged edit, or accepted replacement. Idle polls preserve it.
    pub failure: Option<ReloadFailure>,
}

/// Inspect reload state without advancing the game or consuming queued work.
///
/// ```
/// use ferravex_render::content::{queue, reload_state, ContentSource, ReloadStatus};
///
/// let mut source = ContentSource::new("embedded", None)?;
/// queue("replacement")?;
/// assert_eq!(reload_state().status, ReloadStatus::Pending);
/// assert_eq!(source.poll(0.0, |text| Ok(text.len())), Some(11));
/// let accepted = reload_state();
/// assert_eq!(accepted.generation, 1);
/// assert_eq!(accepted.last_good_revision, accepted.revision);
/// queue("replacement")?;
/// assert!(source.poll::<()>(0.0, |_| panic!("unchanged")).is_none());
/// assert_eq!(reload_state().last_good_revision, accepted.last_good_revision);
/// assert_eq!(source.generation(), 1);
/// # Ok::<(), ferravex_play::authoring::ContentError>(())
/// ```
///
/// Builders keep live game state outside the bridge; replace it only on `Some`.
/// Structured errors retain the builder's kind/span and bounded context:
///
/// ```
/// use ferravex_play::authoring::{ContentError, ErrorKind};
/// use ferravex_render::content::{
///     queue, reload_state, ContentSource, ReloadFailure, ReloadOrigin, ReloadStage,
///     ReloadState, ReloadStatus, MAX_RELOAD_CONTEXT_BYTES,
/// };
/// let mut source = ContentSource::new("embedded", None)?;
/// let mut live = 7;
/// queue("invalid")?;
/// if let Some(next) = source.poll(0.0, |_| Err(ContentError::domain("missing spawn"))) {
///     live = next;
/// }
/// let state: ReloadState = reload_state();
/// assert_eq!(state.status, ReloadStatus::Rejected);
/// let failure: ReloadFailure = state.failure.unwrap();
/// assert_eq!(failure.origin, ReloadOrigin::QueuedEdit);
/// assert_eq!(failure.stage, ReloadStage::Build);
/// assert_eq!(failure.error.kind, ErrorKind::Domain);
/// assert!(!failure.context_truncated);
/// assert!(failure.error.context.len() <= MAX_RELOAD_CONTEXT_BYTES);
/// assert_eq!((live, source.current(), state.generation), (7, "embedded", 0));
/// # Ok::<(), ContentError>(())
/// ```
pub fn reload_state() -> ReloadState {
    BRIDGE.with(|b| {
        let b = b.borrow();
        ReloadState {
            document: b.document,
            status: b.reload_status,
            generation: b.generation,
            revision: b.revision,
            last_good_revision: b.last_good_revision,
            has_pending_edit: b.pending.is_some(),
            failure: b.failure.clone(),
        }
    })
}

#[derive(Default)]
struct Bridge {
    source: String,
    status: String,
    generation: u32,
    revision: u32,
    document: u32,
    reload_status: ReloadStatus,
    last_good_revision: u32,
    failure: Option<ReloadFailure>,
    pending: Option<String>,
    #[cfg(target_arch = "wasm32")]
    buffer: Vec<u8>,
}
thread_local! {static BRIDGE:RefCell<Bridge>=RefCell::new(Bridge::default());}
fn status(state: ReloadStatus, message: String, failure: Option<ReloadFailure>) {
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        b.reload_status = state;
        b.status = message;
        b.failure = failure;
        b.revision = b.revision.wrapping_add(1);
    });
}
/// Queue a single edit; newer unapplied edits replace older ones. Parsing/game
/// validation remains in the game, on its next poll. UTF-8 and size are checked.
/// Rejected submissions update [`reload_state`] but preserve any pending edit.
pub fn queue(source: &str) -> Result<(), ContentError> {
    if source.len() > MAX_SOURCE_BYTES || source.contains('\0') {
        let error = ContentError::domain("source exceeds 2 MiB or contains NUL");
        reject_submission(error.clone(), ReloadStage::Input);
        return Err(error);
    }
    BRIDGE.with(|b| b.borrow_mut().pending = Some(source.into()));
    status(
        ReloadStatus::Pending,
        "Pending content validation.".into(),
        None,
    );
    Ok(())
}
fn reject_submission(error: ContentError, stage: ReloadStage) {
    let failure = ReloadFailure::new(ReloadOrigin::QueuedEdit, stage, error);
    status(
        ReloadStatus::Rejected,
        failure.error.to_string(),
        Some(failure),
    );
}
/// The game supplies a builder that validates its domain and constructs *all*
/// replacement state. Returning Err preserves source, generation and live game.
pub struct ContentSource {
    current: String,
    generation: u32,
    #[cfg(not(target_arch = "wasm32"))]
    path: Option<std::path::PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    elapsed: f64,
    #[cfg(not(target_arch = "wasm32"))]
    last_file: Option<Vec<u8>>,
}
impl ContentSource {
    pub fn new(embedded: &str, native_path: Option<&str>) -> Result<Self, ContentError> {
        if embedded.len() > MAX_SOURCE_BYTES || embedded.contains('\0') {
            return Err(ContentError::domain("invalid embedded source size/UTF-8"));
        }
        BRIDGE.with(|b| {
            let document = b.borrow().document.wrapping_add(1);
            *b.borrow_mut() = Bridge {
                source: embedded.into(),
                status: "Embedded content loaded. Applying an edit restarts this game.".into(),
                reload_status: ReloadStatus::Ready,
                document,
                ..Default::default()
            }
        });
        #[cfg(target_arch = "wasm32")]
        let _ = native_path;
        Ok(Self {
            current: embedded.into(),
            generation: 0,
            #[cfg(not(target_arch = "wasm32"))]
            path: native_path.map(Into::into),
            #[cfg(not(target_arch = "wasm32"))]
            elapsed: 0.5,
            #[cfg(not(target_arch = "wasm32"))]
            last_file: None,
        })
    }
    pub fn current(&self) -> &str {
        &self.current
    }
    pub fn generation(&self) -> u32 {
        self.generation
    }
    /// Build and adopt at most one different source. Only `Some` is an accepted
    /// replacement. When this returns `None`, compare [`reload_state`]'s revision
    /// before/after polling: rejection and unchanged edits update it; idle polls
    /// retain the previous status. Invalid `dt` (non-finite or outside `0..=60`
    /// seconds) leaves pending work and diagnostics untouched.
    /// A builder must construct replacement state without mutating the live game.
    pub fn poll<T>(
        &mut self,
        dt: f32,
        build: impl FnOnce(&str) -> Result<T, ContentError>,
    ) -> Option<T> {
        if !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return None;
        }
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut candidate = BRIDGE.with(|b| b.borrow_mut().pending.take());
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut origin = ReloadOrigin::QueuedEdit;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.elapsed = (self.elapsed + f64::from(dt)).min(0.5);
            if candidate.is_none()
                && self.elapsed >= 0.5
                && let Some(path) = self.path.as_ref()
            {
                self.elapsed = 0.;
                match read_source(path) {
                    Ok(bytes) => {
                        if self.last_file.as_ref() != Some(&bytes) {
                            self.last_file = Some(bytes.clone());
                            match String::from_utf8(bytes) {
                                Ok(text) => {
                                    candidate = Some(text);
                                    origin = ReloadOrigin::NativeFile;
                                }
                                Err(_) => self.reject(
                                    ContentError::domain("content file is not UTF-8"),
                                    ReloadOrigin::NativeFile,
                                    ReloadStage::Decode,
                                ),
                            }
                        }
                    }
                    Err((stage, error)) => {
                        // Reinspect restored bytes, even if they match the last
                        // file read before this I/O/input failure.
                        self.last_file = None;
                        self.reject(error, ReloadOrigin::NativeFile, stage);
                    }
                }
            }
        }
        let source = candidate?;
        if source == self.current {
            status(
                ReloadStatus::Ready,
                format!("Content unchanged; generation {}.", self.generation),
                None,
            );
            return None;
        }
        let Some(generation) = self.generation.checked_add(1) else {
            self.reject(
                ContentError::domain("content generation exhausted"),
                origin,
                ReloadStage::Generation,
            );
            return None;
        };
        match build(&source) {
            Ok(next) => {
                self.current = source;
                self.generation = generation;
                BRIDGE.with(|b| {
                    let mut b = b.borrow_mut();
                    b.source.clone_from(&self.current);
                    b.generation = generation;
                });
                status(
                    ReloadStatus::Ready,
                    format!("Loaded content generation {generation}; game restarted."),
                    None,
                );
                BRIDGE.with(|b| {
                    let mut b = b.borrow_mut();
                    b.last_good_revision = b.revision;
                });
                Some(next)
            }
            Err(error) => {
                self.reject(error, origin, ReloadStage::Build);
                None
            }
        }
    }
    fn reject(&self, error: ContentError, origin: ReloadOrigin, stage: ReloadStage) {
        let failure = ReloadFailure::new(origin, stage, error);
        let message = format!(
            "Rejected content; generation {} preserved.\n{}{}",
            self.generation,
            failure.error,
            if failure.context_truncated {
                " [context truncated]"
            } else {
                ""
            },
        );
        #[cfg(not(target_arch = "wasm32"))]
        if BRIDGE.with(|b| b.borrow().status != message) {
            eprintln!("{message}");
        }
        status(ReloadStatus::Rejected, message, Some(failure));
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn read_source(path: &std::path::Path) -> Result<Vec<u8>, (ReloadStage, ContentError)> {
    use std::io::Read;
    let read_error = |e| {
        (
            ReloadStage::Read,
            ContentError::domain(format!("{}: {e}", path.display())),
        )
    };
    let file = std::fs::File::open(path).map_err(read_error)?;
    let mut bytes = Vec::new();
    file.take((MAX_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    if bytes.len() > MAX_SOURCE_BYTES || bytes.contains(&0) {
        return Err((
            ReloadStage::Input,
            ContentError::domain("content file exceeds 2 MiB or contains NUL"),
        ));
    }
    Ok(bytes)
}
#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    /// Changes when a game selects another authored document, not on live edits.
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_document() -> u32 {
        BRIDGE.with(|b| b.borrow().document)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_source_len() -> u32 {
        BRIDGE.with(|b| b.borrow().source.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_source() -> *const u8 {
        BRIDGE.with(|b| b.borrow().source.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_generation() -> u32 {
        BRIDGE.with(|b| b.borrow().generation)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_status() -> u32 {
        BRIDGE.with(|b| b.borrow().reload_status as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_revision() -> u32 {
        BRIDGE.with(|b| b.borrow().revision)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_message_len() -> u32 {
        BRIDGE.with(|b| b.borrow().status.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_message() -> *const u8 {
        BRIDGE.with(|b| b.borrow().status.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_edit_buffer(len: u32) -> *mut u8 {
        if len as usize > MAX_SOURCE_BYTES {
            return std::ptr::null_mut();
        }
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            b.buffer.resize(len as usize, 0);
            b.buffer.as_mut_ptr()
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_content_submit(len: u32) -> u32 {
        let source = BRIDGE.with(|b| {
            let b = b.borrow();
            if len as usize != b.buffer.len() || len as usize > MAX_SOURCE_BYTES {
                return Err((
                    ReloadStage::Input,
                    ContentError::domain("invalid content edit length"),
                ));
            }
            String::from_utf8(b.buffer.clone()).map_err(|_| {
                (
                    ReloadStage::Decode,
                    ContentError::domain("content edit is not UTF-8"),
                )
            })
        });
        match source {
            Ok(source) => u32::from(queue(&source).is_ok()),
            Err((stage, error)) => {
                reject_submission(error, stage);
                0
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::authoring::{ErrorKind, SourceSpan};

    #[test]
    fn inspection_tracks_rejection_recovery_and_last_good_revision() {
        let mut source = ContentSource::new("initial", None).unwrap();
        let initial = reload_state();
        assert_eq!(initial.status, ReloadStatus::Ready);
        assert_eq!(
            (
                initial.generation,
                initial.revision,
                initial.last_good_revision
            ),
            (0, 0, 0)
        );
        assert!(!initial.has_pending_edit);
        assert!(initial.failure.is_none());

        let error = ContentError {
            kind: ErrorKind::Range,
            span: SourceSpan {
                line: 2,
                column: 7,
                offset: 12,
            },
            context: "spawn outside world".into(),
        };
        let mut live = String::from("initial game");
        queue("bad").unwrap();
        let pending = reload_state();
        assert_eq!(pending.status, ReloadStatus::Pending);
        assert!(pending.has_pending_edit);
        assert_eq!(reload_state(), pending); // Inspection does not drain work.
        if let Some(next) = source.poll(0., |_| Err(error.clone())) {
            live = next;
        }
        let rejected = reload_state();
        assert_eq!(rejected.status, ReloadStatus::Rejected);
        assert_eq!(rejected.revision, pending.revision + 1);
        assert_eq!(rejected.last_good_revision, 0);
        assert!(!rejected.has_pending_edit);
        assert_eq!(
            rejected.failure,
            Some(ReloadFailure {
                origin: ReloadOrigin::QueuedEdit,
                stage: ReloadStage::Build,
                error,
                context_truncated: false,
            })
        );
        assert_eq!(live, "initial game");
        assert_eq!(source.current(), "initial");
        assert_eq!(source.generation(), 0);
        assert!(source.poll::<()>(0., |_| panic!("no edit")).is_none());
        assert_eq!(reload_state(), rejected);

        queue("superseded").unwrap();
        queue("good").unwrap();
        assert!(reload_state().failure.is_none());
        let queued_revision = reload_state().revision;
        if let Some(next) = source.poll(0., |text| {
            assert_eq!(text, "good");
            Ok(String::from("new game"))
        }) {
            live = next;
        }
        let accepted = reload_state();
        assert_eq!(live, "new game");
        assert_eq!(accepted.status, ReloadStatus::Ready);
        assert_eq!(accepted.generation, 1);
        assert_eq!(accepted.revision, queued_revision + 1);
        assert_eq!(accepted.last_good_revision, accepted.revision);
        assert!(!accepted.has_pending_edit);
        assert!(accepted.failure.is_none());
        assert!(
            source
                .poll::<()>(0., |_| panic!("already consumed"))
                .is_none()
        );
        assert_eq!(reload_state(), accepted);

        queue("good").unwrap();
        assert!(source.poll::<()>(0., |_| panic!("unchanged")).is_none());
        let unchanged = reload_state();
        assert_eq!(unchanged.status, ReloadStatus::Ready);
        assert_eq!(unchanged.generation, 1);
        assert_eq!(unchanged.last_good_revision, accepted.last_good_revision);
        assert_eq!(unchanged.revision, accepted.revision + 2);
        queue("bad again").unwrap();
        assert!(
            source
                .poll::<()>(0., |_| Err(ContentError::domain("still bad")))
                .is_none()
        );
        assert_eq!(
            reload_state().last_good_revision,
            accepted.last_good_revision
        );
        assert_eq!(source.current(), "good");
        assert_eq!(source.generation(), 1);
        // Previously returned snapshots remain useful after newer operations.
        assert_eq!(rejected.failure.unwrap().error.kind, ErrorKind::Range);
    }

    #[test]
    fn invalid_submissions_preserve_pending_work_and_last_good_content() {
        let mut source = ContentSource::new("initial", None).unwrap();
        queue("good").unwrap();
        assert_eq!(source.poll(0., |_| Ok(1)), Some(1));
        let accepted = reload_state();
        queue("next").unwrap();
        for input in [String::from("\0"), "x".repeat(MAX_SOURCE_BYTES + 1)] {
            let revision = reload_state().revision;
            let error = queue(&input).unwrap_err();
            let state = reload_state();
            assert_eq!(state.status, ReloadStatus::Rejected);
            assert!(state.has_pending_edit);
            assert_eq!(state.generation, 1);
            assert_eq!(state.revision, revision + 1);
            assert_eq!(state.last_good_revision, accepted.last_good_revision);
            let failure = state.failure.unwrap();
            assert_eq!(failure.origin, ReloadOrigin::QueuedEdit);
            assert_eq!(failure.stage, ReloadStage::Input);
            assert_eq!(failure.error, error);
        }
        assert_eq!(source.current(), "good");
        assert_eq!(
            source.poll(0., |text| Ok(text.to_owned())),
            Some("next".into())
        );
        assert_eq!(source.generation(), 2);
        assert!(reload_state().failure.is_none());
    }

    #[test]
    fn invalid_poll_delta_does_not_consume_or_change_diagnostics() {
        let mut source = ContentSource::new("initial", None).unwrap();
        queue("next").unwrap();
        assert!(queue("\0").is_err());
        let before = reload_state();
        for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 60.1] {
            assert!(source.poll::<()>(dt, |_| panic!("invalid dt")).is_none());
            assert_eq!(reload_state(), before);
        }
        assert_eq!(source.poll(0., |_| Ok(42)), Some(42));
        assert_eq!(source.generation(), 1);
    }

    #[test]
    fn accepted_generation_exhaustion_and_status_revision_wrap_are_separate() {
        let mut source = ContentSource::new("initial", None).unwrap();
        source.generation = u32::MAX - 1;
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            b.generation = source.generation;
            b.revision = u32::MAX - 1;
        });
        queue("last").unwrap();
        assert_eq!(source.poll(0., |_| Ok(())), Some(()));
        let accepted = reload_state();
        assert_eq!(accepted.generation, u32::MAX);
        assert_eq!((accepted.revision, accepted.last_good_revision), (0, 0));
        queue("last").unwrap();
        assert!(source.poll::<()>(0., |_| panic!("unchanged")).is_none());
        assert_eq!(reload_state().status, ReloadStatus::Ready);
        queue("overflow").unwrap();
        assert!(
            source
                .poll::<()>(0., |_| panic!("generation overflow"))
                .is_none()
        );
        let state = reload_state();
        assert_eq!(state.status, ReloadStatus::Rejected);
        assert_eq!(state.generation, u32::MAX);
        assert_eq!(state.last_good_revision, accepted.last_good_revision);
        assert_eq!(state.failure.unwrap().stage, ReloadStage::Generation);
        assert_eq!(source.current(), "last");
        assert_eq!(source.generation(), u32::MAX);
        BRIDGE.with(|b| assert_eq!(b.borrow().source, "last"));
    }

    #[test]
    fn selection_resets_diagnostics_and_invalid_selection_preserves_them() {
        let mut source = ContentSource::new("initial", None).unwrap();
        queue("good").unwrap();
        assert_eq!(source.poll(0., |_| Ok(())), Some(()));
        queue("pending").unwrap();
        assert!(queue("\0").is_err());
        let before = reload_state();
        assert!(ContentSource::new("\0", None).is_err());
        assert_eq!(reload_state(), before);
        let mut next = ContentSource::new("another document", None).unwrap();
        let state = reload_state();
        assert_eq!(state.document, before.document.wrapping_add(1));
        assert_eq!(state.status, ReloadStatus::Ready);
        assert_eq!(
            (state.generation, state.revision, state.last_good_revision),
            (0, 0, 0)
        );
        assert!(!state.has_pending_edit);
        assert!(state.failure.is_none());
        assert!(
            next.poll::<()>(0., |_| panic!("old document edit"))
                .is_none()
        );
    }

    #[test]
    fn failure_context_is_bounded_without_splitting_utf8_or_losing_location() {
        let mut source = ContentSource::new("initial", None).unwrap();
        let span = SourceSpan {
            line: 4,
            column: 8,
            offset: 27,
        };
        let prefix = "a".repeat(MAX_RELOAD_CONTEXT_BYTES - 1);
        queue("bad").unwrap();
        assert!(
            source
                .poll::<()>(0., |_| Err(ContentError {
                    kind: ErrorKind::Syntax,
                    span,
                    context: format!("{prefix}🦀rest of error"),
                }))
                .is_none()
        );
        let failure = reload_state().failure.unwrap();
        assert!(failure.context_truncated);
        assert_eq!(failure.error.kind, ErrorKind::Syntax);
        assert_eq!(failure.error.span, span);
        assert_eq!(failure.error.context, prefix);
        BRIDGE.with(|b| {
            let b = b.borrow();
            assert!(b.status.ends_with(" [context truncated]"));
            assert!(
                b.failure.as_ref().unwrap().error.context.capacity() <= MAX_RELOAD_CONTEXT_BYTES
            );
        });
        queue("bad again").unwrap();
        assert!(
            source
                .poll::<()>(0., |_| Err(ContentError::domain(
                    "x".repeat(MAX_RELOAD_CONTEXT_BYTES)
                )))
                .is_none()
        );
        let failure = reload_state().failure.unwrap();
        assert!(!failure.context_truncated);
        assert_eq!(failure.error.context.len(), MAX_RELOAD_CONTEXT_BYTES);
    }

    #[test]
    fn edits_queued_by_builder_remain_visible_and_advance_on_the_next_poll() {
        let mut source = ContentSource::new("initial", None).unwrap();
        queue("first").unwrap();
        assert_eq!(
            source.poll(0., |_| {
                queue("second")?;
                Ok(1)
            }),
            Some(1)
        );
        let state = reload_state();
        assert_eq!(state.status, ReloadStatus::Ready);
        assert!(state.has_pending_edit);
        assert_eq!(state.generation, 1);
        assert_eq!(
            source.poll(0., |text| {
                assert_eq!(text, "second");
                Ok(2)
            }),
            Some(2)
        );
        assert!(!reload_state().has_pending_edit);
        assert_eq!(source.generation(), 2);
    }

    #[test]
    fn bridge_starts_uninitialized_on_other_threads_and_preserves_status_codes() {
        ContentSource::new("main document", None).unwrap();
        queue("pending").unwrap();
        let before = reload_state();
        std::thread::spawn(|| {
            let state = reload_state();
            assert_eq!(state.status, ReloadStatus::Uninitialized);
            assert_eq!(
                (state.document, state.generation, state.revision),
                (0, 0, 0)
            );
            assert!(!state.has_pending_edit);
            assert!(state.failure.is_none());
            ContentSource::new("other document", None).unwrap();
        })
        .join()
        .unwrap();
        assert_eq!(reload_state(), before);
        assert_eq!(ReloadStatus::Uninitialized as u32, 0);
        assert_eq!(ReloadStatus::Ready as u32, 1);
        assert_eq!(ReloadStatus::Pending as u32, 2);
        assert_eq!(ReloadStatus::Rejected as u32, 3);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_failure_stages_and_same_bytes_recovery_preserve_last_good_revision() {
        struct File(std::path::PathBuf);
        impl Drop for File {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let file = File(std::env::temp_dir().join(format!(
            "ferravex-content-inspection-{}-{:?}.fxscene",
            std::process::id(),
            std::thread::current().id(),
        )));
        std::fs::write(&file.0, "good").unwrap();
        let mut source = ContentSource::new("initial", Some(file.0.to_str().unwrap())).unwrap();
        assert_eq!(source.poll(0., |s| Ok(s.to_owned())), Some("good".into()));
        let accepted = reload_state();

        for (bytes, stage) in [
            (vec![0xff], ReloadStage::Decode),
            (vec![0], ReloadStage::Input),
            (vec![b'x'; MAX_SOURCE_BYTES + 1], ReloadStage::Input),
        ] {
            std::fs::write(&file.0, bytes).unwrap();
            assert!(source.poll::<()>(0.5, |_| panic!("invalid file")).is_none());
            let state = reload_state();
            assert_eq!(state.status, ReloadStatus::Rejected);
            assert_eq!(state.generation, 1);
            assert_eq!(state.last_good_revision, accepted.last_good_revision);
            let failure = state.failure.unwrap();
            assert_eq!(failure.origin, ReloadOrigin::NativeFile);
            assert_eq!(failure.stage, stage);
            assert_eq!(source.current(), "good");
        }

        std::fs::write(&file.0, "good").unwrap();
        assert!(source.poll::<()>(0.5, |_| panic!("unchanged")).is_none());
        assert_eq!(reload_state().status, ReloadStatus::Ready);
        std::fs::remove_file(&file.0).unwrap();
        assert!(source.poll::<()>(0.5, |_| panic!("missing file")).is_none());
        let failure = reload_state().failure.unwrap();
        assert_eq!(failure.origin, ReloadOrigin::NativeFile);
        assert_eq!(failure.stage, ReloadStage::Read);
        assert!(failure.error.context.contains(file.0.to_str().unwrap()));
        std::fs::write(&file.0, "good").unwrap();
        assert!(
            source
                .poll::<()>(0.5, |_| panic!("restored identical source"))
                .is_none()
        );
        let restored = reload_state();
        assert_eq!(restored.status, ReloadStatus::Ready);
        assert!(restored.failure.is_none());
        assert_eq!(restored.generation, 1);
        assert_eq!(restored.last_good_revision, accepted.last_good_revision);

        std::fs::write(&file.0, "bad").unwrap();
        assert!(
            source
                .poll::<()>(0.5, |_| Err(ContentError::domain("invalid scene")))
                .is_none()
        );
        let rejected = reload_state();
        let failure = rejected.failure.as_ref().unwrap();
        assert_eq!(failure.origin, ReloadOrigin::NativeFile);
        assert_eq!(failure.stage, ReloadStage::Build);
        assert!(
            source
                .poll::<()>(0.5, |_| panic!("same failed file"))
                .is_none()
        );
        assert_eq!(reload_state(), rejected);
        std::fs::write(&file.0, "next").unwrap();
        assert_eq!(source.poll(0.5, |s| Ok(s.to_owned())), Some("next".into()));
        let recovered = reload_state();
        assert_eq!(recovered.generation, 2);
        assert_eq!(recovered.status, ReloadStatus::Ready);
        assert!(recovered.failure.is_none());
        assert_eq!(recovered.last_good_revision, recovered.revision);
        assert_eq!(source.current(), "next");
    }

    #[test]
    fn reload_builds_before_adoption_and_rejected_edits_preserve_generation() {
        let mut source = ContentSource::new("initial", None).unwrap();
        queue("bad").unwrap();
        assert!(
            source
                .poll::<u32>(0., |_| Err(ContentError::domain("domain validation")))
                .is_none()
        );
        assert_eq!(source.current(), "initial");
        assert_eq!(source.generation(), 0);
        queue("valid").unwrap();
        assert_eq!(source.poll(0., |s| Ok(s.len())), Some(5));
        assert_eq!(source.generation(), 1);
        assert_eq!(source.current(), "valid");
        queue("valid").unwrap();
        assert!(
            source
                .poll::<u32>(0., |_| panic!("unchanged source must not rebuild"))
                .is_none()
        );
        assert!(queue(&"x".repeat(MAX_SOURCE_BYTES + 1)).is_err());
        assert!(queue("\0").is_err());
    }
    #[test]
    fn explicit_file_watch_detects_same_length_edits_and_keeps_last_good_data() {
        let file = std::env::temp_dir().join(format!(
            "ferravex-content-{}-{:?}.fxscene",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&file, "new").unwrap();
        let mut source = ContentSource::new("old", file.to_str()).unwrap();
        assert_eq!(source.poll(0., |v| Ok(v.to_owned())), Some("new".into()));
        std::fs::write(&file, "bad").unwrap();
        assert!(
            source
                .poll::<String>(0.5, |_| Err(ContentError::domain("bad content")))
                .is_none()
        );
        assert_eq!(source.current(), "new");
        assert_eq!(source.generation(), 1);
        std::fs::write(&file, "yes").unwrap();
        assert_eq!(source.poll(0.5, |v| Ok(v.to_owned())), Some("yes".into()));
        std::fs::remove_file(file).unwrap();
    }
}
