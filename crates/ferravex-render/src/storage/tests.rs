use super::*;
use ferravex_play::persistence::{Document, Value};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[cfg(not(target_arch = "wasm32"))]
struct TempSaves(std::path::PathBuf);
#[cfg(not(target_arch = "wasm32"))]
impl TempSaves {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "ferravex-saves-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create temporary saves directory: {e}"),
            }
        }
    }

    fn store(&self) -> FileStore {
        FileStore::new(self.0.clone()).unwrap()
    }

    fn saved(&self) -> std::path::PathBuf {
        self.0.join("game/records.fxsave")
    }

    fn backup(&self) -> std::path::PathBuf {
        self.saved().with_extension("fxsave.bak")
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl Drop for TempSaves {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Record(u64);
impl SaveData for Record {
    const SCHEMA: u64 = 0x7265636f726473;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 256;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        if self.0 > 1000 {
            return Err(SaveError::InvalidData);
        }
        d.set("score", Value::U64(self.0))
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let v = d.u64("score")?;
        if v > 1000 {
            return Err(SaveError::InvalidData);
        }
        Ok(Self(v))
    }
}
#[derive(Default)]
struct Memory {
    bytes: RefCell<Option<Vec<u8>>>,
    reads: Cell<usize>,
    writes: Cell<usize>,
    fail: Cell<bool>,
}
impl Backend for Rc<Memory> {
    fn load(&self, _: &str, _: &str, _: usize) -> Result<Option<Vec<u8>>, StorageError> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.bytes.borrow().clone())
    }
    fn store(&self, _: &str, _: &str, bytes: &[u8]) -> Result<(), StorageError> {
        self.writes.set(self.writes.get() + 1);
        if self.fail.get() {
            return Err(StorageError::Unavailable);
        }
        *self.bytes.borrow_mut() = Some(bytes.into());
        Ok(())
    }
}
#[test]
fn explicit_commits_validate_and_failed_writes_preserve_accepted_value() {
    let backend = Rc::new(Memory::default());
    let mut slot = Slot::with_backend("game", "records", Record(0), backend.clone()).unwrap();
    assert_eq!(slot.status(), LoadStatus::Missing);
    for _ in 0..1000 {
        assert_eq!(slot.value(), &Record(0));
    }
    assert_eq!(backend.reads.get(), 1);
    assert_eq!(backend.writes.get(), 0);
    slot.commit(Record(50)).unwrap();
    let previous = backend.bytes.borrow().clone();
    assert_eq!(
        slot.commit(Record(1001)),
        Err(StorageError::Invalid(SaveError::InvalidData))
    );
    assert_eq!(backend.writes.get(), 1);
    backend.fail.set(true);
    assert_eq!(slot.commit(Record(80)), Err(StorageError::Unavailable));
    assert_eq!(slot.value(), &Record(50));
    assert_eq!(slot.generation(), 1);
    assert_eq!(*backend.bytes.borrow(), previous);
    backend.fail.set(false);
    slot.commit(Record(80)).unwrap();
    assert_eq!(slot.generation(), 2);
    let reloaded = Slot::with_backend("game", "records", Record(0), backend).unwrap();
    assert_eq!(reloaded.value(), &Record(80));
    assert_eq!(reloaded.generation(), 2);
}
#[test]
fn rejected_future_or_corrupt_data_requires_explicit_reset() {
    for bytes in [
        vec![1, 2, 3],
        Document::new(Record::SCHEMA, 2, 19).unwrap().encode(),
    ] {
        let backend = Rc::new(Memory {
            bytes: RefCell::new(Some(bytes.clone())),
            ..Default::default()
        });
        let mut slot = Slot::with_backend("game", "records", Record(0), backend.clone()).unwrap();
        assert!(matches!(slot.status(), LoadStatus::Rejected(_)));
        assert_eq!(slot.value(), &Record(0));
        assert_eq!(slot.commit(Record(30)), Err(StorageError::Protected));
        assert_eq!(*backend.bytes.borrow(), Some(bytes));
        assert_eq!(backend.writes.get(), 0);
        slot.reset(Record(10)).unwrap();
        assert_eq!(slot.value(), &Record(10));
        assert_eq!(slot.generation(), 1);
    }
}
#[test]
fn keys_are_bounded_and_generation_exhaustion_is_not_wrapped() {
    for key in ["", "..", "../x", "CON", "con", "com9", "a/b", "x.y"] {
        assert!(Slot::with_backend(key, "slot", Record(0), PlatformStore::transient()).is_err());
    }
    let backend = Rc::new(Memory {
        bytes: RefCell::new(Some(Record(2).encode(u64::MAX).unwrap())),
        ..Default::default()
    });
    let mut slot = Slot::with_backend("game", "records", Record(0), backend.clone()).unwrap();
    assert_eq!(
        slot.commit(Record(4)),
        Err(StorageError::GenerationExhausted)
    );
    assert_eq!(backend.writes.get(), 0);
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_files_replace_atomically_bound_reads_and_preserve_rejected_saves() {
    let temp = TempSaves::new();
    let root = &temp.0;
    let files = FileStore::new(root.clone()).unwrap();
    assert_eq!(files.load("game", "records", 256).unwrap(), None);
    let mut slot = Slot::with_backend("game", "records", Record(0), files).unwrap();
    slot.commit(Record(12)).unwrap();
    slot.commit(Record(20)).unwrap();
    let saved = root.join("game/records.fxsave");
    let before = std::fs::read(&saved).unwrap();
    let files = FileStore::new(root.clone()).unwrap();
    assert_eq!(
        files.load("game", "records", 1),
        Err(StorageError::TooLarge)
    );
    assert_eq!(
        files.load("../game", "records", 256),
        Err(StorageError::InvalidKey)
    );
    let blocked = root.join("game/blocked.fxsave");
    std::fs::create_dir(&blocked).unwrap();
    assert_eq!(
        files.store("game", "blocked", b"fail"),
        Err(StorageError::Unavailable)
    );
    assert_eq!(std::fs::read(&saved).unwrap(), before);
    assert_eq!(std::fs::read_dir(root.join("game")).unwrap().count(), 3);
    std::fs::write(&saved, b"corrupt").unwrap();
    let mut slot = Slot::with_backend("game", "records", Record(0), files).unwrap();
    assert_eq!(slot.commit(Record(1)), Err(StorageError::Protected));
    assert_eq!(std::fs::read(&saved).unwrap(), b"corrupt");
    slot.reset(Record(1)).unwrap();
    assert!(Record::decode(&std::fs::read(&saved).unwrap()).is_ok());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_recovery_restores_exact_previous_generation_and_preserves_backup() {
    let temp = TempSaves::new();
    let mut slot = Slot::with_backend("game", "records", Record(0), temp.store()).unwrap();
    assert!(!temp.store().recover::<Record>("game", "records").unwrap());
    assert!(!temp.saved().exists());
    slot.commit(Record(12)).unwrap();
    let previous = std::fs::read(temp.saved()).unwrap();
    assert!(!temp.backup().exists());
    slot.commit(Record(20)).unwrap();
    assert_eq!(std::fs::read(temp.backup()).unwrap(), previous);

    // A stopped writer can leave any partial temp file; it is not a committed save.
    let abandoned = temp.0.join("game/.save-abandoned.tmp");
    std::fs::write(&abandoned, b"unfinished replacement").unwrap();
    let healthy = Slot::with_backend("game", "records", Record(0), temp.store()).unwrap();
    assert_eq!(healthy.value(), &Record(20));
    std::fs::write(temp.saved(), &previous[..previous.len() / 2]).unwrap();
    let mut rejected = Slot::with_backend("game", "records", Record(0), temp.store()).unwrap();
    assert!(matches!(rejected.status(), LoadStatus::Rejected(_)));
    assert_eq!(rejected.commit(Record(5)), Err(StorageError::Protected));
    assert!(temp.store().recover::<Record>("game", "records").unwrap());
    assert_eq!(std::fs::read(temp.saved()).unwrap(), previous);
    assert_eq!(std::fs::read(temp.backup()).unwrap(), previous);
    assert_eq!(std::fs::read(abandoned).unwrap(), b"unfinished replacement");
    let restored = Slot::with_backend("game", "records", Record(0), temp.store()).unwrap();
    assert_eq!(restored.status(), LoadStatus::Loaded);
    assert_eq!(restored.value(), &Record(12));
    assert_eq!(restored.generation(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_recovery_rejects_bad_backups_without_touching_the_original() {
    let temp = TempSaves::new();
    let files = temp.store();
    let current = Record(40).encode(8).unwrap();
    files.store("game", "records", &current).unwrap();
    let mut wrong_schema = Document::new(Record::SCHEMA + 1, 1, 7).unwrap();
    wrong_schema.set("score", Value::U64(20)).unwrap();
    let mut invalid_value = Document::new(Record::SCHEMA, 1, 7).unwrap();
    invalid_value.set("score", Value::U64(1001)).unwrap();
    let mut bad_checksum = Record(20).encode(7).unwrap();
    *bad_checksum.last_mut().unwrap() ^= 1;
    let candidates = [
        (Vec::new(), SaveError::Capacity),
        (current[..12].to_vec(), SaveError::Capacity),
        (bad_checksum, SaveError::InvalidData),
        (wrong_schema.encode(), SaveError::WrongSchema),
        (
            Document::new(Record::SCHEMA, 2, 7).unwrap().encode(),
            SaveError::UnsupportedVersion,
        ),
        (invalid_value.encode(), SaveError::InvalidData),
    ];
    for (bytes, expected) in candidates {
        std::fs::write(temp.backup(), &bytes).unwrap();
        assert_eq!(
            files.recover::<Record>("game", "records"),
            Err(StorageError::Invalid(expected))
        );
        assert_eq!(std::fs::read(temp.saved()).unwrap(), current);
        assert_eq!(std::fs::read(temp.backup()).unwrap(), bytes);
    }
    std::fs::write(temp.backup(), vec![0; Record::MAX_BYTES + 1]).unwrap();
    assert_eq!(
        files.recover::<Record>("game", "records"),
        Err(StorageError::TooLarge)
    );
    assert_eq!(std::fs::read(temp.saved()).unwrap(), current);
    assert_eq!(
        files.recover::<Record>("../game", "records"),
        Err(StorageError::InvalidKey)
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_reset_of_malformed_or_oversized_saves_keeps_last_valid_backup() {
    let temp = TempSaves::new();
    let files = temp.store();
    let previous = Record(10).encode(1).unwrap();
    files.store("game", "records", &previous).unwrap();
    files
        .store("game", "records", &Record(20).encode(2).unwrap())
        .unwrap();

    for oversized in [false, true] {
        let mut corrupt = std::fs::File::create(temp.saved()).unwrap();
        if oversized {
            corrupt.set_len(MAX_SAVE_BYTES as u64 + 1).unwrap();
        } else {
            std::io::Write::write_all(&mut corrupt, b"malformed").unwrap();
        }
        drop(corrupt);
        let mut slot = Slot::with_backend("game", "records", Record(0), temp.store()).unwrap();
        assert!(matches!(slot.status(), LoadStatus::Rejected(_)));
        slot.reset(Record(30)).unwrap();
        assert_eq!(std::fs::read(temp.backup()).unwrap(), previous);
        assert_eq!(
            Record::decode(&std::fs::read(temp.saved()).unwrap()),
            Ok((Record(30), 1))
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_backup_failure_preserves_primary_and_accepted_memory_without_temp_leaks() {
    let temp = TempSaves::new();
    let mut slot = Slot::with_backend("game", "records", Record(0), temp.store()).unwrap();
    slot.commit(Record(12)).unwrap();
    let current = std::fs::read(temp.saved()).unwrap();
    std::fs::create_dir(temp.backup()).unwrap();
    assert_eq!(slot.commit(Record(20)), Err(StorageError::Unavailable));
    assert_eq!(slot.value(), &Record(12));
    assert_eq!(slot.generation(), 1);
    assert_eq!(std::fs::read(temp.saved()).unwrap(), current);
    assert_eq!(std::fs::read_dir(temp.0.join("game")).unwrap().count(), 2);
    assert_eq!(
        temp.store().recover::<Record>("game", "records"),
        Err(StorageError::Unavailable)
    );
    std::fs::remove_dir(temp.backup()).unwrap();
    slot.commit(Record(20)).unwrap();
    assert_eq!(std::fs::read(temp.backup()).unwrap(), current);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_recovery_restores_missing_primary_and_can_explicitly_roll_back() {
    let temp = TempSaves::new();
    let files = temp.store();
    let previous = Record(10).encode(3).unwrap();
    files.store("game", "records", &previous).unwrap();
    files
        .store("game", "records", &Record(20).encode(4).unwrap())
        .unwrap();
    assert!(files.recover::<Record>("game", "records").unwrap());
    assert_eq!(std::fs::read(temp.saved()).unwrap(), previous);
    std::fs::remove_file(temp.saved()).unwrap();
    assert!(files.recover::<Record>("game", "records").unwrap());
    assert_eq!(std::fs::read(temp.saved()).unwrap(), previous);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_raw_io_remains_bounded_and_failed_replacements_keep_existing_bytes() {
    let temp = TempSaves::new();
    let path = temp.0.join("new/nested/export.bin");
    native::atomic_write(&path, b"", 0).unwrap();
    assert_eq!(native::read(&path, 0), Ok(Some(Vec::new())));
    native::atomic_write(&path, b"original", 8).unwrap();
    assert_eq!(native::read(&path, 0), Err(StorageError::TooLarge));
    assert_eq!(native::read(&path, 7), Err(StorageError::TooLarge));
    assert_eq!(native::read(&path, 8), Ok(Some(b"original".to_vec())));
    assert_eq!(
        native::read(&path, usize::MAX),
        Ok(Some(b"original".to_vec()))
    );
    assert_eq!(
        native::atomic_write(&path, b"oversized", 8),
        Err(StorageError::TooLarge)
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    let blocked = path.with_file_name("blocked");
    std::fs::create_dir(&blocked).unwrap();
    assert_eq!(native::read(&blocked, 8), Err(StorageError::Unavailable));
    assert_eq!(
        native::atomic_write(&blocked, b"data", 8),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        2
    );
    assert_eq!(
        native::atomic_write(std::path::Path::new("."), b"data", 8),
        Err(StorageError::InvalidKey)
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_concurrent_readers_see_only_complete_replacements() {
    let temp = TempSaves::new();
    let path = temp.0.join("atomic.bin");
    native::atomic_write(&path, &vec![1; 65536], 65536).unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        threads.spawn(|| {
            barrier.wait();
            for _ in 0..200 {
                let bytes = native::read(&path, 65536).unwrap().unwrap();
                assert!(bytes == vec![1; 65536] || bytes == vec![2; 4096]);
            }
        });
        barrier.wait();
        for index in 0..40 {
            let bytes = if index % 2 == 0 {
                vec![2; 4096]
            } else {
                vec![1; 65536]
            };
            native::atomic_write(&path, &bytes, 65536).unwrap();
        }
    });
}

#[test]
fn level_dependent_validation_protects_a_decoded_save_from_automatic_overwrite() {
    let bytes = Record(80).encode(3).unwrap();
    let backend = Rc::new(Memory {
        bytes: RefCell::new(Some(bytes.clone())),
        ..Default::default()
    });
    let mut slot = Slot::with_backend("game", "records", Record(0), backend.clone()).unwrap();
    let result = slot.validate_loaded(|record| {
        if record.0 > 50 {
            Err(SaveError::InvalidData)
        } else {
            Ok(record.0)
        }
    });
    assert_eq!(result, Err(StorageError::Invalid(SaveError::InvalidData)));
    assert_eq!(slot.commit(Record(20)), Err(StorageError::Protected));
    assert_eq!(*backend.bytes.borrow(), Some(bytes));
    assert_eq!(backend.writes.get(), 0);
}
