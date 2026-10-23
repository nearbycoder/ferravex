//! Synchronous browser transport over the existing three save imports.
//!
//! The host reports length/missing/too-large as positive/0/-2; all other length
//! failures and every failed load/store collapse to `Unavailable`. In particular,
//! quota, blocked storage and corrupt base64 cannot be distinguished by this ABI.
//! The JavaScript binding's `status()` supplies that diagnostic detail. Binary
//! envelope/schema validation stays in `SaveData`/`Slot`, where rejected saves
//! remain protected until an explicit reset. Never retry a failed read from the
//! host's last-good cache or turn a failed write into a successful memory update.
use super::{StorageError, validate};
use ferravex_play::persistence::SaveError;

#[cfg_attr(target_arch = "wasm32", link(wasm_import_module = "env"))]
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
    // Validate at the transport boundary too, before touching the host snapshot.
    validate(owner, slot, limit)?;
    if limit == 0 {
        return Err(StorageError::TooLarge);
    }
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
    let mut bytes = Vec::new();
    // Growing Wasm memory can fail even for a permitted payload. Report failure
    // without attempting the load, which would otherwise expose a partial buffer.
    bytes
        .try_reserve_exact(len as usize)
        .map_err(|_| StorageError::Unavailable)?;
    bytes.resize(len as usize, 0);
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
    validate(owner, slot, bytes.len())?;
    if bytes.is_empty() {
        // The host rejects an empty save; it is invalid input, not a storage outage.
        return Err(StorageError::Invalid(SaveError::Capacity));
    }
    // SAFETY: Host reads these immutable live slices synchronously without
    // retaining pointers. Names and payload bounds have been checked above.
    // One atomic host store is the only mutation: no delete, fallback or retry.
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
