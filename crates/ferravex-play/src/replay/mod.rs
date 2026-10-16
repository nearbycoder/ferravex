//! Bounded deterministic replay timelines. Games own checkpoint/input semantics;
//! the timeline owns fixed ticks, accepted events, hashes, seeking and byte limits.
use std::sync::Arc;
pub mod ghost;
mod wire;
pub const MAX_TICKS: usize = 1_000_000;
pub const MAX_EVENTS: usize = 1_000_000;
pub const MAX_CHECKPOINTS: usize = 8192;
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_INPUT_BYTES: usize = 4096;
pub const MAX_EVENT_BYTES: usize = 4096;
pub const MAX_STATE_BYTES: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    InvalidLimits,
    InvalidHeader,
    TickOrder,
    Capacity,
    MissingCheckpoint,
    InvalidData,
    UnsupportedVersion,
    Checksum,
}
impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ReplayError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub schema: u64,
    pub version: u32,
    pub context: u64,
    pub tick_hz: u16,
    pub seed: u64,
}
impl Header {
    fn valid(self) -> bool {
        self.schema != 0 && self.version != 0 && (1..=1000).contains(&self.tick_hz)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub ticks: usize,
    pub events: usize,
    pub checkpoints: usize,
    pub bytes: usize,
    pub checkpoint_interval: u64,
    pub input_bytes: usize,
    pub event_bytes: usize,
    pub state_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            ticks: 19200,
            events: 32768,
            checkpoints: 152,
            bytes: 4 * 1024 * 1024,
            checkpoint_interval: 128,
            input_bytes: 1024,
            event_bytes: 128,
            state_bytes: MAX_STATE_BYTES,
        }
    }
}
impl Limits {
    fn valid(self) -> bool {
        self.ticks <= MAX_TICKS
            && self.events <= MAX_EVENTS
            && (1..=MAX_CHECKPOINTS).contains(&self.checkpoints)
            && self.bytes <= MAX_FILE_BYTES
            && self.checkpoint_interval > 0
            && self.input_bytes <= MAX_INPUT_BYTES
            && self.event_bytes <= MAX_EVENT_BYTES
            && (1..=MAX_STATE_BYTES).contains(&self.state_bytes)
            && self
                .ticks
                .checked_mul(32)
                .and_then(|n| self.events.checked_mul(12).and_then(|e| n.checked_add(e)))
                .and_then(|n| {
                    self.checkpoints
                        .checked_mul(24)
                        .and_then(|c| n.checked_add(c))
                })
                .and_then(|n| n.checked_add(self.bytes))
                .and_then(|n| n.checked_add(64))
                .is_some_and(|n| n <= MAX_FILE_BYTES)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Span {
    start: u32,
    len: u32,
}
impl Span {
    fn bytes(self, arena: &[u8]) -> &[u8] {
        &arena[self.start as usize..(self.start + self.len) as usize]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Tick {
    tick: u64,
    hash: u64,
    input: Span,
    event_start: u32,
    event_count: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Event {
    kind: u16,
    data: Span,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Checkpoint {
    tick: u64,
    hash: u64,
    state: Span,
}
#[derive(Clone, Debug)]
pub struct Recording {
    header: Header,
    ticks: Vec<Tick>,
    events: Vec<Event>,
    checkpoints: Vec<Checkpoint>,
    arena: Vec<u8>,
}
impl Recording {
    pub fn header(&self) -> Header {
        self.header
    }
    pub fn start_tick(&self) -> u64 {
        self.checkpoints[0].tick
    }
    pub fn end_tick(&self) -> u64 {
        self.ticks.last().map_or(self.start_tick(), |f| f.tick)
    }
    pub fn tick_count(&self) -> usize {
        self.ticks.len()
    }
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }
    pub fn payload_bytes(&self) -> usize {
        self.arena.len()
    }
    pub fn events_at(&self, tick: u64) -> EventIter<'_> {
        let found = self
            .ticks
            .binary_search_by_key(&tick, |f| f.tick)
            .ok()
            .map(|i| self.ticks[i]);
        let events = found.map_or(&[][..], |f| {
            &self.events[f.event_start as usize..f.event_start as usize + f.event_count as usize]
        });
        EventIter {
            events: events.iter(),
            arena: &self.arena,
        }
    }
    pub fn input_at(&self, tick: u64) -> Option<&[u8]> {
        self.ticks
            .binary_search_by_key(&tick, |f| f.tick)
            .ok()
            .map(|i| self.ticks[i].input.bytes(&self.arena))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventInput<'a> {
    pub kind: u16,
    pub payload: &'a [u8],
}
#[derive(Clone)]
pub struct EventIter<'a> {
    events: std::slice::Iter<'a, Event>,
    arena: &'a [u8],
}
impl<'a> Iterator for EventIter<'a> {
    type Item = EventInput<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.events.next().map(|e| EventInput {
            kind: e.kind,
            payload: e.data.bytes(self.arena),
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.events.size_hint()
    }
}
impl ExactSizeIterator for EventIter<'_> {}
/// All metadata and byte buffers reserve their budgets at start. A rejected tick
/// changes no buffers; reaching a limit leaves the complete prefix exportable.
pub struct Recorder {
    data: Recording,
    limits: Limits,
}
impl Recorder {
    pub fn new(
        header: Header,
        limits: Limits,
        tick: u64,
        state: &[u8],
        hash: u64,
    ) -> Result<Self, ReplayError> {
        if !header.valid() {
            return Err(ReplayError::InvalidHeader);
        }
        if !limits.valid() {
            return Err(ReplayError::InvalidLimits);
        }
        if state.is_empty() || state.len() > limits.state_bytes || state.len() > limits.bytes {
            return Err(ReplayError::Capacity);
        }
        let mut data = Recording {
            header,
            ticks: Vec::with_capacity(limits.ticks),
            events: Vec::with_capacity(limits.events),
            checkpoints: Vec::with_capacity(limits.checkpoints),
            arena: Vec::with_capacity(limits.bytes),
        };
        data.arena.extend_from_slice(state);
        data.checkpoints.push(Checkpoint {
            tick,
            hash,
            state: Span {
                start: 0,
                len: state.len() as u32,
            },
        });
        Ok(Self { data, limits })
    }
    pub fn needs_checkpoint(&self, tick: u64) -> bool {
        tick.saturating_sub(self.data.checkpoints.last().unwrap().tick)
            >= self.limits.checkpoint_interval
    }
    pub fn tick_count(&self) -> usize {
        self.data.ticks.len()
    }
    pub fn payload_bytes(&self) -> usize {
        self.data.arena.len()
    }
    pub fn push(
        &mut self,
        tick: u64,
        input: &[u8],
        events: &[EventInput<'_>],
        checkpoint: Option<&[u8]>,
        hash: u64,
    ) -> Result<(), ReplayError> {
        if self.data.end_tick().checked_add(1) != Some(tick) {
            return Err(ReplayError::TickOrder);
        }
        if self.needs_checkpoint(tick) && checkpoint.is_none() {
            return Err(ReplayError::MissingCheckpoint);
        }
        if self.data.ticks.len() == self.limits.ticks
            || input.len() > self.limits.input_bytes
            || events.len() > 64
            || events.len() > self.limits.events - self.data.events.len()
            || events
                .iter()
                .any(|e| e.payload.len() > self.limits.event_bytes)
            || checkpoint.is_some_and(|s| {
                s.is_empty()
                    || s.len() > self.limits.state_bytes
                    || self.data.checkpoints.len() == self.limits.checkpoints
            })
        {
            return Err(ReplayError::Capacity);
        }
        let needed = input.len()
            + events.iter().map(|e| e.payload.len()).sum::<usize>()
            + checkpoint.map_or(0, |s| s.len());
        if needed > self.limits.bytes - self.data.arena.len() {
            return Err(ReplayError::Capacity);
        }
        let mut append = |bytes: &[u8]| {
            let span = Span {
                start: self.data.arena.len() as u32,
                len: bytes.len() as u32,
            };
            self.data.arena.extend_from_slice(bytes);
            span
        };
        let input = append(input);
        let event_start = self.data.events.len() as u32;
        for event in events {
            self.data.events.push(Event {
                kind: event.kind,
                data: append(event.payload),
            });
        }
        if let Some(state) = checkpoint {
            self.data.checkpoints.push(Checkpoint {
                tick,
                hash,
                state: append(state),
            });
        }
        self.data.ticks.push(Tick {
            tick,
            hash,
            input,
            event_start,
            event_count: events.len() as u16,
        });
        Ok(())
    }
    pub fn finish(mut self) -> Recording {
        self.data.ticks.shrink_to_fit();
        self.data.events.shrink_to_fit();
        self.data.checkpoints.shrink_to_fit();
        self.data.arena.shrink_to_fit();
        self.data
    }
}
/// Restore must stage/validate before replacing the game. Step applies recorded
/// inputs exactly once; expected events are for comparison/presentation, not a
/// second application of already simulated authoritative damage.
pub trait ReplayWorld {
    type Error;
    const SCHEMA: u64;
    const VERSION: u32;
    const TICK_HZ: u16;
    fn context(&self) -> Result<u64, Self::Error>;
    fn restore(&mut self, state: &[u8]) -> Result<(), Self::Error>;
    fn step(&mut self, input: &[u8], expected: EventIter<'_>) -> Result<(), Self::Error>;
    fn state_hash(&self) -> Result<u64, Self::Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaybackError<E> {
    State(E),
    Incompatible,
    InvalidTarget,
    NotStarted,
    Ended,
    Seeking,
    Faulted,
    Desync {
        tick: u64,
        expected: u64,
        actual: u64,
    },
}
/// Immutable shared tape plus a cursor. Seeking restores the nearest earlier
/// checkpoint once, then advances at most the caller's work budget per pump.
pub struct Playback {
    recording: Arc<Recording>,
    position: u64,
    row: usize,
    started: bool,
    poisoned: bool,
    seek: Option<u64>,
    fault: Option<(u64, u64, u64)>,
}
impl Playback {
    pub fn new(recording: Arc<Recording>) -> Self {
        let position = recording.start_tick();
        Self {
            recording,
            position,
            row: 0,
            started: false,
            poisoned: false,
            seek: None,
            fault: None,
        }
    }
    pub fn recording(&self) -> &Recording {
        &self.recording
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn seeking(&self) -> bool {
        self.seek.is_some()
    }
    pub fn finished(&self) -> bool {
        self.started && self.position == self.recording.end_tick()
    }
    pub fn fault(&self) -> Option<(u64, u64, u64)> {
        self.fault
    }
    pub fn start<W: ReplayWorld>(&mut self, world: &mut W) -> Result<(), PlaybackError<W::Error>> {
        let h = self.recording.header;
        if h.schema != W::SCHEMA
            || h.version != W::VERSION
            || h.tick_hz != W::TICK_HZ
            || h.context != world.context().map_err(PlaybackError::State)?
        {
            return Err(PlaybackError::Incompatible);
        }
        self.restore_at(world, 0)?;
        self.started = true;
        self.seek = None;
        Ok(())
    }
    fn restore_at<W: ReplayWorld>(
        &mut self,
        world: &mut W,
        index: usize,
    ) -> Result<(), PlaybackError<W::Error>> {
        let cp = self.recording.checkpoints[index];
        world
            .restore(cp.state.bytes(&self.recording.arena))
            .map_err(PlaybackError::State)?;
        self.position = cp.tick;
        self.row = self.recording.ticks.partition_point(|f| f.tick <= cp.tick);
        self.fault = None;
        self.poisoned = false;
        self.verify(world, cp.tick, cp.hash)
    }
    fn verify<W: ReplayWorld>(
        &mut self,
        world: &W,
        tick: u64,
        expected: u64,
    ) -> Result<(), PlaybackError<W::Error>> {
        let actual = match world.state_hash() {
            Ok(hash) => hash,
            Err(e) => {
                self.poisoned = true;
                return Err(PlaybackError::State(e));
            }
        };
        if actual != expected {
            self.fault = Some((tick, expected, actual));
            return Err(PlaybackError::Desync {
                tick,
                expected,
                actual,
            });
        }
        Ok(())
    }
    pub fn step<W: ReplayWorld>(&mut self, world: &mut W) -> Result<(), PlaybackError<W::Error>> {
        if !self.started {
            return Err(PlaybackError::NotStarted);
        }
        if self.seek.is_some() {
            return Err(PlaybackError::Seeking);
        }
        self.step_inner(world)
    }
    fn step_inner<W: ReplayWorld>(&mut self, world: &mut W) -> Result<(), PlaybackError<W::Error>> {
        if let Some((tick, expected, actual)) = self.fault {
            return Err(PlaybackError::Desync {
                tick,
                expected,
                actual,
            });
        }
        let Some(frame) = self.recording.ticks.get(self.row).copied() else {
            return Err(PlaybackError::Ended);
        };
        if self.poisoned {
            return Err(PlaybackError::Faulted);
        }
        if let Err(e) = world.step(
            frame.input.bytes(&self.recording.arena),
            self.recording.events_at(frame.tick),
        ) {
            self.poisoned = true;
            return Err(PlaybackError::State(e));
        }
        self.position = frame.tick;
        self.row += 1;
        self.verify(world, frame.tick, frame.hash)
    }
    pub fn seek<W: ReplayWorld>(
        &mut self,
        world: &mut W,
        target: u64,
    ) -> Result<(), PlaybackError<W::Error>> {
        if !self.started {
            return Err(PlaybackError::NotStarted);
        }
        if target < self.recording.start_tick() || target > self.recording.end_tick() {
            return Err(PlaybackError::InvalidTarget);
        }
        let index = self
            .recording
            .checkpoints
            .partition_point(|c| c.tick <= target)
            - 1;
        self.restore_at(world, index)?;
        self.seek = (self.position < target).then_some(target);
        Ok(())
    }
    pub fn pump_seek<W: ReplayWorld>(
        &mut self,
        world: &mut W,
        budget: usize,
    ) -> Result<usize, PlaybackError<W::Error>> {
        let Some(target) = self.seek else {
            return Ok(0);
        };
        let mut steps = 0;
        while self.position < target && steps < budget.min(MAX_TICKS) {
            self.step_inner(world)?;
            steps += 1;
        }
        if self.position == target {
            self.seek = None;
        }
        Ok(steps)
    }
}
/// Stateless deterministic seed derivation for a tick/channel, independent of
/// render cadence and unrelated systems consuming random values.
pub fn derive_seed(seed: u64, tick: u64, channel: u64) -> u64 {
    let mut x = seed ^ tick.wrapping_mul(0x9e3779b97f4a7c15) ^ channel.rotate_left(29);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
#[cfg(test)]
mod tests;
