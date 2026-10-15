use super::{AnimationError, LoopMode, SpriteClip};

pub const MAX_CLIP_SECONDS: f32 = 3600.;
pub const MAX_ANIMATION_EVENTS: usize = 256;
/// Large host stalls must be split explicitly; each update does bounded event work.
pub const MAX_ANIMATION_DT: f32 = 60.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationEvent {
    /// Seconds in [0, duration], including the end of a one-shot clip.
    pub time: f32,
    pub id: u32,
}
#[derive(Clone, Debug)]
pub struct EventTrack {
    duration: f32,
    events: Vec<AnimationEvent>,
}
impl EventTrack {
    pub fn new(duration: f32, events: Vec<AnimationEvent>) -> Result<Self, AnimationError> {
        valid_duration(duration)?;
        if events.len() > MAX_ANIMATION_EVENTS {
            return Err(AnimationError::Capacity);
        }
        if events
            .iter()
            .any(|e| !e.time.is_finite() || e.time < 0. || e.time > duration)
            || events.windows(2).any(|p| p[0].time > p[1].time)
        {
            return Err(AnimationError::InvalidKey);
        }
        Ok(Self { duration, events })
    }
    pub fn duration(&self) -> f32 {
        self.duration
    }
    pub fn events(&self) -> &[AnimationEvent] {
        &self.events
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EventHit {
    pub id: u32,
    pub time: f32,
    /// Stable index into an AnimationBank, or zero for a standalone timeline.
    pub clip: u16,
    pub cycle: u64,
}
/// Fixed caller-owned event queue. Overflow drops the latest events, reports the
/// exact number dropped, and never prevents the clock/pose from advancing.
#[derive(Clone, Debug)]
pub struct EventBuffer<const N: usize = 64> {
    hits: [EventHit; N],
    len: usize,
    dropped: u64,
}
impl<const N: usize> Default for EventBuffer<N> {
    fn default() -> Self {
        Self {
            hits: [EventHit::default(); N],
            len: 0,
            dropped: 0,
        }
    }
}
impl<const N: usize> EventBuffer<N> {
    pub fn hits(&self) -> &[EventHit] {
        &self.hits[..self.len]
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn clear(&mut self) {
        self.len = 0;
        self.dropped = 0;
    }
    fn push(&mut self, event: AnimationEvent, clip: u16, cycle: u64) {
        self.hits[self.len] = EventHit {
            id: event.id,
            time: event.time,
            clip,
            cycle,
        };
        self.len += 1;
    }
}
fn valid_duration(duration: f32) -> Result<(), AnimationError> {
    if !duration.is_finite() || !(0.0001..=MAX_CLIP_SECONDS).contains(&duration) {
        Err(AnimationError::InvalidClip)
    } else {
        Ok(())
    }
}
/// Forward playback at 0..=8x, with f64 phase accumulation and saturating loop
/// counts. Pausing, zero speed and zero dt preserve pending start events. Seeking
/// is silent. Resuming never re-emits an already crossed event.
#[derive(Clone, Debug)]
pub struct Timeline {
    duration: f32,
    mode: LoopMode,
    phase: f64,
    cycles: u64,
    speed: f32,
    paused: bool,
    start: bool,
}
impl Timeline {
    pub fn new(duration: f32, mode: LoopMode) -> Result<Self, AnimationError> {
        valid_duration(duration)?;
        Ok(Self {
            duration,
            mode,
            phase: 0.,
            cycles: 0,
            speed: 1.,
            paused: false,
            start: true,
        })
    }
    pub fn time(&self) -> f32 {
        self.phase as f32
    }
    pub fn normalized_time(&self) -> f32 {
        (self.phase / f64::from(self.duration)) as f32
    }
    pub fn cycles(&self) -> u64 {
        self.cycles
    }
    pub fn speed(&self) -> f32 {
        self.speed
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn finished(&self) -> bool {
        self.mode == LoopMode::Clamp && self.phase >= f64::from(self.duration)
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    pub fn set_speed(&mut self, speed: f32) -> Result<(), AnimationError> {
        if !speed.is_finite() || !(0.0..=8.).contains(&speed) {
            return Err(AnimationError::InvalidTime);
        }
        self.speed = speed;
        Ok(())
    }
    pub fn seek(&mut self, time: f32) -> Result<(), AnimationError> {
        if !time.is_finite() || time < 0. {
            return Err(AnimationError::InvalidTime);
        }
        self.phase = if self.mode == LoopMode::Repeat {
            f64::from(time) % f64::from(self.duration)
        } else {
            f64::from(time.min(self.duration))
        };
        self.cycles = 0;
        self.start = false;
        Ok(())
    }
    pub fn restart(&mut self) {
        self.phase = 0.;
        self.cycles = 0;
        self.start = true;
    }
    fn end(&self, dt: f32) -> Result<f64, AnimationError> {
        if !dt.is_finite() || !(0.0..=MAX_ANIMATION_DT).contains(&dt) {
            return Err(AnimationError::InvalidTime);
        }
        let delta = if self.paused {
            0.
        } else {
            f64::from(dt) * f64::from(self.speed)
        };
        let end = self.phase + delta;
        Ok(if self.mode == LoopMode::Clamp {
            end.min(f64::from(self.duration))
        } else {
            end
        })
    }
    fn commit(&mut self, end: f64) {
        if end > self.phase {
            self.start = false;
        }
        if self.mode == LoopMode::Repeat {
            self.cycles = self
                .cycles
                .saturating_add((end / f64::from(self.duration)).floor() as u64);
            self.phase = end % f64::from(self.duration);
        } else {
            self.phase = end;
        }
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), AnimationError> {
        let end = self.end(dt)?;
        self.commit(end);
        Ok(())
    }
    /// Clears output on success, emits chronological events in (old,new], plus
    /// initial time-zero events. Repeat seams emit the old cycle's end before
    /// the new cycle's start. Work is O(event count + output capacity), even if
    /// millions of loops were crossed. Invalid input leaves clock/queue intact.
    pub fn advance_events<const N: usize>(
        &mut self,
        dt: f32,
        events: &EventTrack,
        out: &mut EventBuffer<N>,
    ) -> Result<(), AnimationError> {
        self.advance_tagged(dt, events, 0, out)
    }
    pub(super) fn advance_tagged<const N: usize>(
        &mut self,
        dt: f32,
        events: &EventTrack,
        clip: u16,
        out: &mut EventBuffer<N>,
    ) -> Result<(), AnimationError> {
        if events.duration != self.duration {
            return Err(AnimationError::InvalidClip);
        }
        let end = self.end(dt)?;
        out.clear();
        if end <= self.phase {
            return Ok(());
        }
        let d = f64::from(self.duration);
        let mut total = 0_u64;
        for event in &events.events {
            let e = f64::from(event.time);
            let count = |t: f64| -> u64 {
                if t < e {
                    0
                } else if self.mode == LoopMode::Repeat {
                    ((t - e) / d).floor() as u64 + 1
                } else {
                    1
                }
            };
            total += count(end) - count(self.phase);
            if self.start && e == self.phase {
                total += 1;
            }
        }
        let mut index = events.events.partition_point(|e| {
            f64::from(e.time) < self.phase || (!self.start && f64::from(e.time) == self.phase)
        });
        let mut cycle = 0_u64;
        for _ in 0..total.min(N as u64) {
            if index == events.events.len() {
                index = 0;
                cycle += 1;
            }
            out.push(
                events.events[index],
                clip,
                self.cycles.saturating_add(cycle),
            );
            index += 1;
        }
        out.dropped = total.saturating_sub(out.len as u64);
        self.commit(end);
        Ok(())
    }
}

/// Uses the same clock, pause/speed/seek semantics and bounded events as skeletal
/// animation. Frame IDs are authored drawing/atlas IDs; no texture API is required.
#[derive(Clone, Debug)]
pub struct SpritePlayer {
    clip: SpriteClip,
    events: EventTrack,
    clock: Timeline,
}
impl SpritePlayer {
    pub fn new(
        clip: SpriteClip,
        mode: LoopMode,
        events: Vec<AnimationEvent>,
    ) -> Result<Self, AnimationError> {
        let clock = Timeline::new(clip.duration(), mode)?;
        let events = EventTrack::new(clip.duration(), events)?;
        Ok(Self {
            clip,
            events,
            clock,
        })
    }
    pub fn frame(&self) -> u32 {
        self.clip
            .frame(self.clock.time(), LoopMode::Clamp)
            .expect("validated sprite clock")
    }
    pub fn timeline(&self) -> &Timeline {
        &self.clock
    }
    pub fn timeline_mut(&mut self) -> &mut Timeline {
        &mut self.clock
    }
    pub fn advance<const N: usize>(
        &mut self,
        dt: f32,
        out: &mut EventBuffer<N>,
    ) -> Result<(), AnimationError> {
        self.clock.advance_events(dt, &self.events, out)
    }
}
