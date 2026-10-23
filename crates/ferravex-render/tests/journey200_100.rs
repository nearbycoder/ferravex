//! Native ABI-contract tests of the actual browser adapter, using synchronous
//! host symbols. This does not claim execution inside a browser or Wasm engine.
#![cfg(not(target_arch = "wasm32"))]

use ferravex_play::persistence::{Document, MAX_SAVE_BYTES, SaveData, SaveError, Value};
use ferravex_render::storage::{Backend, LoadStatus, PlatformStore, Slot, StorageError};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::{Cell, RefCell},
};

// Reuse the public transient backend's production key/limit validation, since
// storage::validate is private. No platform I/O or copied validation rules.
fn validate(owner: &str, slot: &str, limit: usize) -> Result<(), StorageError> {
    PlatformStore::transient()
        .load(owner, slot, limit)
        .map(|_| ())
}

// The production module is registered only for Wasm. Compile the identical
// source against native test symbols without changing the crate registration.
#[path = "../src/storage/browser.rs"]
mod browser;

thread_local! {
    static HOST: RefCell<Host> = RefCell::new(Host::default());
    static FAIL_ALLOCATION_SIZE: Cell<Option<usize>> = const { Cell::new(None) };
}

struct FailingAllocator;

fn fail_allocation(size: usize) -> bool {
    FAIL_ALLOCATION_SIZE
        .try_with(|fail| {
            if fail.get() == Some(size) {
                fail.set(None);
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
}

// SAFETY: All successful operations forward the identical layout and pointer to
// System. The thread-local, one-shot failure returns null as GlobalAlloc permits.
unsafe impl GlobalAlloc for FailingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if fail_allocation(layout.size()) {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc(layout) }
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if fail_allocation(layout.size()) {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc_zeroed(layout) }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if fail_allocation(size) {
            std::ptr::null_mut()
        } else {
            unsafe { System.realloc(ptr, layout, size) }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: FailingAllocator = FailingAllocator;

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Len(usize),
    Load(usize),
    Store(usize),
}

#[derive(Default)]
struct Host {
    saved: Option<Vec<u8>>,
    snapshot: Option<Vec<u8>>,
    calls: Vec<Call>,
    names: Option<(Vec<u8>, Vec<u8>)>,
    len_result: Option<i32>,
    load_result: i32,
    store_result: i32,
    fail_read_allocation: bool,
}

fn setup(saved: Option<Vec<u8>>) {
    HOST.with(|host| {
        *host.borrow_mut() = Host {
            saved,
            ..Host::default()
        };
    });
}

// SAFETY: Called only with the live name slices provided by browser.rs; copying
// them synchronously mirrors the existing host ABI without retaining pointers.
unsafe fn names(owner: *const u8, owner_len: usize, slot: *const u8, slot_len: usize) {
    let owner = unsafe { std::slice::from_raw_parts(owner, owner_len) }.to_vec();
    let slot = unsafe { std::slice::from_raw_parts(slot, slot_len) }.to_vec();
    HOST.with(|host| host.borrow_mut().names = Some((owner, slot)));
}

#[unsafe(no_mangle)]
unsafe extern "C" fn ferravex_save_len(
    owner: *const u8,
    owner_len: usize,
    slot: *const u8,
    slot_len: usize,
    limit: usize,
) -> i32 {
    unsafe { names(owner, owner_len, slot, slot_len) };
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.calls.push(Call::Len(limit));
        if let Some(code) = host.len_result {
            return code;
        }
        if host.saved.as_ref().is_some_and(|bytes| bytes.len() > limit) {
            return -2;
        }
        host.snapshot = host.saved.clone();
        let len = host.snapshot.as_ref().map_or(0, Vec::len);
        if host.fail_read_allocation && len > 0 {
            host.fail_read_allocation = false;
            FAIL_ALLOCATION_SIZE.with(|fail| fail.set(Some(len)));
        }
        len as i32
    })
}

#[unsafe(no_mangle)]
unsafe extern "C" fn ferravex_save_load(
    owner: *const u8,
    owner_len: usize,
    slot: *const u8,
    slot_len: usize,
    out: *mut u8,
    len: usize,
) -> i32 {
    unsafe { names(owner, owner_len, slot, slot_len) };
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.calls.push(Call::Load(len));
        // SAFETY: The adapter allocated and initialized this exact live buffer.
        let out = unsafe { std::slice::from_raw_parts_mut(out, len) };
        assert!(out.iter().all(|byte| *byte == 0));
        if host.load_result != 0 {
            // A misbehaving host may fill part of the buffer before failing.
            out[0] = 0xff;
            return host.load_result;
        }
        let Some(snapshot) = &host.snapshot else {
            return -1;
        };
        if snapshot.len() != len {
            return -1;
        }
        out.copy_from_slice(snapshot);
        0
    })
}

