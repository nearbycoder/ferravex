//! Bounded authority history and clock estimates; rendering and transport are independent.
use std::collections::VecDeque;
#[derive(Clone, Debug)]
pub struct History<T> {
    samples: VecDeque<(u64, T)>,
    capacity: usize,
}
impl<T> History<T> {
    pub fn new(capacity: usize) -> Self {
        assert!((1..=4096).contains(&capacity));
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }
    pub fn push(&mut self, tick: u64, state: T) -> bool {
        if self.samples.back().is_some_and(|(last, _)| tick <= *last) {
            return false;
        }
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back((tick, state));
        true
    }
    /// No extrapolation outside retained history: old/untrusted shot times are rejected.
    pub fn bracket(&self, tick: f64) -> Option<(&T, &T, f32)> {
        if !tick.is_finite()
            || tick < self.samples.front()?.0 as f64
            || tick > self.samples.back()?.0 as f64
        {
            return None;
        }
        for i in 0..self.samples.len() {
            let (a, left) = &self.samples[i];
            if tick == *a as f64 {
                return Some((left, left, 0.0));
            }
            if let Some((b, right)) = self.samples.get(i + 1)
                && tick < *b as f64
            {
                return Some((left, right, ((tick - *a as f64) / (*b - *a) as f64) as f32));
            }
        }
        None
    }
    pub fn iter(&self) -> impl Iterator<Item = (u64, &T)> {
        self.samples.iter().map(|(t, v)| (*t, v))
    }
    pub fn first_tick(&self) -> Option<u64> {
        self.samples.front().map(|s| s.0)
    }
    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}
#[derive(Default, Clone, Copy, Debug)]
pub struct ClockSync {
    pub offset_seconds: f64,
    pub rtt_seconds: f64,
    samples: u32,
}
impl ClockSync {
    /// NTP-style exchange: local send/receive and server receive/send timestamps.
    pub fn observe(
        &mut self,
        local_send: f64,
        server_receive: f64,
        server_send: f64,
        local_receive: f64,
    ) -> bool {
        if ![local_send, server_receive, server_send, local_receive]
            .iter()
            .all(|v| v.is_finite())
            || local_receive < local_send
            || server_send < server_receive
        {
            return false;
        }
        let rtt = (local_receive - local_send) - (server_send - server_receive);
        if !(0.0..=5.0).contains(&rtt) {
            return false;
        }
        let offset = ((server_receive - local_send) + (server_send - local_receive)) * 0.5;
        let weight = if self.samples == 0 { 1.0 } else { 0.125 };
        self.offset_seconds += (offset - self.offset_seconds) * weight;
        self.rtt_seconds += (rtt - self.rtt_seconds) * weight;
        self.samples = self.samples.saturating_add(1);
        true
    }
    pub fn server_time(&self, local: f64) -> f64 {
        local + self.offset_seconds
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_or_future_shots_are_rejected_and_history_interpolates() {
        let mut h = History::new(3);
        for i in 0..5 {
            assert!(h.push(i, i as f32));
        }
        assert_eq!(h.len(), 3);
        assert!(h.bracket(1.0).is_none());
        assert!(h.bracket(5.0).is_none());
        assert!(!h.push(3, 0.0));
        assert_eq!(h.bracket(2.5), Some((&2.0, &3.0, 0.5)));
        assert_eq!(h.bracket(4.0), Some((&4.0, &4.0, 0.0)));
    }
    #[test]
    fn clock_offset_accounts_for_server_processing() {
        let mut c = ClockSync::default();
        assert!(c.observe(1.0, 11.1, 11.2, 1.3));
        assert!((c.offset_seconds - 10.0).abs() < 1e-9);
        assert!((c.rtt_seconds - 0.2).abs() < 1e-9);
        assert!(!c.observe(f64::NAN, 0., 0., 0.));
    }
}
