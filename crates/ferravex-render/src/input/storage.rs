use super::MAX_CONFIG_BYTES;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageError {
    Unavailable,
    TooLarge,
    InvalidOwner,
}
fn owner_valid(owner: &str) -> bool {
    crate::storage::valid_name(owner)
}
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn ferravex_controls_len() -> i32;
    fn ferravex_controls_load(ptr: *mut u8, len: usize) -> i32;
    fn ferravex_controls_store(ptr: *const u8, len: usize) -> i32;
}
pub(super) fn load(owner: &str) -> Result<Option<Vec<u8>>, StorageError> {
    if !owner_valid(owner) {
        return Err(StorageError::InvalidOwner);
    }
    #[cfg(target_arch = "wasm32")]
    {
        // SAFETY: host reads/writes only these owned, live byte slices; lengths
        // are bounded before allocating or exposing a writable buffer.
        let len = unsafe { ferravex_controls_len() };
        if len < 0 {
            return Err(StorageError::Unavailable);
        }
        if len == 0 {
            return Ok(None);
        }
        if len as usize > MAX_CONFIG_BYTES {
            return Err(StorageError::TooLarge);
        }
        let mut bytes = vec![0; len as usize];
        if unsafe { ferravex_controls_load(bytes.as_mut_ptr(), bytes.len()) } != 0 {
            return Err(StorageError::Unavailable);
        }
        Ok(Some(bytes))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        load_path(&path(owner)?)
    }
}
pub(super) fn save(owner: &str, bytes: &[u8]) -> Result<(), StorageError> {
    if !owner_valid(owner) {
        return Err(StorageError::InvalidOwner);
    }
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(StorageError::TooLarge);
    }
    #[cfg(target_arch = "wasm32")]
    {
        // SAFETY: host receives an immutable live slice for this synchronous call.
        if unsafe { ferravex_controls_store(bytes.as_ptr(), bytes.len()) } == 0 {
            Ok(())
        } else {
            Err(StorageError::Unavailable)
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        save_path(&path(owner)?, bytes)
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn path(owner: &str) -> Result<std::path::PathBuf, StorageError> {
    fn absolute(var: &str) -> Option<std::path::PathBuf> {
        std::env::var_os(var)
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
    }
    #[cfg(target_os = "windows")]
    let root = absolute("APPDATA");
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let root = absolute("HOME").map(|p| p.join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
    let root = absolute("XDG_CONFIG_HOME").or_else(|| absolute("HOME").map(|p| p.join(".config")));
    Ok(root
        .ok_or(StorageError::Unavailable)?
        .join("ferravex/controls")
        .join(format!("{owner}.fxin")))
}
#[cfg(not(target_arch = "wasm32"))]
fn load_path(path: &std::path::Path) -> Result<Option<Vec<u8>>, StorageError> {
    crate::storage::native::read(path, MAX_CONFIG_BYTES).map_err(|e| {
        if e == crate::storage::StorageError::TooLarge {
            StorageError::TooLarge
        } else {
            StorageError::Unavailable
        }
    })
}
#[cfg(not(target_arch = "wasm32"))]
fn save_path(path: &std::path::Path, bytes: &[u8]) -> Result<(), StorageError> {
    crate::storage::native::atomic_write(path, bytes, MAX_CONFIG_BYTES).map_err(|e| {
        if e == crate::storage::StorageError::TooLarge {
            StorageError::TooLarge
        } else {
            StorageError::Unavailable
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_names_cannot_escape_the_controls_directory() {
        for invalid in [
            "",
            "../strike",
            "foo/bar",
            "a.b",
            "CON",
            "con",
            "com1",
            "lpt9",
        ] {
            assert!(!owner_valid(invalid), "{invalid}");
        }
        assert!(owner_valid("dune-skimmer"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_control_storage_is_bounded_and_replaces_atomically() {
        let dir =
            std::env::temp_dir().join(format!("ferravex-input-storage-{}", std::process::id()));
        let path = dir.join("game.fxin");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load_path(&path).unwrap(), None);
        save_path(&path, b"first").unwrap();
        save_path(&path, b"replacement").unwrap();
        assert_eq!(load_path(&path).unwrap().unwrap(), b"replacement");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let oversized = std::fs::File::create(&path).unwrap();
        oversized.set_len(MAX_CONFIG_BYTES as u64 + 1).unwrap();
        assert_eq!(load_path(&path), Err(StorageError::TooLarge));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