#[unsafe(no_mangle)]
unsafe extern "C" fn ferravex_save_store(
    owner: *const u8,
    owner_len: usize,
    slot: *const u8,
    slot_len: usize,
    bytes: *const u8,
    len: usize,
) -> i32 {
    unsafe { names(owner, owner_len, slot, slot_len) };
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.calls.push(Call::Store(len));
        if host.store_result != 0 {
            return host.store_result;
        }
        // SAFETY: The adapter supplies the caller's live immutable byte slice.
        let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };
        host.saved = Some(bytes.to_vec());
        host.snapshot = host.saved.clone();
        0
    })
}

struct BrowserBackend;

impl Backend for BrowserBackend {
    fn load(&self, owner: &str, slot: &str, limit: usize) -> Result<Option<Vec<u8>>, StorageError> {
        browser::load(owner, slot, limit)
    }

    fn store(&self, owner: &str, slot: &str, bytes: &[u8]) -> Result<(), StorageError> {
        browser::store(owner, slot, bytes)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Score(u64);

impl SaveData for Score {
    const SCHEMA: u64 = 100;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 256;

    fn write(&self, document: &mut Document) -> Result<(), SaveError> {
        document.set("score", Value::U64(self.0))
    }

    fn read(document: &Document) -> Result<Self, SaveError> {
        Ok(Self(document.u64("score")?))
    }
}

#[test]
fn invalid_keys_and_bounds_are_typed_before_any_host_call() {
    setup(None);
    let long = "x".repeat(65);
    for invalid in ["", "../game", "Game", "con", "lpt9", "game\n", "é", &long] {
        for (owner, slot) in [(invalid, "records"), ("game", invalid)] {
            assert_eq!(
                browser::load(owner, slot, 256),
                Err(StorageError::InvalidKey)
            );
            assert_eq!(
                browser::store(owner, slot, b"data"),
                Err(StorageError::InvalidKey)
            );
        }
    }
    for limit in [0, MAX_SAVE_BYTES + 1, usize::MAX] {
        assert_eq!(
            browser::load("game", "records", limit),
            Err(StorageError::TooLarge)
        );
    }
    assert_eq!(
        browser::store("game", "records", &vec![0; MAX_SAVE_BYTES + 1]),
        Err(StorageError::TooLarge)
    );
    assert_eq!(
        browser::store("game", "records", b""),
        Err(StorageError::Invalid(SaveError::Capacity))
    );
    HOST.with(|host| assert!(host.borrow().calls.is_empty()));
}

#[test]
fn missing_and_length_errors_never_attempt_a_load_or_store() {
    for (code, expected) in [
        (0, Ok(None)),
        (-1, Err(StorageError::Unavailable)),
        (-2, Err(StorageError::TooLarge)),
        (-3, Err(StorageError::Unavailable)),
        (i32::MIN, Err(StorageError::Unavailable)),
        (257, Err(StorageError::TooLarge)),
        (i32::MAX, Err(StorageError::TooLarge)),
    ] {
        setup(None);
        HOST.with(|host| host.borrow_mut().len_result = Some(code));
        assert_eq!(browser::load("game", "records", 256), expected);
        HOST.with(|host| assert_eq!(host.borrow().calls, [Call::Len(256)]));
    }
}

#[test]
fn exact_limit_and_binary_envelope_pass_through_unchanged() {
    let bytes = Score(42).encode(9).unwrap();
    setup(None);
    assert_eq!(browser::load("game", "records", bytes.len()), Ok(None));
    browser::store("game", "records", &bytes).unwrap();
    assert_eq!(
        browser::load("game", "records", bytes.len()),
        Ok(Some(bytes.clone()))
    );
    HOST.with(|host| {
        let host = host.borrow();
        assert_eq!(host.saved.as_ref(), Some(&bytes));
        assert_eq!(host.snapshot.as_ref(), Some(&bytes));
        assert_eq!(host.names, Some((b"game".to_vec(), b"records".to_vec())));
        assert_eq!(
            host.calls,
            [
                Call::Len(bytes.len()),
                Call::Store(bytes.len()),
                Call::Len(bytes.len()),
                Call::Load(bytes.len())
            ]
        );
    });
}

#[test]
fn every_nonzero_load_or_store_code_is_failure_even_after_partial_copy() {
    let bytes = Score(42).encode(9).unwrap();
    for code in [-1, -2, -3, 1, i32::MIN, i32::MAX] {
        setup(Some(bytes.clone()));
        HOST.with(|host| {
            let mut host = host.borrow_mut();
            host.load_result = code;
            host.store_result = code;
        });
        assert_eq!(
            browser::load("game", "records", 256),
            Err(StorageError::Unavailable)
        );
        assert_eq!(
            browser::store("game", "records", b"replacement"),
            Err(StorageError::Unavailable)
        );
        HOST.with(|host| {
            let host = host.borrow();
            assert_eq!(host.saved.as_ref(), Some(&bytes));
            assert_eq!(host.snapshot.as_ref(), Some(&bytes));
            assert_eq!(
                host.calls,
                [Call::Len(256), Call::Load(bytes.len()), Call::Store(11)]
            );
        });
    }
}

#[test]
fn allocation_failure_returns_unavailable_without_loading_and_can_be_retried() {
    let bytes = vec![0xab; 4093];
    setup(Some(bytes.clone()));
    HOST.with(|host| host.borrow_mut().fail_read_allocation = true);
    assert_eq!(
        browser::load("game", "records", bytes.len()),
        Err(StorageError::Unavailable)
    );
    FAIL_ALLOCATION_SIZE.with(|fail| assert_eq!(fail.get(), None));
    HOST.with(|host| {
        let host = host.borrow();
        assert_eq!(host.calls, [Call::Len(bytes.len())]);
        assert_eq!(host.saved.as_ref(), Some(&bytes));
        assert_eq!(host.snapshot.as_ref(), Some(&bytes));
    });
    assert_eq!(
        browser::load("game", "records", bytes.len()),
        Ok(Some(bytes))
    );
}

#[test]
fn failed_commit_preserves_the_accepted_value_generation_and_exact_bytes() {
    let bytes = Score(42).encode(9).unwrap();
    setup(Some(bytes.clone()));
    let mut slot = Slot::with_backend("game", "records", Score(0), BrowserBackend).unwrap();
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.calls.clear();
        // Both quota and blocked storage return this same code in the real host.
        host.store_result = -1;
    });
    assert_eq!(slot.commit(Score(99)), Err(StorageError::Unavailable));
    assert_eq!(slot.value(), &Score(42));
    assert_eq!(slot.generation(), 9);
    assert_eq!(slot.status(), LoadStatus::Loaded);
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        assert_eq!(host.saved.as_ref(), Some(&bytes));
        assert_eq!(host.snapshot.as_ref(), Some(&bytes));
        assert_eq!(host.calls, [Call::Store(bytes.len())]);
        host.store_result = 0;
    });
    slot.commit(Score(99)).unwrap();
    let reopened = Slot::with_backend("game", "records", Score(0), BrowserBackend).unwrap();
    assert_eq!(reopened.value(), &Score(99));
    assert_eq!(reopened.generation(), 10);
}

