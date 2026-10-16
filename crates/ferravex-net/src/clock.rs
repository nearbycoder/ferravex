//! Correlated clock probes and a bounded interpolation delay. All times are
//! supplied explicitly, making jitter/loss/reorder behavior testable offline.
use crate::{history::ClockSync, sequence_newer};
const MAX_US: u64 = 1 << 53;
#[derive(Clone, Copy, Debug)]
struct Probe {
    nonce: u32,
    sent: u64,
}
#[derive(Clone, Copy, Debug)]
struct Anchor {
    tick: u32,
    extended: u64,
    server_us: u64,
}
#[derive(Clone, Debug, Default)]
pub struct ClientClock {
    sync: ClockSync,
    probes: [Option<Probe>; 4],
    next_nonce: u32,
    anchor: Option<Anchor>,
    jitter: f64,
    last_rtt: Option<f64>,
}
impl ClientClock {
    pub fn probe(&mut self, local_us: u64) -> Option<(u32, u64)> {
        if local_us > MAX_US {
            return None;
        }
        self.next_nonce = self.next_nonce.wrapping_add(1);
        let nonce = self.next_nonce;
        self.probes[nonce as usize % 4] = Some(Probe {
            nonce,
            sent: local_us,
        });
        Some((nonce, local_us))
    }
    pub fn observe(
        &mut self,
        nonce: u32,
        client_us: u64,
        receive_us: u64,
        send_us: u64,
        tick: u32,
        now_us: u64,
    ) -> bool {
        if [client_us, receive_us, send_us, now_us]
            .into_iter()
            .any(|v| v > MAX_US)
            || send_us < receive_us
            || now_us < client_us
        {
            return false;
        }
        let index = nonce as usize % 4;
        let Some(probe) = self.probes[index] else {
            return false;
        };
        if probe.nonce != nonce || probe.sent != client_us || now_us - client_us > 5_000_000 {
            return false;
        }
        if self.anchor.is_some_and(|a| {
            send_us <= a.server_us || (!sequence_newer(tick, a.tick) && tick != a.tick)
        }) {
            return false;
        }
        let mut sync = self.sync;
        if !sync.observe(
            client_us as f64 / 1e6,
            receive_us as f64 / 1e6,
            send_us as f64 / 1e6,
            now_us as f64 / 1e6,
        ) {
            return false;
        }
        let extended = if let Some(a) = self.anchor {
            let Some(n) = a.extended.checked_add(u64::from(tick.wrapping_sub(a.tick))) else {
                return false;
            };
            n
        } else {
            u64::from(tick)
        };
        let rtt = (now_us - client_us) as f64 / 1e6 - (send_us - receive_us) as f64 / 1e6;
        if let Some(old) = self.last_rtt {
            self.jitter += (rtt - old)
                .abs()
                .min(0.25)
                .mul_add(0.125, -self.jitter * 0.125);
        }
        self.last_rtt = Some(rtt);
        self.sync = sync;
        self.anchor = Some(Anchor {
            tick,
            extended,
            server_us: send_us,
        });
        self.probes[index] = None;
        true
    }
    pub fn rtt_seconds(&self) -> Option<f64> {
        self.anchor.map(|_| self.sync.rtt_seconds)
    }
    pub fn jitter_seconds(&self) -> f64 {
        self.jitter
    }
    pub fn render_delay_ticks(&self, hz: u16) -> u32 {
        (6. + (self.jitter * f64::from(hz) * 2.).ceil()).clamp(6., 12.) as u32
    }
    pub fn estimated_tick(&self, local_us: u64, hz: u16) -> Option<f64> {
        let a = self.anchor?;
        if local_us > MAX_US || !(1..=1000).contains(&hz) {
            return None;
        }
        let server_now = self.sync.server_time(local_us as f64 / 1e6);
        let elapsed = (server_now - a.server_us as f64 / 1e6).clamp(-0.25, 5.);
        Some(a.extended as f64 + elapsed * f64::from(hz))
    }
    pub fn render_tick(&self, local_us: u64, hz: u16) -> Option<f64> {
        Some(self.estimated_tick(local_us, hz)? - f64::from(self.render_delay_ticks(hz)))
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct TickUnwrapper {
    latest: Option<(u32, u64)>,
}
impl TickUnwrapper {
    pub fn observe(&mut self, tick: u32) -> Option<u64> {
        let extended = match self.latest {
            None => u64::from(tick),
            Some((old, n)) => {
                if !sequence_newer(tick, old) {
                    return None;
                }
                n.checked_add(u64::from(tick.wrapping_sub(old)))?
            }
        };
        self.latest = Some((tick, extended));
        Some(extended)
    }
    pub fn project(&self, tick: u32) -> Option<u64> {
        let (last, n) = self.latest?;
        if tick == last {
            Some(n)
        } else if sequence_newer(tick, last) {
            n.checked_add(u64::from(tick.wrapping_sub(last)))
        } else {
            n.checked_sub(u64::from(last.wrapping_sub(tick)))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probes_are_correlated_reorder_safe_and_wrap_ticks() {
        let mut c = ClientClock::default();
        let (id, sent) = c.probe(1_000_000).unwrap();
        assert!(!c.observe(id, sent + 1, 11_050_000, 11_051_000, u32::MAX, 1_101_000));
        assert!(c.observe(id, sent, 11_050_000, 11_051_000, u32::MAX, 1_101_000));
        assert!((c.rtt_seconds().unwrap() - 0.1).abs() < 1e-9);
        assert!(!c.observe(id, sent, 11_050_000, 11_051_000, u32::MAX, 1_101_000));
        let (a, at) = c.probe(1_200_000).unwrap();
        let (b, bt) = c.probe(1_300_000).unwrap();
        assert!(c.observe(b, bt, 11_350_000, 11_351_000, 18, 1_401_000));
        assert!(!c.observe(a, at, 11_250_000, 11_251_000, 12, 1_402_000));
        assert!(c.estimated_tick(1_401_000, 64).unwrap() > u32::MAX as f64);
        assert!((6..=12).contains(&c.render_delay_ticks(64)));
        let mut u = TickUnwrapper::default();
        assert_eq!(u.observe(u32::MAX), Some(u64::from(u32::MAX)));
        assert_eq!(u.observe(0), Some(1 << 32));
        assert_eq!(u.project(u32::MAX), Some((1 << 32) - 1));
        assert_eq!(u.observe(u32::MAX), None);
    }
}
