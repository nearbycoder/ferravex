//! Offline authority replay. Every player's sanitized command is recorded;
//! replay drives the same simulation and compares accepted events once.
use super::*;
use checkpoint::{CheckpointError, MAX_CHECKPOINT_BYTES};
use ferravex_play::replay::{
    self as timeline, EventInput, EventIter, Header, Limits, Recorder, Recording, ReplayWorld,
};
pub const SCHEMA: u64 = 0x4658535452494b45;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Checkpoint(CheckpointError),
    Timeline(timeline::ReplayError),
    InvalidInput,
    EventsDiffer,
}
impl From<CheckpointError> for Error {
    fn from(v: CheckpointError) -> Self {
        Self::Checkpoint(v)
    }
}
impl From<timeline::ReplayError> for Error {
    fn from(v: timeline::ReplayError) -> Self {
        Self::Timeline(v)
    }
}
/// Scratch is reserved once, before recording begins. Simulation/event output
/// allocation remains owned by Match, independently of timeline recording.
pub struct Capture {
    recorder: Recorder,
    state: Vec<u8>,
    input: Vec<u8>,
    events: [Vec<u8>; 64],
    tick: u64,
}
impl Capture {
    pub fn new(game: &Match, seed: u64, limits: Limits) -> Result<Self, Error> {
        Self::with_header(
            game,
            Header {
                schema: SCHEMA,
                version: 1,
                context: game.context_hash()?,
                tick_hz: TICK_RATE as u16,
                seed,
            },
            limits,
        )
    }
    // Narrow additive seam: alternate schemas reuse exactly the legacy
    // checkpoint/input/event encoding and Recorder preparation.
    pub(crate) fn with_header(game: &Match, header: Header, limits: Limits) -> Result<Self, Error> {
        let mut state = Vec::with_capacity(MAX_CHECKPOINT_BYTES);
        game.checkpoint_into(&mut state)?;
        let recorder = Recorder::new(
            header,
            limits,
            u64::from(game.tick),
            &state,
            game.state_hash()?,
        )?;
        Ok(Self {
            recorder,
            state,
            input: Vec::with_capacity(512),
            events: std::array::from_fn(|_| Vec::with_capacity(32)),
            tick: u64::from(game.tick),
        })
    }
    pub fn tick_count(&self) -> usize {
        self.recorder.tick_count()
    }
    /// Call after exactly one authority step with that step's complete commands.
    /// A failed push leaves the previously recorded prefix intact.
    pub fn push(
        &mut self,
        game: &Match,
        commands: &[(u8, InputFrame)],
        shots: &[ShotTime],
        events: &[GameEvent],
    ) -> Result<(), Error> {
        let tick = self.tick.checked_add(1).ok_or(Error::InvalidInput)?;
        if game.tick != tick as u32
            || commands.len() > MAX_PLAYERS
            || shots.len() > MAX_PLAYERS
            || events.len() > 64
        {
            return Err(Error::InvalidInput);
        }
        encode_inputs(commands, shots, &mut self.input)?;
        for (i, event) in events.iter().enumerate() {
            NetworkEvent {
                tick: game.tick,
                event: *event,
            }
            .encode_into(&mut self.events[i])
            .map_err(|_| Error::InvalidInput)?;
        }
        let entries: [EventInput<'_>; 64] = std::array::from_fn(|i| EventInput {
            kind: 1,
            payload: &self.events[i],
        });
        let cp = if self.recorder.needs_checkpoint(tick) {
            game.checkpoint_into(&mut self.state)?;
            Some(self.state.as_slice())
        } else {
            None
        };
        self.recorder.push(
            tick,
            &self.input,
            &entries[..events.len()],
            cp,
            game.state_hash()?,
        )?;
        self.tick = tick;
        Ok(())
    }
    pub fn finish(self) -> Recording {
        self.recorder.finish()
    }
}
fn encode_inputs(
    commands: &[(u8, InputFrame)],
    shots: &[ShotTime],
    out: &mut Vec<u8>,
) -> Result<(), Error> {
    out.clear();
    out.push(commands.len() as u8);
    let mut ids = 0u16;
    for &(id, input) in commands {
        if id as usize >= MAX_PLAYERS || ids & (1 << id) != 0 {
            return Err(Error::InvalidInput);
        }
        ids |= 1 << id;
        let input = input.sanitized();
        out.push(id);
        out.extend(input.sequence.to_le_bytes());
        for f in [input.move_x, input.move_z, input.yaw, input.pitch] {
            out.extend(f.to_le_bytes());
        }
        out.push(input.buttons);
    }
    out.push(shots.len() as u8);
    ids = 0;
    for shot in shots {
        if shot.player as usize >= MAX_PLAYERS || ids & (1 << shot.player) != 0 {
            return Err(Error::InvalidInput);
        }
        ids |= 1 << shot.player;
        out.push(shot.player);
        out.extend(shot.tick.to_le_bytes());
    }
    Ok(())
}
pub struct World {
    pub game: Match,
    pub events: Vec<GameEvent>,
}
impl World {
    pub fn new(game: Match) -> Self {
        Self {
            game,
            events: Vec::with_capacity(64),
        }
    }
}
impl ReplayWorld for World {
    type Error = Error;
    const SCHEMA: u64 = SCHEMA;
    const VERSION: u32 = 1;
    const TICK_HZ: u16 = TICK_RATE as u16;
    fn context(&self) -> Result<u64, Error> {
        Ok(self.game.context_hash()?)
    }
    fn restore(&mut self, state: &[u8]) -> Result<(), Error> {
        let game = Match::from_checkpoint(state)?;
        if game.context_hash()? != self.game.context_hash()? {
            return Err(Error::InvalidInput);
        }
        self.game = game;
        self.events.clear();
        Ok(())
    }
    fn state_hash(&self) -> Result<u64, Error> {
        Ok(self.game.state_hash()?)
    }
    fn step(&mut self, input: &[u8], expected: EventIter<'_>) -> Result<(), Error> {
        let input = decode_inputs(input)?;
        let wanted = decode_events(expected.clone(), self.game.tick.wrapping_add(1))?;
        let step = self.game.step_with_rewind(
            &input.commands[..input.command_count],
            &input.shots[..input.shot_count],
        );
        self.events.clear();
        self.events.extend(step.events);
        compare_events(&self.events, &wanted, expected.len())
    }
}
// Fixed-size shared input and event validation; no alternate codec.
pub(crate) struct DecodedInputs {
    pub commands: [(u8, InputFrame); MAX_PLAYERS],
    pub command_count: usize,
    pub shots: [ShotTime; MAX_PLAYERS],
    pub shot_count: usize,
}
pub(crate) fn decode_inputs(input: &[u8]) -> Result<DecodedInputs, Error> {
    let mut r = checkpoint::Reader { b: input, i: 0 };
    let n = r.u8()? as usize;
    if n > MAX_PLAYERS {
        return Err(Error::InvalidInput);
    }
    let mut commands = [(0, InputFrame::default()); MAX_PLAYERS];
    let mut ids = 0u16;
    for c in &mut commands[..n] {
        let id = r.u8()?;
        let frame = InputFrame {
            sequence: r.u32()?,
            move_x: r.f32()?,
            move_z: r.f32()?,
            yaw: r.f32()?,
            pitch: r.f32()?,
            buttons: r.u8()?,
        };
        if id as usize >= MAX_PLAYERS || ids & (1 << id) != 0 || !frame.is_valid() {
            return Err(Error::InvalidInput);
        }
        ids |= 1 << id;
        *c = (id, frame);
    }
    let s = r.u8()? as usize;
    if s > MAX_PLAYERS {
        return Err(Error::InvalidInput);
    }
    let mut shots = [ShotTime { player: 0, tick: 0 }; MAX_PLAYERS];
    ids = 0;
    for shot in &mut shots[..s] {
        *shot = ShotTime {
            player: r.u8()?,
            tick: r.u32()?,
        };
        if shot.player as usize >= MAX_PLAYERS || ids & (1 << shot.player) != 0 {
            return Err(Error::InvalidInput);
        }
        ids |= 1 << shot.player;
    }
    if r.i != input.len() {
        return Err(Error::InvalidInput);
    }
    Ok(DecodedInputs {
        commands,
        command_count: n,
        shots,
        shot_count: s,
    })
}
pub(crate) fn decode_events(
    expected: EventIter<'_>,
    tick: u32,
) -> Result<[Option<GameEvent>; 64], Error> {
    if expected.len() > 64 {
        return Err(Error::InvalidInput);
    }
    // Validate authority payloads before mutating the simulation.
    let mut wanted = [None; 64];
    for (slot, e) in wanted.iter_mut().zip(expected.clone()) {
        if e.kind != 1 {
            return Err(Error::InvalidInput);
        }
        let e = NetworkEvent::decode(e.payload).map_err(|_| Error::InvalidInput)?;
        if e.tick != tick {
            return Err(Error::InvalidInput);
        }
        *slot = Some(e.event);
    }
    Ok(wanted)
}
pub(crate) fn compare_events(
    events: &[GameEvent],
    wanted: &[Option<GameEvent>; 64],
    count: usize,
) -> Result<(), Error> {
    if events.len() != count || events.iter().zip(wanted).any(|(a, b)| Some(*a) != *b) {
        return Err(Error::EventsDiffer);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use timeline::Playback;
    #[test]
    fn real_combat_roundtrip_seek_rewind_and_continue() {
        let mut g = Match::new(Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -8.)],
        });
        g.spawn_player(0);
        g.spawn_player(1);
        let mut capture = Capture::new(
            &g,
            19,
            Limits {
                checkpoint_interval: 17,
                ..Limits::default()
            },
        )
        .unwrap();
        let mut hits = 0;
        for i in 0..400 {
            let commands = [
                (
                    0,
                    InputFrame {
                        sequence: i,
                        buttons: if i % 200 < 100 {
                            BUTTON_FIRE
                        } else {
                            BUTTON_RELOAD
                        },
                        ..Default::default()
                    },
                ),
                (
                    1,
                    InputFrame {
                        sequence: i,
                        ..Default::default()
                    },
                ),
            ];
            let shots = [ShotTime {
                player: 0,
                tick: g.tick.saturating_sub(2),
            }];
            let events = g.step_with_rewind(&commands, &shots).events;
            hits += events
                .iter()
                .filter(|e| matches!(e, GameEvent::Hit { .. }))
                .count();
            capture.push(&g, &commands, &shots, &events).unwrap();
        }
        assert!(hits > 0);
        let tape = capture.finish();
        let bytes = tape.encode();
        let tape = Arc::new(Recording::decode(&bytes).unwrap());
        let mut world = World::new(Match::new(g.arena.clone()));
        let mut p = Playback::new(tape);
        p.start(&mut world).unwrap();
        while !p.finished() {
            p.step(&mut world).unwrap();
        }
        assert_eq!(g.state_hash(), world.game.state_hash());
        for target in [0, 199, 25, 399, 100, 400] {
            p.seek(&mut world, target).unwrap();
            while p.seeking() {
                assert!(p.pump_seek(&mut world, 3).unwrap() <= 3);
            }
            while !p.finished() {
                p.step(&mut world).unwrap();
            }
            assert_eq!(g.state_hash(), world.game.state_hash());
        }
    }
}