#[test]
fn failed_read_does_not_use_stale_cache_and_failed_reset_keeps_protection() {
    let bytes = Score(42).encode(9).unwrap();
    for (code, error) in [
        (-1, StorageError::Unavailable),
        (-2, StorageError::TooLarge),
    ] {
        setup(Some(bytes.clone()));
        browser::load("game", "records", 256).unwrap();
        HOST.with(|host| {
            let mut host = host.borrow_mut();
            host.calls.clear();
            host.len_result = Some(code);
            host.store_result = -1;
        });
        let mut slot = Slot::with_backend("game", "records", Score(7), BrowserBackend).unwrap();
        assert_eq!(slot.value(), &Score(7));
        assert_eq!(slot.generation(), 0);
        assert_eq!(slot.status(), LoadStatus::Rejected(error));
        assert_eq!(slot.commit(Score(99)), Err(StorageError::Protected));
        HOST.with(|host| assert_eq!(host.borrow().calls, [Call::Len(256)]));
        assert_eq!(slot.reset(Score(99)), Err(StorageError::Unavailable));
        assert_eq!(slot.status(), LoadStatus::Rejected(error));
        assert_eq!(slot.value(), &Score(7));
        assert_eq!(slot.generation(), 0);
        assert_eq!(slot.commit(Score(99)), Err(StorageError::Protected));
        HOST.with(|host| {
            let mut host = host.borrow_mut();
            assert_eq!(host.saved.as_ref(), Some(&bytes));
            assert_eq!(host.snapshot.as_ref(), Some(&bytes));
            assert_eq!(host.calls, [Call::Len(256), Call::Store(bytes.len())]);
            host.store_result = 0;
        });
        slot.reset(Score(99)).unwrap();
        assert_eq!(slot.value(), &Score(99));
        assert_eq!(slot.generation(), 1);
        assert_eq!(slot.status(), LoadStatus::Loaded);
        slot.commit(Score(100)).unwrap();
        assert_eq!(slot.generation(), 2);
    }
}

