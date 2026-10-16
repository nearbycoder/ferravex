use super::*;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
/// Files live under root/owner/slot.fxsave. Root must be absolute.
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
        atomic_write(&self.path(owner, slot), bytes, MAX_SAVE_BYTES)
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
    let mut file = std::fs::File::open(path).map_err(|_| StorageError::Unavailable)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut chunk = [0u8; 4096];
    loop {
        let amount = (limit - bytes.len() + 1).min(chunk.len());
        let n = file
            .read(&mut chunk[..amount])
            .map_err(|_| StorageError::Unavailable)?;
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
    let dir = path.parent().ok_or(StorageError::InvalidKey)?;
    std::fs::create_dir_all(dir).map_err(|_| StorageError::Unavailable)?;
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
        replace(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err(StorageError::Unavailable);
    }
    #[cfg(unix)]
    {
        std::fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| StorageError::DurabilityUncertain)?;
    }
    Ok(())
}
#[cfg(not(target_os = "windows"))]
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}
#[cfg(target_os = "windows")]
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(old: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: Both UTF-16 paths are live and NUL-terminated for this synchronous
    // OS call. Same-directory replacement avoids a cross-volume copy fallback.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
