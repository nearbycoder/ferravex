use super::*;
#[derive(Default)]
struct World {
    tick: u64,
    state: u64,
    context: u64,
    broken: bool,
    fail_restore: bool,
    corrupt_restore: bool,
    fail_hash_at: Option<u64>,
}
impl ReplayWorld for World {
    type Error = ();
    const SCHEMA: u64 = 1;
    const VERSION: u32 = 1;
    const TICK_HZ: u16 = 64;
    fn context(&self) -> Result<u64, ()> {
        Ok(self.context)
    }
    fn restore(&mut self, b: &[u8]) -> Result<(), ()> {
        if self.fail_restore || b.len() != 16 {
            return Err(());
        }
        self.tick = u64::from_le_bytes(b[..8].try_into().unwrap());
        self.state = u64::from_le_bytes(b[8..].try_into().unwrap());
        if self.corrupt_restore {
            self.state ^= 1;
        }
        Ok(())
    }
    fn step(&mut self, input: &[u8], expected: EventIter<'_>) -> Result<(), ()> {
        if input.len() != 1 {
            return Err(());
        }
        self.tick += 1;
        self.state = self
            .state
            .wrapping_add(derive_seed(input[0] as u64, self.tick, 0));
        if self.broken {
            self.state ^= 1;
        }
        assert_eq!(expected.len(), 1);
        Ok(())
    }
    fn state_hash(&self) -> Result<u64, ()> {
        if self.fail_hash_at == Some(self.tick) {
            Err(())
        } else {
            Ok(self.state)
        }
    }
}
impl World {
    fn bytes(&self) -> Vec<u8> {
        [self.tick.to_le_bytes(), self.state.to_le_bytes()].concat()
    }
}
fn tape() -> Recording {
    let mut w = World::default();
    let mut r = Recorder::new(
        Header {
            schema: 1,
            version: 1,
            context: 0,
            tick_hz: 64,
            seed: 7,
        },
        Limits {
            ticks: 50,
            events: 50,
            checkpoints: 11,
            bytes: 10000,
            checkpoint_interval: 5,
            ..Limits::default()
        },
        0,
        &w.bytes(),
        0,
    )
    .unwrap();
    for tick in 1..=50 {
        let input = [tick as u8];
        w.tick = tick;
        w.state = w.state.wrapping_add(derive_seed(input[0] as u64, tick, 0));
        let state = w.bytes();
        r.push(
            tick,
            &input,
            &[EventInput {
                kind: 1,
                payload: &[3],
            }],
            r.needs_checkpoint(tick).then_some(state.as_slice()),
            w.state,
        )
        .unwrap();
    }
    r.finish()
}
#[test]
fn playback_seek_and_desync() {
    let tape = Arc::new(tape());
    let mut w = World::default();
    let mut p = Playback::new(tape.clone());
    p.start(&mut w).unwrap();
    for _ in 0..50 {
        p.step(&mut w).unwrap();
    }
    assert!(p.finished());
    let final_hash = w.state;
    p.seek(&mut w, 23).unwrap();
    assert_eq!(p.position(), 20);
    assert_eq!(p.pump_seek(&mut w, 2).unwrap(), 2);
    assert!(p.seeking());
    assert_eq!(p.pump_seek(&mut w, 2).unwrap(), 1);
    while !p.finished() {
        p.step(&mut w).unwrap();
    }
    assert_eq!(w.state, final_hash);
    p.seek(&mut w, 1).unwrap();
    w.broken = true;
    assert!(matches!(
        p.pump_seek(&mut w, 1),
        Err(PlaybackError::Desync { tick: 1, .. })
    ));
    assert!(p.fault().is_some());
    assert!(!p.seeking());
    assert_eq!(p.pump_seek(&mut w, 64), Ok(0));
    w.broken = false;
    p.seek(&mut w, 0).unwrap();
    p.step(&mut w).unwrap();
    let before = w.state;
    assert_eq!(p.seek(&mut w, 51), Err(PlaybackError::InvalidTarget));
    assert_eq!(w.state, before);
    w.context = 99;
    assert_eq!(
        Playback::new(tape).start(&mut w),
        Err(PlaybackError::Incompatible)
    );
}
#[test]
fn hostile_wire_and_canonical_packing() {
    let t = tape();
    let b = t.encode();
    assert_eq!(Recording::decode(&b).unwrap().encode(), b);
    for n in 0..b.len() {
        assert!(Recording::decode(&b[..n]).is_err());
    }
    for offset in [0, 30, 40, 44, 48, 52, 56, 72, 80, 84, 86, 56 + 50 * 32 + 2] {
        let mut bad = b.clone();
        bad[offset] ^= 0xff;
        let n = bad.len() - 8;
        let h = bad[..n].iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        bad[n..].copy_from_slice(&h.to_le_bytes());
        assert!(Recording::decode(&bad).is_err(), "offset {offset}");
    }
}
#[test]
fn rejected_tick_keeps_exportable_prefix() {
    let mut r = Recorder::new(
        Header {
            schema: 1,
            version: 1,
            context: 0,
            tick_hz: 64,
            seed: 0,
        },
        Limits {
            ticks: 1,
            events: 0,
            checkpoints: 1,
            bytes: 32,
            checkpoint_interval: 2,
            ..Limits::default()
        },
        u64::MAX - 1,
        &[1],
        1,
    )
    .unwrap();
    assert_eq!(r.push(u64::MAX, &[], &[], None, 2), Ok(()));
    assert_eq!(r.push(0, &[], &[], None, 3), Err(ReplayError::TickOrder));
    let b = r.finish().encode();
    assert_eq!(Recording::decode(&b).unwrap().tick_count(), 1);
}

