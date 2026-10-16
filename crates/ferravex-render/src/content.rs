//! Optional authoring bridge. A Source polls only explicit local files or queued
//! browser edits; ordinary frames do no filesystem work or source allocation.
use ferravex_play::authoring::{ContentError, MAX_SOURCE_BYTES};
use std::cell::RefCell;
#[derive(Default)]
struct Bridge {
    source: String,
    status: String,
    generation: u32,
    revision: u32,
    document: u32,
    code: u32,
    pending: Option<String>,
    #[cfg(target_arch = "wasm32")]
    buffer: Vec<u8>,
}
thread_local! {static BRIDGE:RefCell<Bridge>=RefCell::new(Bridge::default());}
fn status(code: u32, message: String) {
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        b.code = code;
        b.status = message;
        b.revision = b.revision.wrapping_add(1);
    });
}
/// Queue a single edit; newer unapplied edits replace older ones. Parsing/game
/// validation remains in the game, on its next poll. UTF-8 and size are checked.
pub fn queue(source: &str) -> Result<(), ContentError> {
    if source.len() > MAX_SOURCE_BYTES || source.contains('\0') {
        return Err(ContentError::domain("source exceeds 2 MiB or contains NUL"));
    }
    BRIDGE.with(|b| b.borrow_mut().pending = Some(source.into()));
    status(2, "Pending content validation.".into());
    Ok(())
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
                code: 1,
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
                                Ok(text) => candidate = Some(text),
                                Err(_) => {
                                    self.reject(ContentError::domain("content file is not UTF-8"))
                                }
                            }
                        }
                    }
                    Err(error) => self.reject(error),
                }
            }
        }
        let source = candidate?;
        if source == self.current {
            status(
                1,
                format!("Content unchanged; generation {}.", self.generation),
            );
            return None;
        }
        let Some(generation) = self.generation.checked_add(1) else {
            self.reject(ContentError::domain("content generation exhausted"));
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
                    1,
                    format!("Loaded content generation {generation}; game restarted."),
                );
                Some(next)
            }
            Err(error) => {
                self.reject(error);
                None
            }
        }
    }
    fn reject(&self, error: ContentError) {
        let message = format!(
            "Rejected content; generation {} preserved.\n{error}",
            self.generation
        );
        #[cfg(not(target_arch = "wasm32"))]
        if BRIDGE.with(|b| b.borrow().status != message) {
            eprintln!("{message}");
        }
        status(3, message);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn read_source(path: &std::path::Path) -> Result<Vec<u8>, ContentError> {
    use std::io::Read;
    let file = std::fs::File::open(path)
        .map_err(|e| ContentError::domain(format!("{}: {e}", path.display())))?;
    let mut bytes = Vec::new();
    file.take((MAX_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| ContentError::domain(format!("{}: {e}", path.display())))?;
    if bytes.len() > MAX_SOURCE_BYTES || bytes.contains(&0) {
        return Err(ContentError::domain(
            "content file exceeds 2 MiB or contains NUL",
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
        BRIDGE.with(|b| b.borrow().code)
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
                return Err(ContentError::domain("invalid content edit length"));
            }
            String::from_utf8(b.buffer.clone())
                .map_err(|_| ContentError::domain("content edit is not UTF-8"))
        });
        match source.and_then(|s| queue(&s)) {
            Ok(()) => 1,
            Err(e) => {
                status(3, e.to_string());
                0
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
