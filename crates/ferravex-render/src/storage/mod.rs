//! Typed local saves. Filesystem/browser I/O occurs only on explicit load/commit.
use ferravex_play::persistence::{MAX_SAVE_BYTES, SaveData, SaveError};
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::FileStore;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageError {
    Unavailable,
    TooLarge,
    InvalidKey,
    Protected,
    GenerationExhausted,
    Invalid(SaveError),
    DurabilityUncertain,
}
impl From<SaveError> for StorageError {
    fn from(v: SaveError) -> Self {
        Self::Invalid(v)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    Missing,
    Loaded,
    Transient,
    Rejected(StorageError),
}
pub trait Backend {
    fn is_transient(&self) -> bool {
        false
    }
    fn load(&self, owner: &str, slot: &str, limit: usize) -> Result<Option<Vec<u8>>, StorageError>;
    /// DurabilityUncertain means complete data replaced the destination, but the
    /// parent-directory durability check failed. Other errors do not confirm commit.
    fn store(&self, owner: &str, slot: &str, bytes: &[u8]) -> Result<(), StorageError>;
}
pub(crate) fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b))
        && !matches!(value, "con" | "prn" | "aux" | "nul")
        && !(value.len() == 4
            && (value.starts_with("com") || value.starts_with("lpt"))
            && matches!(value.as_bytes()[3], b'1'..=b'9'))
}
fn validate(owner: &str, slot: &str, limit: usize) -> Result<(), StorageError> {
    if !valid_name(owner) || !valid_name(slot) {
        return Err(StorageError::InvalidKey);
    }
    if limit > MAX_SAVE_BYTES {
        return Err(StorageError::TooLarge);
    }
    Ok(())
}
/// Default platform backend. Simulated/autoplay smoke sessions use transient
/// mode so local validation cannot replace the user's records/settings.
pub struct PlatformStore {
    transient: bool,
    #[cfg(not(target_arch = "wasm32"))]
    files: Option<FileStore>,
}
impl Default for PlatformStore {
    fn default() -> Self {
        Self::new()
    }
}
impl PlatformStore {
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let transient = std::env::args()
            .any(|a| matches!(a.as_str(), "--autoplay" | "--smoke-frames" | "--frames"));
        #[cfg(target_arch = "wasm32")]
        let transient = crate::autoplay_enabled();
        Self {
            transient,
            #[cfg(not(target_arch = "wasm32"))]
            files: FileStore::platform().ok(),
        }
    }
    pub fn transient() -> Self {
        Self {
            transient: true,
            #[cfg(not(target_arch = "wasm32"))]
            files: None,
        }
    }
    pub fn is_transient(&self) -> bool {
        self.transient
    }
}
impl Backend for PlatformStore {
    fn is_transient(&self) -> bool {
        self.transient
    }
    fn load(&self, owner: &str, slot: &str, limit: usize) -> Result<Option<Vec<u8>>, StorageError> {
        validate(owner, slot, limit)?;
        if self.transient {
            return Ok(None);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.files
                .as_ref()
                .ok_or(StorageError::Unavailable)?
                .load(owner, slot, limit)
        }
        #[cfg(target_arch = "wasm32")]
        {
            browser::load(owner, slot, limit)
        }
    }
    fn store(&self, owner: &str, slot: &str, bytes: &[u8]) -> Result<(), StorageError> {
        validate(owner, slot, bytes.len())?;
        if self.transient {
            return Ok(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.files
                .as_ref()
                .ok_or(StorageError::Unavailable)?
                .store(owner, slot, bytes)
        }
        #[cfg(target_arch = "wasm32")]
        {
            browser::store(owner, slot, bytes)
        }
    }
}
/// Last accepted in-memory value plus its durable generation. Invalid/future
/// files are preserved until reset() is explicitly requested by the application.
pub struct Slot<T: SaveData, B: Backend = PlatformStore> {
    owner: String,
    name: String,
    value: T,
    generation: u64,
    status: LoadStatus,
    protected: bool,
    backend: B,
}
impl<T: SaveData> Slot<T> {
    pub fn open(owner: &str, name: &str, default: T) -> Result<Self, StorageError> {
        Self::with_backend(owner, name, default, PlatformStore::new())
    }
}
impl<T: SaveData, B: Backend> Slot<T, B> {
    pub fn with_backend(
        owner: &str,
        name: &str,
        default: T,
        backend: B,
    ) -> Result<Self, StorageError> {
        validate(owner, name, T::MAX_BYTES)?;
        default.encode(0)?;
        let mut value = default;
        let mut generation = 0;
        let mut protected = false;
        let status = match backend.load(owner, name, T::MAX_BYTES) {
            Ok(None) => {
                if backend.is_transient() {
                    LoadStatus::Transient
                } else {
                    LoadStatus::Missing
                }
            }
            Ok(Some(bytes)) => match T::decode(&bytes) {
                Ok((loaded, stamp)) => {
                    value = loaded;
                    generation = stamp;
                    LoadStatus::Loaded
                }
                Err(e) => {
                    protected = true;
                    LoadStatus::Rejected(e.into())
                }
            },
            Err(e) => {
                protected = true;
                LoadStatus::Rejected(e)
            }
        };
        Ok(Self {
            owner: owner.into(),
            name: name.into(),
            value,
            generation,
            status,
            protected,
            backend,
        })
    }
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn status(&self) -> LoadStatus {
        self.status
    }
    /// Apply validation that needs the current level/rules. Failure protects the
    /// original bytes from automatic overwrite; a constructed result can be staged.
    pub fn validate_loaded<R>(
        &mut self,
        validate: impl FnOnce(&T) -> Result<R, SaveError>,
    ) -> Result<R, StorageError> {
        match validate(&self.value) {
            Ok(result) => Ok(result),
            Err(e) => {
                self.protected = true;
                self.status = LoadStatus::Rejected(e.into());
                Err(e.into())
            }
        }
    }
    /// Validate/serialize a replacement and commit it atomically. Failed writes
    /// retain the accepted memory value; corrupt/future files are never auto-overwritten.
    pub fn commit(&mut self, next: T) -> Result<(), StorageError> {
        if self.protected {
            return Err(StorageError::Protected);
        }
        self.write(next, false)
    }
    /// Explicitly replace a rejected save with a validated new value.
    pub fn reset(&mut self, next: T) -> Result<(), StorageError> {
        self.write(next, true)
    }
    fn write(&mut self, next: T, reset: bool) -> Result<(), StorageError> {
        let generation = if reset {
            1
        } else {
            self.generation
                .checked_add(1)
                .ok_or(StorageError::GenerationExhausted)?
        };
        let bytes = next.encode(generation)?;
        let result = self.backend.store(&self.owner, &self.name, &bytes);
        if result.is_ok() || result == Err(StorageError::DurabilityUncertain) {
            self.value = next;
            self.generation = generation;
            self.protected = false;
            self.status = if self.backend.is_transient() {
                LoadStatus::Transient
            } else {
                LoadStatus::Loaded
            };
        }
        result
    }
}
mod records;
pub use records::{Record, RecordBook, Records, run_micros};
#[cfg(test)]
mod tests;