#[test]
fn failed_step_poisoning_prevents_duplicate_authority_application() {
    struct Failing(World);
    impl ReplayWorld for Failing {
        type Error = ();
        const SCHEMA: u64 = 1;
        const VERSION: u32 = 1;
        const TICK_HZ: u16 = 64;
        fn context(&self) -> Result<u64, ()> {
            Ok(0)
        }
        fn restore(&mut self, b: &[u8]) -> Result<(), ()> {
            self.0.restore(b)
        }
        fn state_hash(&self) -> Result<u64, ()> {
            self.0.state_hash()
        }
        fn step(&mut self, _: &[u8], _: EventIter<'_>) -> Result<(), ()> {
            self.0.tick += 1;
            Err(())
        }
    }
    let mut world = Failing(World::default());
    let mut p = Playback::new(Arc::new(tape()));
    p.start(&mut world).unwrap();
    assert_eq!(p.step(&mut world), Err(PlaybackError::State(())));
    assert_eq!(p.step(&mut world), Err(PlaybackError::Faulted));
    assert_eq!(world.0.tick, 1);
    p.seek(&mut world, 0).unwrap();
    assert_eq!(world.0.tick, 0);
}

#[test]
fn divergent_playback_retains_first_state_mismatch_until_restart() {
    let bytes = tape().encode();
    let recording = Arc::new(Recording::decode(&bytes).unwrap());
    let expected = recording.ticks[16].hash;
    let final_hash = recording.ticks.last().unwrap().hash;
    let mut p = Playback::new(recording);
    let mut w = World::default();
    p.start(&mut w).unwrap();
    for tick in 1..17 {
        p.step_with_input(&mut w, &[tick]).unwrap();
    }
    w.broken = true;
    let error = PlaybackError::Desync {
        tick: 17,
        expected,
        actual: expected ^ 1,
    };
    assert_eq!(p.step_with_input(&mut w, &[17]), Err(error.clone()));
    let mismatch = ReplayMismatch {
        tick: 17,
        checksum: ReplayChecksum::State,
        expected,
        actual: expected ^ 1,
    };
    assert_eq!(p.first_mismatch(), Some(mismatch));
    assert_eq!(p.fault(), Some((17, expected, expected ^ 1)));
    assert_eq!(p.status(), PlaybackStatus::Faulted);
    assert!(!p.finished());
    assert_eq!(
        mismatch.to_string(),
        format!(
            "replay state mismatch at tick 17: expected {expected:016x}, actual {:016x}",
            expected ^ 1
        )
    );

    w.broken = false;
    assert_eq!(p.step(&mut w), Err(error.clone()));
    assert_eq!(p.step_with_input(&mut w, &[255]), Err(error));
    assert_eq!((w.tick, w.state), (17, expected ^ 1));
    assert_eq!(p.position(), 17);
    assert_eq!(p.seek(&mut w, 51), Err(PlaybackError::InvalidTarget));
    assert_eq!(p.first_mismatch(), Some(mismatch));

    p.start(&mut w).unwrap();
    assert_eq!(p.first_mismatch(), None);
    assert_eq!((p.position(), w.tick, w.state), (0, 0, 0));
    for _ in 0..50 {
        p.step(&mut w).unwrap();
    }
    assert_eq!(w.state, final_hash);
    assert_eq!(p.status(), PlaybackStatus::Finished);
    assert_eq!(p.recording().encode(), bytes);
}