#[test]
fn corrupt_future_and_wrong_schema_binary_saves_are_typed_and_never_auto_overwritten() {
    let mut bad_checksum = Score(42).encode(9).unwrap();
    *bad_checksum.last_mut().unwrap() ^= 1;
    let wrong_schema = Document::new(Score::SCHEMA + 1, 1, 9).unwrap().encode();
    let future = Document::new(Score::SCHEMA, Score::VERSION + 1, 9)
        .unwrap()
        .encode();
    for (bytes, error) in [
        (vec![1, 2, 3], SaveError::Capacity),
        (bad_checksum, SaveError::InvalidData),
        (wrong_schema, SaveError::WrongSchema),
        (future, SaveError::UnsupportedVersion),
    ] {
        setup(Some(bytes.clone()));
        let mut slot = Slot::with_backend("game", "records", Score(7), BrowserBackend).unwrap();
        assert_eq!(slot.value(), &Score(7));
        assert_eq!(slot.generation(), 0);
        assert_eq!(
            slot.status(),
            LoadStatus::Rejected(StorageError::Invalid(error))
        );
        assert_eq!(slot.commit(Score(99)), Err(StorageError::Protected));
        HOST.with(|host| {
            let host = host.borrow();
            assert_eq!(host.saved.as_ref(), Some(&bytes));
            assert_eq!(host.calls, [Call::Len(256), Call::Load(bytes.len())]);
        });
        slot.reset(Score(99)).unwrap();
        HOST.with(|host| {
            assert_eq!(
                Score::decode(host.borrow().saved.as_ref().unwrap()),
                Ok((Score(99), 1))
            );
        });
    }
}
