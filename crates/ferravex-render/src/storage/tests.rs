use super::*;
use ferravex_play::persistence::{Document, Value};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
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
    let root = std::env::temp_dir().join(format!(
        "ferravex-saves-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
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
    assert_eq!(std::fs::read_dir(root.join("game")).unwrap().count(), 2);
    std::fs::write(&saved, b"corrupt").unwrap();
    let mut slot = Slot::with_backend("game", "records", Record(0), files).unwrap();
    assert_eq!(slot.commit(Record(1)), Err(StorageError::Protected));
    assert_eq!(std::fs::read(&saved).unwrap(), b"corrupt");
    slot.reset(Record(1)).unwrap();
    assert!(Record::decode(&std::fs::read(&saved).unwrap()).is_ok());
    std::fs::remove_dir_all(root).unwrap();
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