#[test]
fn input_mismatch_does_not_advance_and_seek_starts_new_verification() {
    let mut p = Playback::new(Arc::new(tape()));
    let mut w = World::default();
    p.start(&mut w).unwrap();
    for tick in 1..7 {
        p.step_with_input(&mut w, &[tick]).unwrap();
    }
    let before = w.state;
    let expected = input_checksum(&[7]);
    let actual = input_checksum(&[7, 0]);
    let error = PlaybackError::Desync {
        tick: 7,
        expected,
        actual,
    };
    assert_eq!(p.step_with_input(&mut w, &[7, 0]), Err(error.clone()));
    assert_eq!(
        p.first_mismatch(),
        Some(ReplayMismatch {
            tick: 7,
            checksum: ReplayChecksum::Input,
            expected,
            actual,
        })
    );
    assert_eq!(p.fault(), Some((7, expected, actual)));
    assert_eq!(p.status(), PlaybackStatus::Faulted);
    assert_eq!(p.step_with_input(&mut w, &[7]), Err(error.clone()));
    assert_eq!(p.step(&mut w), Err(error));
    assert_eq!(p.pump_seek(&mut w, 10), Ok(0));
    assert_eq!((p.position(), w.tick, w.state), (6, 6, before));

    p.seek(&mut w, 7).unwrap();
    assert_eq!(p.first_mismatch(), None);
    assert_eq!(p.fault(), None);
    assert_eq!(p.status(), PlaybackStatus::Seeking { target: 7 });
    let checkpoint_state = w.state;
    assert_eq!(p.seek(&mut w, 51), Err(PlaybackError::InvalidTarget));
    assert_eq!(p.step(&mut w), Err(PlaybackError::Seeking));
    assert_eq!(p.step_with_input(&mut w, &[]), Err(PlaybackError::Seeking));
    assert_eq!(p.pump_seek(&mut w, 0), Ok(0));
    assert_eq!((p.position(), w.tick, w.state), (5, 5, checkpoint_state));
    assert_eq!(p.status(), PlaybackStatus::Seeking { target: 7 });
    assert_eq!(p.pump_seek(&mut w, 1), Ok(1));
    assert!(p.seeking());
    assert_eq!(p.pump_seek(&mut w, 10), Ok(1));
    assert_eq!(p.status(), PlaybackStatus::Playing);
    assert_eq!((p.position(), w.tick), (7, 7));
    assert_eq!(w.state, p.recording().ticks[6].hash);
    assert_eq!(p.pump_seek(&mut w, 10), Ok(0));
}

#[test]
fn reset_cancels_seek_without_touching_world_and_requires_start() {
    let recording = Arc::new(tape());
    let mut p = Playback::new(recording.clone());
    let mut w = World::default();
    p.start(&mut w).unwrap();
    p.seek(&mut w, 38).unwrap();
    p.pump_seek(&mut w, 1).unwrap();
    assert_eq!(p.status(), PlaybackStatus::Seeking { target: 38 });
    let before = (w.tick, w.state);

    p.reset();
    assert_eq!(p.status(), PlaybackStatus::NotStarted);
    assert!(!p.finished());
    assert!(!p.seeking());
    assert_eq!(p.position(), 0);
    assert_eq!(p.first_mismatch(), None);
    assert_eq!(p.step(&mut w), Err(PlaybackError::NotStarted));
    assert_eq!(
        p.step_with_input(&mut w, &[1]),
        Err(PlaybackError::NotStarted)
    );
    assert_eq!(p.seek(&mut w, 0), Err(PlaybackError::NotStarted));
    assert_eq!(p.pump_seek(&mut w, 50), Ok(0));
    assert_eq!((w.tick, w.state), before);
    assert!(std::ptr::eq(p.recording(), recording.as_ref()));

    p.start(&mut w).unwrap();
    assert_eq!((w.tick, w.state), (0, 0));
    assert_eq!(p.status(), PlaybackStatus::Playing);
    for _ in 0..50 {
        p.step(&mut w).unwrap();
    }
    assert!(p.finished());
    let before = (w.tick, w.state);
    assert_eq!(p.step(&mut w), Err(PlaybackError::Ended));
    assert_eq!(p.step_with_input(&mut w, &[]), Err(PlaybackError::Ended));
    assert_eq!((w.tick, w.state), before);
    p.reset();
    p.reset();
    assert_eq!(p.status(), PlaybackStatus::NotStarted);
    assert_eq!((w.tick, w.state), before);
    p.start(&mut w).unwrap();
    p.step(&mut w).unwrap();
    assert_eq!((p.position(), w.tick), (1, 1));
}

