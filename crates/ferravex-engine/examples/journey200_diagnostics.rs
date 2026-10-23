//! Run with `cargo run -p ferravex-engine --example journey200_diagnostics
//! --offline --locked -j 2`. Std-only fault injection; no window or save-file I/O.
//! Successful stdout is one complete Diagnostics NDJSON snapshot. Expected
//! failures are assertions, so an unexpected outcome makes the command fail.
use ferravex_engine::{
    assets::Bundle,
    diagnostics::{DiagnosticContext, Diagnostics, Severity},
    persistence::{RecordBook, SaveData, SaveError},
    render::storage::{Backend, LoadStatus, Slot, StorageError},
    replay::{
        EventIter, Header, Limits, Playback, PlaybackError, PlaybackStatus, Recorder, Recording,
        ReplayChecksum, ReplayError, ReplayWorld,
    },
    snapshots::{SnapshotBracket, SnapshotQueryError, SnapshotRing},
};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    fmt::Debug,
    io::{self, Write},
    rc::Rc,
    sync::Arc,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// Some existing errors intentionally expose Debug without std::error::Error.
fn checked<T, E: Debug>(result: std::result::Result<T, E>, operation: &str) -> Result<T> {
    result.map_err(|error| io::Error::other(format!("{operation}: {error:?}")).into())
}

fn log(
    diagnostics: &mut Diagnostics,
    tick: u64,
    severity: Severity,
    code: &str,
    message: &str,
    operation: &str,
    help: &str,
) -> Result<()> {
    assert!(diagnostics.record_with_context(
        tick,
        severity,
        code,
        message,
        DiagnosticContext {
            source: None, // There is no authored source coordinate for these runtime failures.
            operation: Some(operation),
            help: Some(help),
        },
    )?);
    Ok(())
}

fn inspect_assets(diagnostics: &mut Diagnostics) -> Result<(u64, u64)> {
    let mut bundle = Bundle::default();
    bundle.apply_edits_checked([
        ("./rules//counter.txt", Some(&b"checked-u64-add/v1"[..])),
        (r".\meta\title", Some(&b"Journey200 diagnostics"[..])),
    ])?;
    let decoded = Bundle::decode(&bundle.encode())?;
    assert!(bundle.manifest().eq(decoded.manifest()));
    for entry in bundle.manifest() {
        log(
            diagnostics,
            0,
            Severity::Info,
            "asset.manifest",
            &format!(
                "path={} bytes={} fingerprint={:016x}",
                entry.path, entry.byte_len, entry.fingerprint
            ),
            "Bundle::manifest",
            "Compare canonical paths, decoded lengths and fingerprints before replaying.",
        )?;
    }
    let context = bundle
        .manifest()
        .find(|e| e.path == "rules/counter.txt")
        .unwrap()
        .fingerprint;
    let original = bundle.get_checked("rules/counter.txt")?;
    bundle.insert("rules/counter.txt", &original)?;
    assert!(Arc::ptr_eq(
        &original,
        &bundle.get_checked("rules/counter.txt")?
    ));

    // Same byte length, different content. The title is presentation only; this
    // tiny world's sole rules asset is its explicit replay context.
    bundle.insert("rules/counter.txt", b"checked-u64-add/v2")?;
    let changed = bundle
        .manifest()
        .find(|e| e.path == "rules/counter.txt")
        .unwrap()
        .fingerprint;
    assert_ne!(context, changed);
    log(
        diagnostics,
        0,
        Severity::Info,
        "asset.changed",
        &format!(
            "rules/counter.txt: {context:016x} -> {changed:016x}; identical insert retained Arc"
        ),
        "Bundle::insert",
        "A content fingerprint is diagnostic evidence, not authentication or proof of equality.",
    )?;
    Ok((context, changed))
}

#[derive(Debug, PartialEq, Eq)]
struct CounterWorld {
    value: u64,
    context: u64,
}
impl CounterWorld {
    fn apply(&mut self, input: &[u8]) -> std::result::Result<(), &'static str> {
        let [amount] = input else {
            return Err("expected exactly one input byte");
        };
        let next = self
            .value
            .checked_add(u64::from(*amount))
            .ok_or("counter overflow")?;
        self.value = next;
        Ok(())
    }
}
impl ReplayWorld for CounterWorld {
    type Error = &'static str;
    const SCHEMA: u64 = 181;
    const VERSION: u32 = 1;
    const TICK_HZ: u16 = 60;

