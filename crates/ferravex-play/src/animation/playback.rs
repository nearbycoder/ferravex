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
/// Per-update catch-up diagnostics from [`EventBuffer::report`]. Counts describe
/// event crossings, including repeated IDs and keys with identical timestamps.
/// Dropped crossings are consumed: they are never replayed on a later update.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EventCatchUpReport {
    /// Earliest crossings retained in the output buffer.
    pub emitted: usize,
    /// Later crossings omitted because the buffer was full.
    pub dropped: u64,
    /// Repeat seams crossed, even when the event track is empty. Clamp is zero.
    pub crossed_cycles: u64,
    /// Event-bearing cycles with no retained crossing in this update. A cycle
    /// with some retained and some dropped hits is not counted; `dropped` still
    /// includes its missing hits. Empty cycles are never counted.
    pub skipped_event_cycles: u64,
    /// Earliest omitted crossing, including its clip and saturating cycle ID.
    pub first_dropped: Option<EventHit>,
    /// The timeline's cycle counter has reached `u64::MAX`. Cycle labels can no
    /// longer distinguish subsequent loops; the per-update counts remain exact.
    pub cycle_saturated: bool,
}
/// Fixed caller-owned event queue. Overflow drops the latest events, reports the
/// exact number dropped, and never prevents the clock/pose from advancing.
#[derive(Clone, Debug)]
pub struct EventBuffer<const N: usize = 64> {
    hits: [EventHit; N],
    len: usize,
    report: EventCatchUpReport,
}
impl<const N: usize> Default for EventBuffer<N> {
    fn default() -> Self {
        Self {
            hits: [EventHit::default(); N],
            len: 0,
            report: EventCatchUpReport::default(),
        }
    }
}
impl<const N: usize> EventBuffer<N> {
    pub fn hits(&self) -> &[EventHit] {
        &self.hits[..self.len]
    }
    pub fn dropped(&self) -> u64 {
        self.report.dropped
    }
    /// Returns diagnostics for the latest successful event update. Paused,
    /// zero-speed, zero-dt and finished updates replace the previous report with
    /// zero counts; invalid updates preserve both hits and report. [`Self::clear`]
    /// resets it. Sprite, skeletal and graph players populate the same report.
    /// Iterating [`Self::hits`] visits at most `N` retained crossings.
    ///
    /// ```
    /// use ferravex_play::animation::{
    ///     AnimationEvent, EventBuffer, EventCatchUpReport, EventTrack, LoopMode, Timeline,
    /// };
    /// let track = EventTrack::new(1.0, vec![AnimationEvent { time: 0.5, id: 7 }])?;
    /// let mut clock = Timeline::new(1.0, LoopMode::Repeat)?;
    /// let mut output = EventBuffer::<2>::default();
    /// clock.advance_events(4.0, &track, &mut output)?;
    /// let report: EventCatchUpReport = output.report();
    /// assert_eq!((report.emitted, report.dropped), (2, 2));
    /// assert_eq!(report.crossed_cycles, 4);
    /// assert_eq!(report.skipped_event_cycles, 2);
    /// assert_eq!(report.first_dropped.unwrap().cycle, 2);
    /// assert!(!report.cycle_saturated);
    /// assert_eq!(output.dropped(), report.dropped);
    /// assert_eq!(output.hits().iter().map(|hit| hit.id).sum::<u32>(), 14);
    /// // Overflow does not defer events or prevent clock progression.
    /// clock.advance_events(0.25, &track, &mut output)?;
    /// assert_eq!(output.report(), EventCatchUpReport::default());
    /// # Ok::<(), ferravex_play::animation::AnimationError>(())
    /// ```
    pub fn report(&self) -> EventCatchUpReport {
        self.report
    }
    pub fn clear(&mut self) {
        self.len = 0;
        self.report = EventCatchUpReport::default();
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
        let (cycles, phase) = self.position(end);
        self.cycles = self.cycles.saturating_add(cycles);
        self.phase = phase;
    }
    fn position(&self, end: f64) -> (u64, f64) {
        if self.mode == LoopMode::Repeat {
            let duration = f64::from(self.duration);
            ((end / duration).floor() as u64, end % duration)
        } else {
            (0, end)
        }
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), AnimationError> {
        let end = self.end(dt)?;
        self.commit(end);
        Ok(())
    }
    /// Clears output on success, emits chronological events in (old,new], plus
    /// initial time-zero events. Repeat seams emit the old cycle's end before
    /// the new cycle's start; equal-time keys retain authored order. Work is
    /// O(log(event count) + output capacity), even if millions of loops were
    /// crossed, without allocating. [`EventBuffer::report`] describes overflow
    /// and skipped event-bearing cycles. Invalid input leaves clock/queue intact.
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
        let (crossed_cycles, phase) = self.position(end);
        out.report.crossed_cycles = crossed_cycles;
        out.report.cycle_saturated = self.cycles.saturating_add(crossed_cycles) == u64::MAX;
        if end <= self.phase {
            return Ok(());
        }
        let mut index = events.events.partition_point(|e| {
            f64::from(e.time) < self.phase || (!self.start && f64::from(e.time) == self.phase)
        });
        let end_index = events
            .events
            .partition_point(|e| f64::from(e.time) <= phase);
        let event_count = events.events.len();
        // Count the initial tail, whole middle cycles and final prefix. Compare
        // local phases directly: (end - event.time) / duration can round tiny
        // positive keys onto the preceding seam and deliver them twice.
        let total = if crossed_cycles == 0 {
            (end_index - index) as u64
        } else {
            (event_count - index) as u64
                + (crossed_cycles - 1) * event_count as u64
                + end_index as u64
        };
        let first_event_cycle = u64::from(index == event_count);
        let mut cycle = 0_u64;
        for _ in 0..total.min(N as u64) {
            if index == event_count {
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
        out.report.emitted = out.len;
        out.report.dropped = total - out.len as u64;
        if total > 0 {
            let last_event_cycle = if end_index > 0 {
                crossed_cycles
            } else {
                crossed_cycles.saturating_sub(1)
            };
            let emitted_cycles = if out.len == 0 {
                0
            } else {
                cycle - first_event_cycle + 1
            };
            out.report.skipped_event_cycles =
                last_event_cycle - first_event_cycle + 1 - emitted_cycles;
        }
        if out.report.dropped > 0 {
            if index == event_count {
                index = 0;
                cycle += 1;
            }
            let event = events.events[index];
            out.report.first_dropped = Some(EventHit {
                id: event.id,
                time: event.time,
                clip,
                cycle: self.cycles.saturating_add(cycle),
            });
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(time: f32, id: u32) -> AnimationEvent {
        AnimationEvent { time, id }
    }

    #[test]
    fn tiny_positive_key_is_not_rounded_onto_the_previous_seam() {
        let tiny = f32::MIN_POSITIVE;
        let events = EventTrack::new(1., vec![event(0., 0), event(tiny, 1), event(1., 2)]).unwrap();
        let mut clock = Timeline::new(1., LoopMode::Repeat).unwrap();
        let mut out = EventBuffer::<8>::default();
        clock.advance_events(1., &events, &mut out).unwrap();
        assert_eq!(
            out.hits()
                .iter()
                .map(|hit| (hit.id, hit.cycle))
                .collect::<Vec<_>>(),
            [(0, 0), (1, 0), (2, 0), (0, 1)]
        );
        clock.advance_events(tiny, &events, &mut out).unwrap();
        assert_eq!(
            out.hits(),
            &[EventHit {
                id: 1,
                time: tiny,
                clip: 0,
                cycle: 1
            }]
        );
        clock.advance_events(tiny, &events, &mut out).unwrap();
        assert!(out.hits().is_empty());
    }

    #[test]
    fn large_and_partitioned_updates_deliver_identical_crossings() {
        let duration = 0.1;
        let track = EventTrack::new(
            duration,
            vec![
                event(0., 1),
                event(f32::MIN_POSITIVE, 2),
                event(duration / 2., 3),
                event(duration, 4),
            ],
        )
        .unwrap();
        let mut full = Timeline::new(duration, LoopMode::Repeat).unwrap();
        let mut parts = full.clone();
        let mut out = EventBuffer::<64>::default();
        full.advance_events(duration * 8., &track, &mut out)
            .unwrap();
        let expected = out.hits().to_vec();
        let report = out.report();
        let mut actual = Vec::new();
        let mut crossed_cycles = 0;
        for _ in 0..64 {
            parts
                .advance_events(duration / 8., &track, &mut out)
                .unwrap();
            actual.extend_from_slice(out.hits());
            crossed_cycles += out.report().crossed_cycles;
            assert_eq!(out.dropped(), 0);
        }
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), report.emitted);
        assert_eq!(crossed_cycles, report.crossed_cycles);
        assert_eq!((parts.phase, parts.cycles()), (full.phase, full.cycles()));
    }

    fn compare_with_enumerated_crossings<const N: usize>(
        mut clock: Timeline,
        track: &EventTrack,
        dt: f32,
    ) {
        // Small dyadic times keep this deliberately unbounded test oracle exact.
        // Production instead counts whole cycles without visiting their events.
        let end = clock.end(dt).unwrap();
        let crossed_cycles = if clock.mode == LoopMode::Repeat {
            end.floor() as u64
        } else {
            0
        };
        let mut expected = Vec::new();
        if end > clock.phase {
            for cycle in 0..=crossed_cycles {
                for event in track.events() {
                    let time = cycle as f64 + f64::from(event.time);
                    if (time > clock.phase || (clock.start && time == clock.phase)) && time <= end {
                        expected.push(EventHit {
                            id: event.id,
                            time: event.time,
                            clip: 19,
                            cycle,
                        });
                    }
                }
            }
        }
        let emitted = expected.len().min(N);
        let event_cycles = expected
            .iter()
            .map(|hit| hit.cycle)
            .collect::<std::collections::BTreeSet<_>>();
        let retained_cycles = expected[..emitted]
            .iter()
            .map(|hit| hit.cycle)
            .collect::<std::collections::BTreeSet<_>>();
        let mut out = EventBuffer::<N>::default();
        clock.advance_tagged(dt, track, 19, &mut out).unwrap();
        assert_eq!(out.hits(), &expected[..emitted]);
        assert_eq!(
            out.report(),
            EventCatchUpReport {
                emitted,
                dropped: (expected.len() - emitted) as u64,
                crossed_cycles,
                skipped_event_cycles: (event_cycles.len() - retained_cycles.len()) as u64,
                first_dropped: expected.get(emitted).copied(),
                cycle_saturated: false,
            }
        );
        assert_eq!(out.dropped(), (expected.len() - emitted) as u64);
        assert_eq!(clock.cycles(), crossed_cycles);
        assert_eq!(
            clock.phase,
            if clock.mode == LoopMode::Repeat {
                end % 1.
            } else {
                end
            }
        );
    }

    #[test]
    fn bounded_output_and_reports_match_enumerated_crossings() {
        for keys in [
            vec![],
            vec![event(0., 1)],
            vec![event(1., 1)],
            vec![event(0.5, 1)],
            vec![
                event(0., 1),
                event(0., 1),
                event(0.25, 2),
                event(0.5, 3),
                event(1., 4),
                event(1., 5),
            ],
        ] {
            let track = EventTrack::new(1., keys).unwrap();
            for mode in [LoopMode::Repeat, LoopMode::Clamp] {
                for seek in [None, Some(0.), Some(0.125), Some(0.5), Some(0.75), Some(1.)] {
                    for speed in [0., 0.5, 2., 8.] {
                        for paused in [false, true] {
                            let mut clock = Timeline::new(1., mode).unwrap();
                            if let Some(time) = seek {
                                clock.seek(time).unwrap();
                            }
                            clock.set_speed(speed).unwrap();
                            clock.set_paused(paused);
                            for dt in [0., 0.125, 0.5, 1., 3.75] {
                                compare_with_enumerated_crossings::<0>(clock.clone(), &track, dt);
                                compare_with_enumerated_crossings::<1>(clock.clone(), &track, dt);
                                compare_with_enumerated_crossings::<4>(clock.clone(), &track, dt);
                                compare_with_enumerated_crossings::<256>(clock.clone(), &track, dt);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn overflow_reports_partial_and_fully_skipped_cycles_without_replay() {
        let track = EventTrack::new(1., vec![event(0., 1), event(0.5, 2), event(1., 3)]).unwrap();
        let mut clock = Timeline::new(1., LoopMode::Repeat).unwrap();
        let mut out = EventBuffer::<4>::default();
        clock.advance_events(3., &track, &mut out).unwrap();
        assert_eq!(
            out.report(),
            EventCatchUpReport {
                emitted: 4,
                dropped: 6,
                crossed_cycles: 3,
                skipped_event_cycles: 2,
                first_dropped: Some(EventHit {
                    id: 2,
                    time: 0.5,
                    clip: 0,
                    cycle: 1
                }),
                cycle_saturated: false,
            }
        );
        assert_eq!((clock.time(), clock.cycles()), (0., 3));
        clock.advance_events(0.25, &track, &mut out).unwrap();
        assert!(out.hits().is_empty());
        assert_eq!(out.report(), EventCatchUpReport::default());
        clock.advance_events(0.25, &track, &mut out).unwrap();
        assert_eq!(
            out.hits(),
            &[EventHit {
                id: 2,
                time: 0.5,
                clip: 0,
                cycle: 3
            }]
        );
        out.clear();
        assert!(out.hits().is_empty());
        assert_eq!(out.report(), EventCatchUpReport::default());
    }

    #[test]
    fn seek_restart_pause_and_speed_preserve_crossing_boundaries() {
        let track = EventTrack::new(1., vec![event(0., 1), event(0.5, 2), event(1., 3)]).unwrap();
        let mut clock = Timeline::new(1., LoopMode::Repeat).unwrap();
        let mut out = EventBuffer::<8>::default();
        clock.set_paused(true);
        clock.advance_events(60., &track, &mut out).unwrap();
        clock.set_paused(false);
        clock.set_speed(0.).unwrap();
        clock.advance_events(60., &track, &mut out).unwrap();
        clock.set_speed(2.).unwrap();
        clock.advance_events(0., &track, &mut out).unwrap();
        clock.advance_events(0.25, &track, &mut out).unwrap();
        assert_eq!(
            out.hits().iter().map(|hit| hit.id).collect::<Vec<_>>(),
            [1, 2]
        );
        clock.set_paused(true);
        clock.advance_events(60., &track, &mut out).unwrap();
        assert_eq!(out.report(), EventCatchUpReport::default());
        clock.set_paused(false);
        clock.set_speed(0.5).unwrap();
        clock.advance_events(1., &track, &mut out).unwrap();
        assert_eq!(
            out.hits()
                .iter()
                .map(|hit| (hit.id, hit.cycle))
                .collect::<Vec<_>>(),
            [(3, 0), (1, 1)]
        );
        // Seeking onto a key is silent, including wrapping an exact end to zero.
        for time in [0.5, 1., 10.] {
            clock.seek(time).unwrap();
            clock.advance_events(0.25, &track, &mut out).unwrap();
            assert!(out.hits().is_empty());
            assert_eq!(clock.cycles(), 0);
        }
        clock.restart();
        clock.advance_events(0.25, &track, &mut out).unwrap();
        assert_eq!(out.hits().iter().map(|hit| hit.id).collect::<Vec<_>>(), [1]);
        // Silent clock-only progression consumes start/crossings as before.
        clock.restart();
        clock.advance(1.).unwrap();
        clock.advance_events(0.25, &track, &mut out).unwrap();
        assert!(out.hits().is_empty());
    }

    #[test]
    fn maximum_catch_up_has_exact_counts_and_empty_tracks_still_report_cycles() {
        let duration = 0.0001;
        let mut clock = Timeline::new(duration, LoopMode::Repeat).unwrap();
        clock.set_speed(8.).unwrap();
        let track = EventTrack::new(duration, vec![event(0., 1); MAX_ANIMATION_EVENTS]).unwrap();
        let mut out = EventBuffer::<3>::default();
        clock
            .advance_events(MAX_ANIMATION_DT, &track, &mut out)
            .unwrap();
        let cycles = (480. / f64::from(duration)).floor() as u64;
        assert_eq!(out.report().crossed_cycles, cycles);
        assert_eq!(
            out.dropped(),
            (cycles + 1) * MAX_ANIMATION_EVENTS as u64 - 3
        );
        assert_eq!(out.report().skipped_event_cycles, cycles);
        assert_eq!(out.report().first_dropped, out.hits().first().copied());
        assert_eq!(clock.phase, 480. % f64::from(duration));
        let empty = EventTrack::new(duration, vec![]).unwrap();
        clock.restart();
        let mut no_storage = EventBuffer::<0>::default();
        clock
            .advance_events(MAX_ANIMATION_DT, &empty, &mut no_storage)
            .unwrap();
        assert_eq!(
            no_storage.report(),
            EventCatchUpReport {
                crossed_cycles: cycles,
                ..Default::default()
            }
        );
        clock.restart();
        clock
            .advance_events(MAX_ANIMATION_DT, &track, &mut no_storage)
            .unwrap();
        assert_eq!(
            no_storage.dropped(),
            (cycles + 1) * MAX_ANIMATION_EVENTS as u64
        );
        assert_eq!(no_storage.report().skipped_event_cycles, cycles + 1);
    }

    #[test]
    fn saturated_cycle_labels_do_not_corrupt_relative_counts() {
        let track = EventTrack::new(1., vec![event(0.5, 1)]).unwrap();
        let mut clock = Timeline::new(1., LoopMode::Repeat).unwrap();
        clock.cycles = u64::MAX - 1;
        let mut out = EventBuffer::<2>::default();
        clock.advance_events(4., &track, &mut out).unwrap();
        assert_eq!(clock.cycles(), u64::MAX);
        assert_eq!(
            out.hits().iter().map(|hit| hit.cycle).collect::<Vec<_>>(),
            [u64::MAX - 1, u64::MAX]
        );
        assert_eq!(
            out.report(),
            EventCatchUpReport {
                emitted: 2,
                dropped: 2,
                crossed_cycles: 4,
                skipped_event_cycles: 2,
                first_dropped: Some(EventHit {
                    id: 1,
                    time: 0.5,
                    clip: 0,
                    cycle: u64::MAX
                }),
                cycle_saturated: true,
            }
        );
        clock.advance_events(0., &track, &mut out).unwrap();
        assert_eq!(
            out.report(),
            EventCatchUpReport {
                cycle_saturated: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn invalid_input_preserves_clock_hits_and_report() {
        let track = EventTrack::new(1., vec![event(0.5, 1)]).unwrap();
        let wrong_duration = EventTrack::new(2., vec![]).unwrap();
        let mut clock = Timeline::new(1., LoopMode::Repeat).unwrap();
        let mut out = EventBuffer::<1>::default();
        clock.advance_events(2., &track, &mut out).unwrap();
        let hits = out.hits().to_vec();
        let report = out.report();
        let state = format!("{clock:?}");
        for dt in [f32::NAN, f32::INFINITY, -1., MAX_ANIMATION_DT + 1.] {
            assert_eq!(
                clock.advance_events(dt, &track, &mut out),
                Err(AnimationError::InvalidTime)
            );
            assert_eq!(out.hits(), hits);
            assert_eq!(out.report(), report);
            assert_eq!(format!("{clock:?}"), state);
        }
        assert_eq!(
            clock.advance_events(1., &wrong_duration, &mut out),
            Err(AnimationError::InvalidClip)
        );
        for value in [f32::NAN, f32::INFINITY, -1.] {
            assert_eq!(clock.seek(value), Err(AnimationError::InvalidTime));
            assert_eq!(clock.set_speed(value), Err(AnimationError::InvalidTime));
        }
        assert_eq!(clock.set_speed(8.01), Err(AnimationError::InvalidTime));
        assert_eq!(format!("{clock:?}"), state);
        assert_eq!(out.hits(), hits);
        assert_eq!(out.report(), report);
    }

    #[test]
    fn sprite_player_populates_the_shared_report_and_finishes_once() {
        let clip = SpriteClip::new(vec![super::super::SpriteFrame {
            index: 4,
            duration: 1.,
        }])
        .unwrap();
        let mut sprite =
            SpritePlayer::new(clip, LoopMode::Clamp, vec![event(0., 1), event(1., 2)]).unwrap();
        let mut out = EventBuffer::<1>::default();
        sprite.advance(60., &mut out).unwrap();
        assert!(sprite.timeline().finished());
        assert_eq!(sprite.frame(), 4);
        assert_eq!(
            out.report(),
            EventCatchUpReport {
                emitted: 1,
                dropped: 1,
                first_dropped: Some(EventHit {
                    id: 2,
                    time: 1.,
                    clip: 0,
                    cycle: 0
                }),
                ..Default::default()
            }
        );
        sprite.advance(60., &mut out).unwrap();
        assert_eq!(out.report(), EventCatchUpReport::default());
        assert!(out.hits().is_empty());
    }
}