#[test]
fn final_tick_divergence_is_not_finished_and_reset_clears_diagnostics() {
    let mut p = Playback::new(Arc::new(tape()));
    let mut w = World::default();
    p.start(&mut w).unwrap();
    for _ in 0..49 {
        p.step(&mut w).unwrap();
    }
    w.broken = true;
    let error = p.step(&mut w).unwrap_err();
    let mismatch = p.first_mismatch().unwrap();
    assert_eq!(
        (mismatch.tick, mismatch.checksum),
        (50, ReplayChecksum::State)
    );
    assert_eq!(p.position(), p.recording().end_tick());
    assert_eq!(p.status(), PlaybackStatus::Faulted);
    assert!(!p.finished());
    assert_eq!(p.step(&mut w), Err(error));

    p.reset();
    assert_eq!(p.first_mismatch(), None);
    assert_eq!(p.fault(), None);
    assert_eq!(p.status(), PlaybackStatus::NotStarted);
    w.broken = false;
    p.start(&mut w).unwrap();
    for _ in 0..50 {
        p.step(&mut w).unwrap();
    }
    assert_eq!(w.state, p.recording().ticks.last().unwrap().hash);
    assert!(p.finished());
}

fn empty_tape(tick: u64) -> Recording {
    let w = World {
        tick,
        state: 42,
        ..World::default()
    };
    Recorder::new(
        Header {
            schema: 1,
            version: 1,
            context: 0,
            tick_hz: 64,
            seed: 0,
        },
        Limits {
            ticks: 0,
            events: 0,
            checkpoints: 1,
            bytes: 16,
            ..Limits::default()
        },
        tick,
        &w.bytes(),
        w.state,
    )
    .unwrap()
    .finish()
}

#[test]
fn empty_recording_finishes_only_after_checkpoint_verification() {
    let mut p = Playback::new(Arc::new(empty_tape(u64::MAX)));
    let mut w = World::default();
    assert_eq!(p.status(), PlaybackStatus::NotStarted);
    assert!(!p.finished());
    assert_eq!(p.step(&mut w), Err(PlaybackError::NotStarted));
    assert_eq!(p.pump_seek(&mut w, 10), Ok(0));
    p.start(&mut w).unwrap();
    assert_eq!((w.tick, w.state), (u64::MAX, 42));
    assert_eq!(p.position(), u64::MAX);
    assert!(p.finished());
    assert_eq!(p.status(), PlaybackStatus::Finished);
    assert_eq!(p.step(&mut w), Err(PlaybackError::Ended));
    assert_eq!(p.step_with_input(&mut w, &[]), Err(PlaybackError::Ended));
    assert_eq!(
        p.seek(&mut w, u64::MAX - 1),
        Err(PlaybackError::InvalidTarget)
    );
    p.seek(&mut w, u64::MAX).unwrap();
    assert!(p.finished());
    assert!(!p.seeking());
    p.reset();
    assert!(!p.finished());
    assert_eq!(p.position(), u64::MAX);
    p.start(&mut w).unwrap();
    assert!(p.finished());
}

#[test]
fn bad_initial_checkpoint_reports_mismatch_even_before_started() {
    let mut p = Playback::new(Arc::new(empty_tape(99)));
    let mut w = World {
        corrupt_restore: true,
        ..World::default()
    };
    let error = PlaybackError::Desync {
        tick: 99,
        expected: 42,
        actual: 43,
    };
    assert_eq!(p.start(&mut w), Err(error.clone()));
    assert_eq!(p.first_mismatch().unwrap().checksum, ReplayChecksum::State);
    assert_eq!(p.status(), PlaybackStatus::Faulted);
    assert!(!p.finished());
    assert_eq!(p.step(&mut w), Err(error));
    w.corrupt_restore = false;
    p.start(&mut w).unwrap();
    assert_eq!(p.first_mismatch(), None);
    assert!(p.finished());
}