    fn context(&self) -> std::result::Result<u64, Self::Error> {
        Ok(self.context)
    }
    fn restore(&mut self, state: &[u8]) -> std::result::Result<(), Self::Error> {
        let bytes = state
            .try_into()
            .map_err(|_| "expected eight checkpoint bytes")?;
        self.value = u64::from_le_bytes(bytes);
        Ok(())
    }
    fn step(
        &mut self,
        input: &[u8],
        expected: EventIter<'_>,
    ) -> std::result::Result<(), Self::Error> {
        // This world emits no events. A real game compares accepted events after
        // simulation; it must not apply their damage/rewards a second time.
        if expected.len() != 0 {
            return Err("unexpected recorded event");
        }
        self.apply(input)
    }
    fn state_hash(&self) -> std::result::Result<u64, Self::Error> {
        // All mutable simulation state is this one integer. Its identity is an
        // exact checksum here; real games must cover RNG, queues, timers, etc.
        Ok(self.value)
    }
}

fn record(context: u64) -> Result<Recording> {
    let mut world = CounterWorld { value: 0, context };
    let mut recorder = Recorder::new(
        Header {
            schema: CounterWorld::SCHEMA,
            version: CounterWorld::VERSION,
            context,
            tick_hz: CounterWorld::TICK_HZ,
            seed: 0, // No randomness in this world.
        },
        Limits {
            ticks: 4,
            events: 0,
            checkpoints: 3,
            bytes: 64,
            checkpoint_interval: 2,
            input_bytes: 1,
            event_bytes: 0,
            state_bytes: 8,
        },
        0,
        &world.value.to_le_bytes(),
        world.value,
    )?;
    for amount in 1_u8..=4 {
        let tick = u64::from(amount);
        checked(world.apply(&[amount]), "record tick")?;
        let state = world.value.to_le_bytes();
        recorder.push(
            tick,
            &[amount],
            &[],
            recorder.needs_checkpoint(tick).then_some(&state[..]),
            world.value,
        )?;
    }
    Ok(recorder.finish())
}

