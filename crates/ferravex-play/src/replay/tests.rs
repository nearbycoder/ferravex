use super::*;
#[derive(Default)]
struct World {
    tick: u64,
    state: u64,
    context: u64,
    broken: bool,
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
        if b.len() != 16 {
            return Err(());
        }
        self.tick = u64::from_le_bytes(b[..8].try_into().unwrap());
        self.state = u64::from_le_bytes(b[8..].try_into().unwrap());
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
        Ok(self.state)
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