#[test]
fn seek_reports_checkpoint_tick_and_first_divergent_replayed_tick() {
    let mut p = Playback::new(Arc::new(tape()));
    let mut w = World::default();
    p.start(&mut w).unwrap();
    w.corrupt_restore = true;
    let error = p.seek(&mut w, 24).unwrap_err();
    let mismatch = p.first_mismatch().unwrap();
    assert_eq!(mismatch.tick, 20);
    assert_eq!(mismatch.checksum, ReplayChecksum::State);
    assert_eq!(mismatch.expected, p.recording().ticks[19].hash);
    assert_eq!(mismatch.actual, w.state);
    assert_eq!(p.status(), PlaybackStatus::Faulted);
    assert!(!p.seeking());
    assert_eq!(p.step(&mut w), Err(error));

    w.corrupt_restore = false;
    p.seek(&mut w, 24).unwrap();
    assert_eq!(p.first_mismatch(), None);
    p.pump_seek(&mut w, 1).unwrap();
    w.broken = true;
    let error = p.pump_seek(&mut w, 100).unwrap_err();
    assert_eq!(p.first_mismatch().unwrap().tick, 22);
    assert_eq!((w.tick, p.position()), (22, 22));
    assert!(!p.seeking());
    assert_eq!(p.pump_seek(&mut w, 100), Ok(0));
    assert_eq!(p.step(&mut w), Err(error));
    assert_eq!(w.tick, 22);
}

#[test]
fn final_hash_failure_is_faulted_before_exhaustion_and_seek_recovers() {
    let mut p = Playback::new(Arc::new(tape()));
    let mut w = World {
        fail_hash_at: Some(50),
        ..World::default()
    };
    p.start(&mut w).unwrap();
    for _ in 0..49 {
        p.step(&mut w).unwrap();
    }
    assert_eq!(p.step(&mut w), Err(PlaybackError::State(())));
    assert_eq!((p.position(), w.tick), (50, 50));
    assert_eq!(p.first_mismatch(), None);
    assert_eq!(p.status(), PlaybackStatus::Faulted);
    assert!(!p.finished());
    assert_eq!(p.step(&mut w), Err(PlaybackError::Faulted));
    assert_eq!(p.step_with_input(&mut w, &[]), Err(PlaybackError::Faulted));

    w.fail_hash_at = None;
    p.seek(&mut w, 50).unwrap();
    assert!(p.finished());
    assert_eq!(p.step(&mut w), Err(PlaybackError::Ended));
}

#[test]
fn failed_restore_at_finished_cursor_requires_explicit_recovery() {
    let mut p = Playback::new(Arc::new(empty_tape(5)));
    let mut w = World::default();
    p.start(&mut w).unwrap();
    assert!(p.finished());
    w.fail_restore = true;
    assert_eq!(p.seek(&mut w, 5), Err(PlaybackError::State(())));
    assert_eq!(p.status(), PlaybackStatus::Faulted);
    assert!(!p.finished());
    assert_eq!(p.step(&mut w), Err(PlaybackError::Faulted));
    p.reset();
    assert_eq!(p.start(&mut w), Err(PlaybackError::State(())));
    assert_eq!(p.step(&mut w), Err(PlaybackError::Faulted));
    assert_eq!(p.status(), PlaybackStatus::Faulted);
    w.fail_restore = false;
    p.start(&mut w).unwrap();
    assert!(p.finished());
}

#[test]
fn input_checksum_is_stable_and_empty_input_mismatch_is_identified() {
    assert_eq!(input_checksum(b""), 0xcbf29ce484222325);
    assert_eq!(input_checksum(b"a"), 0xaf63dc4c8601ec8c);
    assert_eq!(input_checksum(b"hello"), 0xa430d84680aabd0b);
    let mut p = Playback::new(Arc::new(tape()));
    let mut w = World::default();
    p.start(&mut w).unwrap();
    assert!(p.step_with_input(&mut w, &[]).is_err());
    let mismatch = p.first_mismatch().unwrap();
    assert_eq!(mismatch.tick, 1);
    assert_eq!(mismatch.checksum, ReplayChecksum::Input);
    assert_eq!(mismatch.expected, input_checksum(&[1]));
    assert_eq!(mismatch.actual, input_checksum(&[]));
    assert_eq!((p.position(), w.tick, w.state), (0, 0, 0));
}