fn inspect_replay(context: u64, changed: u64, diagnostics: &mut Diagnostics) -> Result<()> {
    let bytes = record(context)?.encode();
    let tape = Arc::new(Recording::decode(&bytes)?);
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(matches!(
        Recording::decode(&corrupt),
        Err(ReplayError::Checksum)
    ));
    log(
        diagnostics,
        0,
        Severity::Warning,
        "replay.corrupt",
        "Corrupted FXRT0001 rejected with Checksum",
        "Recording::decode",
        "Keep the original clip; a decode failure is different from simulation divergence.",
    )?;

    let mut playback = Playback::new(tape);
    let mut world = CounterWorld {
        value: 0,
        context: changed,
    };
    assert_eq!(playback.start(&mut world), Err(PlaybackError::Incompatible));
    assert_eq!(playback.status(), PlaybackStatus::NotStarted);
    assert_eq!(world.value, 0);
    log(
        diagnostics,
        0,
        Severity::Warning,
        "replay.incompatible",
        "Changed rules context rejected before restore",
        "Playback::start",
        "Match schema, version, tick rate and context; do not bypass compatibility checks.",
    )?;

    world.context = context;
    checked(playback.start(&mut world), "start playback")?;
    let failure = playback.step_with_input(&mut world, &[9]);
    assert!(matches!(
        failure,
        Err(PlaybackError::Desync { tick: 1, .. })
    ));
    let mismatch = playback.first_mismatch().unwrap();
    assert_eq!(mismatch.checksum, ReplayChecksum::Input);
    assert_eq!((playback.position(), world.value), (0, 0));
    assert_eq!(playback.step_with_input(&mut world, &[1]), failure);
    assert_eq!(playback.first_mismatch(), Some(mismatch));
    for _ in 0..2 {
        log(
            diagnostics,
            mismatch.tick,
            Severity::Error,
            "replay.input_mismatch",
            &mismatch.to_string(),
            "Playback::step_with_input",
            "Inspect input bytes and edge consumption, then explicitly restart or seek.",
        )?;
    }

    checked(playback.start(&mut world), "restart playback")?;
    assert!(playback.first_mismatch().is_none());
    let mut history = SnapshotRing::new(3);
    checked(history.push(0, world.value), "capture initial state")?;
    for _ in 0..3 {
        checked(playback.step(&mut world), "verified tick")?;
        checked(
            history.push(playback.position(), world.value),
            "capture verified state",
        )?;
    }
    assert!(history.get(0).is_none()); // Capacity evicts tick 0; ticks 1, 2, 3 remain.
    world.value += 1; // Deliberately perturb live state; the valid recording is untouched.
    let failure = playback.step(&mut world);
    assert_eq!(
        failure,
        Err(PlaybackError::Desync {
            tick: 4,
            expected: 10,
            actual: 11
        })
    );
    let mismatch = playback.first_mismatch().unwrap();
    assert_eq!(mismatch.checksum, ReplayChecksum::State);
    assert_eq!(playback.status(), PlaybackStatus::Faulted);
    assert!(!playback.finished()); // Exhausted rows alone do not establish completion.
    assert_eq!((playback.position(), world.value), (4, 11));
    assert_eq!(playback.step(&mut world), failure);
    assert_eq!((playback.position(), world.value), (4, 11));
    log(
        diagnostics,
        mismatch.tick,
        Severity::Error,
        "replay.state_mismatch",
        &mismatch.to_string(),
        "Playback::step",
        "The failing step already ran; retain the last verified snapshot and restore before continuing.",
    )?;
    inspect_snapshots(&mut history, diagnostics)?;

    // Seek restores checkpoint 2; a budget of zero does no work, then one tick
    // reaches 3. This verifies only the checkpoint and the suffix, not ticks 0..1.
    checked(playback.seek(&mut world, 3), "seek")?;
    assert_eq!((playback.position(), world.value), (2, 3));
    assert_eq!(
        checked(playback.pump_seek(&mut world, 0), "zero seek budget")?,
        0
    );
    assert_eq!(playback.status(), PlaybackStatus::Seeking { target: 3 });
    assert_eq!(
        checked(playback.pump_seek(&mut world, 1), "bounded seek")?,
        1
    );
    assert_eq!((playback.position(), world.value), (3, 6));
    checked(playback.step(&mut world), "verify final tick after seek")?;
    assert!(playback.finished());

    // A full pass provides the stronger claim: every recorded tick verified.
    checked(playback.start(&mut world), "full verification restart")?;
    for _ in 0..4 {
        checked(playback.step(&mut world), "full verification tick")?;
    }
    assert!(playback.finished());
    assert_eq!(world.value, 10);
    log(
        diagnostics,
        4,
        Severity::Info,
        "replay.finished",
        "Finished: all four ticks verified from initial checkpoint; final state=10",
        "Playback::finished",
        "Keep local menus/presentation separate from authoritative replay progression.",
    )?;
    Ok(())
}

fn inspect_snapshots(history: &mut SnapshotRing<u64>, diagnostics: &mut Diagnostics) -> Result<()> {
    assert_eq!(history.push(3, 99), Err(99));
    assert_eq!(
        history.bracket_checked(2, f64::NAN),
        Err(SnapshotQueryError::InvalidFraction)
    );
    assert!(matches!(
        history.bracket_checked(0, 0.0)?,
        SnapshotBracket::BeforeOldest { oldest: (1, _) }
    ));
    assert!(matches!(
        history.bracket_checked(3, 0.0)?,
        SnapshotBracket::Exact { snapshot: (3, &6) }
    ));
    assert!(matches!(
        history.bracket_checked(3, 0.5)?,
        SnapshotBracket::AfterNewest { newest: (3, _) }
    ));
    match history.bracket_checked(2, 0.5)? {
        SnapshotBracket::Between {
            before: (2, &3),
            after: (3, &6),
            alpha,
        } => assert_eq!(alpha, 0.5),
        other => panic!("unexpected bracket: {other:?}"),
    }
    for (tick, fraction) in [(0, 0.0), (2, 0.5), (3, 0.0), (3, 0.5)] {
        log(
            diagnostics,
            tick,
            Severity::Info,
            "snapshot.inspect",
            &format!(
                "query={tick}+{fraction}: {:?}",
                history.bracket_checked(tick, fraction)?
            ),
            "SnapshotRing::bracket_checked",
            "Boundary results do not extrapolate; this ring contains only verified states.",
        )?;
    }
    Ok(())
}

