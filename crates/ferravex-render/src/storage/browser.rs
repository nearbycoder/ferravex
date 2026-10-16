use super::*;
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn ferravex_save_len(
        owner: *const u8,
        owner_len: usize,
        slot: *const u8,
        slot_len: usize,
        limit: usize,
    ) -> i32;
    fn ferravex_save_load(
        owner: *const u8,
        owner_len: usize,
        slot: *const u8,
        slot_len: usize,
        out: *mut u8,
        len: usize,
    ) -> i32;
    fn ferravex_save_store(
        owner: *const u8,
        owner_len: usize,
        slot: *const u8,
        slot_len: usize,
        bytes: *const u8,
        len: usize,
    ) -> i32;
}
pub(super) fn load(owner: &str, slot: &str, limit: usize) -> Result<Option<Vec<u8>>, StorageError> {
    // SAFETY: Host receives live bounded UTF-8 slices and synchronously fills only
    // the owned output buffer. Signed errors are checked before allocating.
    let len = unsafe {
        ferravex_save_len(
            owner.as_ptr(),
            owner.len(),
            slot.as_ptr(),
            slot.len(),
            limit,
        )
    };
    if len < 0 {
        return Err(if len == -2 {
            StorageError::TooLarge
        } else {
            StorageError::Unavailable
        });
    }
    if len == 0 {
        return Ok(None);
    }
    if len as usize > limit {
        return Err(StorageError::TooLarge);
    }
    let mut bytes = vec![0; len as usize];
    if unsafe {
        ferravex_save_load(
            owner.as_ptr(),
            owner.len(),
            slot.as_ptr(),
            slot.len(),
            bytes.as_mut_ptr(),
            bytes.len(),
        )
    } != 0
    {
        return Err(StorageError::Unavailable);
    }
    Ok(Some(bytes))
}
pub(super) fn store(owner: &str, slot: &str, bytes: &[u8]) -> Result<(), StorageError> {
    // SAFETY: Host reads these immutable live slices synchronously without
    // retaining pointers; the caller enforces name and payload bounds.
    if unsafe {
        ferravex_save_store(
            owner.as_ptr(),
            owner.len(),
            slot.as_ptr(),
            slot.len(),
            bytes.as_ptr(),
            bytes.len(),
        )
    } == 0
    {
        Ok(())
    } else {
        Err(StorageError::Unavailable)
    }
}
