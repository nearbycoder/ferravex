use super::*;
use ferravex_play::persistence::Document;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
/// Files live under root/owner/slot.fxsave. Root must be absolute.
///
/// Replacing an existing, structurally valid save retains its exact bytes in
/// `slot.fxsave.bak`. Malformed or oversized originals never replace that backup.
/// Loads still report the original file, including corruption; recovery is an
/// explicit operation using [`Self::recover`]. Temporary files left by a stopped
/// process are never loaded or promoted. Writers to the same slot should be
/// serialized when the ordering of backup generations matters.
pub struct FileStore {
    root: PathBuf,
}
impl FileStore {
    pub fn new(root: PathBuf) -> Result<Self, StorageError> {
        if !root.is_absolute() {
            return Err(StorageError::InvalidKey);
        }
        Ok(Self { root })
    }
    pub fn platform() -> Result<Self, StorageError> {
        fn absolute(key: &str) -> Option<PathBuf> {
            std::env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        }
        if let Some(root) = absolute("FERRAVEX_SAVE_DIR") {
            return Self::new(root);
        }
        #[cfg(target_os = "windows")]
        let root = absolute("LOCALAPPDATA").or_else(|| absolute("APPDATA"));
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let root = absolute("HOME").map(|p| p.join("Library/Application Support"));
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
        let root =
            absolute("XDG_DATA_HOME").or_else(|| absolute("HOME").map(|p| p.join(".local/share")));
        Self::new(
            root.ok_or(StorageError::Unavailable)?
                .join("ferravex/saves"),
        )
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Restore the previous save only after validating it as `T`.
    ///
    /// Returns `false` when no backup exists. Invalid, incompatible, oversized or
    /// unreadable backups return an error without changing the current file. A
    /// successful recovery copies the backup atomically, preserving its original
    /// generation and the backup itself. Reopen any cached [`Slot`] afterwards.
    /// This also allows an explicit rollback of a healthy current save.
    ///
    /// `DurabilityUncertain` means the replacement completed but synchronizing
    /// its directory failed. Other errors leave the destination uncommitted.
    ///
    /// ```no_run
    /// use ferravex_render::storage::{FileStore, RecordBook, Slot, StorageError};
    ///
    /// # fn restore_records() -> Result<(), StorageError> {
    /// let files = FileStore::platform()?;
    /// if files.recover::<RecordBook>("my-game", "records")? {
    ///     let restored = Slot::with_backend(
    ///         "my-game", "records", RecordBook::default(), files,
    ///     )?;
    ///     assert!(restored.generation() > 0);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn recover<T: SaveData>(&self, owner: &str, slot: &str) -> Result<bool, StorageError> {
        validate(owner, slot, T::MAX_BYTES)?;
        let path = self.path(owner, slot);
        let Some(bytes) = read(&path.with_extension("fxsave.bak"), T::MAX_BYTES)? else {
            return Ok(false);
        };
        T::decode(&bytes)?;
        atomic_write(&path, &bytes, T::MAX_BYTES)?;
        Ok(true)
    }
    fn path(&self, owner: &str, slot: &str) -> PathBuf {
        self.root.join(owner).join(format!("{slot}.fxsave"))
    }
}
impl Backend for FileStore {
    fn load(&self, owner: &str, slot: &str, limit: usize) -> Result<Option<Vec<u8>>, StorageError> {
        validate(owner, slot, limit)?;
        read(&self.path(owner, slot), limit)
    }
    fn store(&self, owner: &str, slot: &str, bytes: &[u8]) -> Result<(), StorageError> {
        validate(owner, slot, bytes.len())?;
        let path = self.path(owner, slot);
        match read(&path, MAX_SAVE_BYTES) {
            Ok(Some(previous)) if Document::decode(&previous).is_ok() => {
                // Finish the backup before touching the original. In particular,
                // uncertain *backup* durability must not tell Slot that the new
                // primary value was committed.
                atomic_write(
                    &path.with_extension("fxsave.bak"),
                    &previous,
                    MAX_SAVE_BYTES,
                )
                .map_err(|_| StorageError::Unavailable)?;
            }
            Ok(_) | Err(StorageError::TooLarge) => {}
            Err(error) => return Err(error),
        }
        atomic_write(&path, bytes, MAX_SAVE_BYTES)
    }
}
pub(crate) fn read(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, StorageError> {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StorageError::Unavailable),
    };
    if !metadata.is_file() {
        return Err(StorageError::Unavailable);
    }
    if metadata.len() > limit as u64 {
        return Err(StorageError::TooLarge);
    }
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StorageError::Unavailable),
    };
    // The path may have been atomically replaced since the first metadata check.
    // Inspect the opened file too, so allocation and bounds describe this read.
    let metadata = file.metadata().map_err(|_| StorageError::Unavailable)?;
    if !metadata.is_file() {
        return Err(StorageError::Unavailable);
    }
    if metadata.len() > limit as u64 {
        return Err(StorageError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut chunk = [0u8; 4096];
    loop {
        let amount = (limit - bytes.len()).min(chunk.len() - 1) + 1;
        let n = match file.read(&mut chunk[..amount]) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(StorageError::Unavailable),
        };
        if n == 0 {
            return Ok(Some(bytes));
        }
        if n > limit - bytes.len() {
            return Err(StorageError::TooLarge);
        }
        if bytes.len() + n > bytes.capacity() {
            bytes.reserve_exact(n);
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8], limit: usize) -> Result<(), StorageError> {
    if bytes.len() > limit {
        return Err(StorageError::TooLarge);
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if path.file_name().is_none() {
        return Err(StorageError::InvalidKey);
    }
    let dir = parent_dir(path);
    create_dir_durable(dir).map_err(|_| StorageError::Unavailable)?;
    let mut temp = None;
    for _ in 0..16 {
        let name = dir.join(format!(
            ".save-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&name)
        {
            Ok(file) => {
                temp = Some((name, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(StorageError::Unavailable),
        }
    }
    let (temp, mut file) = temp.ok_or(StorageError::Unavailable)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // Same-directory rename replaces the destination without an unlink gap
        // on platforms supporting atomic replacement, including Unix/Windows.
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err(StorageError::Unavailable);
    }
    sync_dir(dir).map_err(|_| StorageError::DurabilityUncertain)
}
fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}
fn create_dir_durable(dir: &Path) -> std::io::Result<()> {
    let mut missing = Vec::new();
    let mut ancestor = dir;
    loop {
        match std::fs::metadata(ancestor) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(ancestor);
                let parent = parent_dir(ancestor);
                if parent == ancestor {
                    return Err(e);
                }
                ancestor = parent;
            }
            Err(e) => return Err(e),
        }
    }
    std::fs::create_dir_all(dir)?;
    // Sync newly-created directory entries as well as the final file entry.
    // Otherwise a first save could lose its entire owner/root directory on crash.
    for path in missing.into_iter().rev() {
        sync_dir(parent_dir(path))?;
    }
    Ok(())
}
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir; // std has no portable directory synchronization primitive.
    Ok(())
}