// A private, single-slot fault-injection backend. It implements the actual Slot
// backend boundary; it is not an alternative filesystem/browser save service.
#[derive(Clone, Default)]
struct MemoryStore(Rc<Memory>);
#[derive(Default)]
struct Memory {
    bytes: RefCell<Option<Vec<u8>>>,
    failure: Cell<Option<StorageError>>,
    writes: Cell<usize>,
}
impl Backend for MemoryStore {
    fn load(
        &self,
        owner: &str,
        slot: &str,
        limit: usize,
    ) -> std::result::Result<Option<Vec<u8>>, StorageError> {
        if (owner, slot) != ("journey200", "records") {
            return Err(StorageError::InvalidKey);
        }
        let bytes = self.0.bytes.borrow();
        if limit > RecordBook::MAX_BYTES || bytes.as_ref().is_some_and(|b| b.len() > limit) {
            return Err(StorageError::TooLarge);
        }
        Ok(bytes.clone())
    }
    fn store(
        &self,
        owner: &str,
        slot: &str,
        bytes: &[u8],
    ) -> std::result::Result<(), StorageError> {
        if (owner, slot) != ("journey200", "records") {
            return Err(StorageError::InvalidKey);
        }
        if bytes.len() > RecordBook::MAX_BYTES {
            return Err(StorageError::TooLarge);
        }
        self.0.writes.set(self.0.writes.get() + 1);
        match self.0.failure.get() {
            Some(StorageError::DurabilityUncertain) => {
                *self.0.bytes.borrow_mut() = Some(bytes.to_vec());
                Err(StorageError::DurabilityUncertain)
            }
            Some(error) => Err(error),
            None => {
                *self.0.bytes.borrow_mut() = Some(bytes.to_vec());
                Ok(())
            }
        }
    }
}

fn inspect_save(context: u64, diagnostics: &mut Diagnostics) -> Result<()> {
    let store = MemoryStore::default();
    let mut slot = checked(
        Slot::with_backend(
            "journey200",
            "records",
            RecordBook::default(),
            store.clone(),
        ),
        "open scratch slot",
    )?;
    assert_eq!(slot.status(), LoadStatus::Missing);
    let mut accepted = RecordBook::default();
    checked(
        accepted.record("counter", context, 10, None),
        "record accepted score",
    )?;
    checked(slot.commit(accepted.clone()), "initial commit")?;
    let saved = store.0.bytes.borrow().clone();
    let mut candidate = accepted.clone();
    checked(
        candidate.record("counter", context, 20, None),
        "prepare candidate",
    )?;
    store.0.failure.set(Some(StorageError::Unavailable));
    assert_eq!(
        slot.commit(candidate.clone()),
        Err(StorageError::Unavailable)
    );
    assert_eq!((slot.value(), slot.generation()), (&accepted, 1));
    assert_eq!(*store.0.bytes.borrow(), saved);
    assert_eq!(slot.status(), LoadStatus::Loaded); // Load status is not the last commit result.
    log(
        diagnostics,
        4,
        Severity::Error,
        "save.unavailable",
        "Injected write failure: value, generation=1 and stored bytes unchanged",
        "Slot::commit",
        "Keep the candidate separately; surface the commit error and retry only at an explicit event.",
    )?;
    store.0.failure.set(None);
    checked(slot.commit(candidate.clone()), "explicit retry")?;
    assert_eq!((slot.value(), slot.generation()), (&candidate, 2));

    // Demonstrate the documented exception to ordinary failure rollback.
    store.0.failure.set(Some(StorageError::DurabilityUncertain));
    assert_eq!(
        slot.commit(accepted.clone()),
        Err(StorageError::DurabilityUncertain)
    );
    assert_eq!((slot.value(), slot.generation()), (&accepted, 3));
    let stored = store.0.bytes.borrow();
    assert_eq!(
        checked(
            RecordBook::decode(stored.as_deref().unwrap()),
            "inspect stored generation"
        )?,
        (accepted, 3)
    );
    drop(stored);
    log(
        diagnostics,
        4,
        Severity::Warning,
        "save.durability_uncertain",
        "Injected post-replacement failure: new value accepted at generation=3",
        "Slot::commit",
        "Report uncertain durability; do not assume the old value survived this specific error.",
    )?;

    store.0.failure.set(None);
    *store
        .0
        .bytes
        .borrow_mut()
        .as_mut()
        .unwrap()
        .last_mut()
        .unwrap() ^= 1;
    let rejected_bytes = store.0.bytes.borrow().clone();
    let mut rejected = checked(
        Slot::with_backend(
            "journey200",
            "records",
            RecordBook::default(),
            store.clone(),
        ),
        "reopen corrupt slot",
    )?;
    assert_eq!(
        rejected.status(),
        LoadStatus::Rejected(StorageError::Invalid(SaveError::InvalidData))
    );
    assert!(rejected.value().entries().is_empty());
    let writes = store.0.writes.get();
    assert_eq!(
        rejected.commit(candidate.clone()),
        Err(StorageError::Protected)
    );
    assert_eq!(store.0.writes.get(), writes);
    assert_eq!(*store.0.bytes.borrow(), rejected_bytes);
    log(
        diagnostics,
        4,
        Severity::Error,
        "save.protected",
        "Corrupt save rejected; defaults usable; commit protected original bytes",
        "Slot::with_backend",
        "Preserve the original for recovery. Reset is an explicit replacement decision, not automatic retry.",
    )?;

    // Only this disposable in-memory slot is reset, never a user's save.
    checked(rejected.reset(candidate.clone()), "reset disposable slot")?;
    assert_eq!((rejected.value(), rejected.generation()), (&candidate, 1));
    let restored = checked(
        Slot::with_backend("journey200", "records", RecordBook::default(), store),
        "reload recovered slot",
    )?;
    assert_eq!((restored.value(), restored.generation()), (&candidate, 1));
    log(
        diagnostics,
        4,
        Severity::Info,
        "save.recovered",
        "Disposable slot reset and reloaded at generation=1",
        "Slot::reset",
        "This demonstrates typed slot semantics, not actual disk or browser durability.",
    )?;
    Ok(())
}

fn scenario() -> Result<Diagnostics> {
    let mut diagnostics = Diagnostics::new(32, Severity::Info);
    let (context, changed) = inspect_assets(&mut diagnostics)?;
    inspect_replay(context, changed, &mut diagnostics)?;
    inspect_save(context, &mut diagnostics)?;
    let summary = diagnostics.summary();
    assert_eq!(summary.evicted, 0);
    assert_eq!(
        summary.retained.occurrences,
        summary.retained.records as u64 + 1
    );
    Ok(diagnostics)
}

fn main() -> Result<()> {
    let diagnostics = scenario()?;
    let mut stdout = io::stdout().lock();
    write!(stdout, "{}", diagnostics.ndjson())?;
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faults_recovery_and_inspection_run_together() -> Result<()> {
        let diagnostics = scenario()?;
        assert_eq!(
            diagnostics.summary().for_severity(Severity::Error).records,
            4
        );
        assert_eq!(
            diagnostics
                .summary()
                .for_severity(Severity::Error)
                .occurrences,
            5
        );
        Ok(())
    }

    #[test]
    fn example_world_rejects_malformed_and_overflowing_input_without_mutation() {
        let mut world = CounterWorld {
            value: u64::MAX,
            context: 7,
        };
        assert!(world.apply(&[1]).is_err());
        assert!(world.apply(&[]).is_err());
        assert!(world.apply(&[0, 0]).is_err());
        assert!(world.restore(&[0; 7]).is_err());
        assert_eq!(
            world,
            CounterWorld {
                value: u64::MAX,
                context: 7
            }
        );
    }

    #[test]
    fn scratch_backend_is_bounded_and_scoped() {
        let store = MemoryStore::default();
        assert_eq!(
            store.store("elsewhere", "records", &[1]),
            Err(StorageError::InvalidKey)
        );
        assert_eq!(
            store.store("journey200", "records", &vec![0; RecordBook::MAX_BYTES + 1]),
            Err(StorageError::TooLarge)
        );
        assert_eq!(store.0.writes.get(), 0);
        store.store("journey200", "records", &[1, 2]).unwrap();
        assert_eq!(
            store.load("journey200", "records", 1),
            Err(StorageError::TooLarge)
        );
        assert_eq!(store.load("journey200", "records", 2), Ok(Some(vec![1, 2])));
    }
}
