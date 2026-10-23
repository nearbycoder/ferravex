//! Correlated clock probes and a bounded interpolation delay. All times are
//! supplied explicitly, making jitter/loss/reorder behavior testable offline.
//!
//! A transport must validate the enclosing connection/session before delivering
//! a reply. Nonces correlate probes, not sessions. All local timestamps below
//! are microseconds in the same caller-owned monotonic time domain; server
//! timestamps use the server's domain. Neither domain is a wall clock.
//!
//! ```
//! use ferravex_net::clock::{ClientClock, ClockAge, ClockSampleIgnored, TickUnwrapper};
//!
//! let mut clock = ClientClock::default();
//! let (nonce, client_us) = clock.probe(1_000_000).unwrap();
//! let pending = clock.diagnostics(1_050_000, 64);
//! let probe = pending.pending_probes.into_iter().flatten().next().unwrap();
//! assert_eq!(probe.age, ClockAge::AgeMicros(50_000));
//! // After validating the reply's outer session, pass its four timestamps.
//! clock.observe_detailed(nonce, client_us, 11_050_000, 11_051_000,
//!                        640, 1_101_000).unwrap();
//! let diagnostics = clock.diagnostics(1_201_000, 64);
//! assert_eq!(diagnostics.sample_age, ClockAge::AgeMicros(100_000));
//! assert!((diagnostics.rtt_seconds.unwrap() - 0.1).abs() < 1e-9);
//! assert!((diagnostics.offset_seconds.unwrap() - 10.0).abs() < 1e-9);
//! assert_eq!(diagnostics.render_delay_ticks, 6);
//! let estimated = clock.estimated_tick(1_201_000, 64).unwrap();
//! let render = clock.render_tick(1_201_000, 64).unwrap();
//! assert!((render - (estimated - 6.0)).abs() < 1e-9);
//! assert_eq!(clock.observe_detailed(nonce, client_us, 11_050_000, 11_051_000,
//!                                  640, 1_101_000),
//!            Err(ClockSampleIgnored::ProbeNotRetained));
//! // At reconnect, discard old-session traffic/history and reset both clocks.
//! let mut ticks = TickUnwrapper::default();
//! ticks.observe(640);
//! ticks.reset();
//! clock.reset();
//! assert_eq!(clock.diagnostics(0, 64).sample_age, ClockAge::Unknown);
//! assert_eq!(ticks.observe(2), Some(2));
//! // A new local/server time origin and lower tick can now establish the anchor.
//! let (nonce, sent) = clock.probe(0).unwrap();
//! assert!(clock.observe(nonce, sent, 50_000, 51_000, 2, 101_000));
//! assert!(clock.render_tick(101_000, 64).is_some());
//! ```
use crate::{history::ClockSync, sequence_newer};
const MAX_US: u64 = 1 << 53;
const MAX_PROBE_AGE_US: u64 = 5_000_000;

/// Why a reply was ignored. The first failing check is reported, in timestamp,
/// correlation, expiry, anchor ordering, estimator, then tick-overflow order.
/// Every rejection preserves the entire clock, including retained probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClockSampleIgnored {
    /// At least one supplied timestamp exceeds the inclusive 2^53 µs bound.
    TimestampOutOfRange,
    /// Server send precedes server receive.
    ServerTimeReversed,
    /// Local receive precedes the echoed local send.
    LocalTimeReversed,
    /// No matching nonce is retained. Unknown, overwritten, already consumed,
    /// and reset probes cannot be distinguished without additional history.
    ProbeNotRetained,
    /// The nonce matches, but the echoed local send timestamp does not.
    ProbeTimestampMismatch,
    /// Local receive is more than 5,000,000 µs after this retained probe's send.
    ProbeExpired,
    /// Server send is not strictly newer than the accepted anchor's send.
    ServerTimeNotNewer,
    /// Tick is older than the anchor or exactly half the serial space away.
    /// Equal ticks are permitted if server send is newer.
    TickNotCurrentOrNewer,
    /// The existing floating-point estimator rejected RTT outside 0..=5 seconds.
    RttOutOfRange,
    /// Extending the anchor's tick would overflow u64.
    TickOverflow,
}

/// Age relative to a supplied local timestamp. This reports age, not a freshness
/// policy: the caller chooses how old an accepted estimate may be. No method
/// can detect timestamps from a different domain that happen to be ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClockAge {
    /// No accepted sample exists (including after reset).
    Unknown,
    /// The supplied inspection time exceeds 2^53 microseconds.
    TimestampOutOfRange,
    /// Inspection time precedes the retained timestamp; no age is inferred.
    BeforeTimestamp,
    /// Elapsed microseconds in the caller's local time domain.
    AgeMicros(u64),
}

fn age_at(timestamp: Option<u64>, now_us: u64) -> ClockAge {
    let Some(timestamp) = timestamp else {
        return ClockAge::Unknown;
    };
    if now_us > MAX_US {
        ClockAge::TimestampOutOfRange
    } else if let Some(age) = now_us.checked_sub(timestamp) {
        ClockAge::AgeMicros(age)
    } else {
        ClockAge::BeforeTimestamp
    }
}

/// A retained probe, including expired probes (inspection never consumes them).
/// Only `AgeMicros(age)` with `age > 5_000_000` establishes expiry; invalid or
/// preceding inspection times establish neither freshness nor expiry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PendingClockProbe {
    pub nonce: u32,
    pub client_us: u64,
    pub age: ClockAge,
}

/// Allocation-free, read-only diagnostics at an explicitly supplied time/rate.
/// RTT and offset share `sample_age`, measured since the last accepted local
/// receive. `None` means unknown, while `Some(0.0)` is a valid zero estimate.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ClockDiagnostics {
    pub local_us: u64,
    pub tick_rate_hz: u16,
    /// Fixed slot order (`nonce % 4`), not chronological order.
    pub pending_probes: [Option<PendingClockProbe>; 4],
    pub last_observed_us: Option<u64>,
    pub sample_age: ClockAge,
    pub rtt_seconds: Option<f64>,
    /// Estimated server-minus-local time, using the existing smoothing formula.
    pub offset_seconds: Option<f64>,
    pub jitter_seconds: f64,
    /// Same selection as `render_delay_ticks`, including the six-tick default
    /// before any accepted sample. This does not validate the supplied rate.
    pub render_delay_ticks: u32,
}
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
/// Four-slot correlated clock estimator. See the module recipe for transport,
/// time-domain, render sampling and reconnect responsibilities.
#[derive(Clone, Debug, Default)]
pub struct ClientClock {
    sync: ClockSync,
    probes: [Option<Probe>; 4],
    next_nonce: u32,
    anchor: Option<Anchor>,
    jitter: f64,
    last_rtt: Option<f64>,
    last_observed_us: Option<u64>,
}
impl ClientClock {
    /// Retain a probe with a wrapping u32 nonce. Timestamps through 2^53 µs
    /// inclusive are accepted; an invalid time changes nothing. The fifth probe
    /// overwrites the oldest slot regardless of age, preserving legacy policy.
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
    /// Compatibility wrapper: true exactly when [`Self::observe_detailed`]
    /// accepts the reply. False preserves probes and all estimator state.
    pub fn observe(
        &mut self,
        nonce: u32,
        client_us: u64,
        receive_us: u64,
        send_us: u64,
        tick: u32,
        now_us: u64,
    ) -> bool {
        self.observe_detailed(nonce, client_us, receive_us, send_us, tick, now_us)
            .is_ok()
    }

    /// Observe a session-validated reply with typed rejection reasons. Same
    /// acceptance, estimator, expiry and unwrapping rules as [`Self::observe`].
    /// Failure changes nothing, so a malformed echo cannot consume a valid
    /// pending probe; even expired probes remain until overwritten or reset.
    /// See the module's runnable exchange/render/reset recipe.
    pub fn observe_detailed(
        &mut self,
        nonce: u32,
        client_us: u64,
        receive_us: u64,
        send_us: u64,
        tick: u32,
        now_us: u64,
    ) -> Result<(), ClockSampleIgnored> {
        if [client_us, receive_us, send_us, now_us]
            .into_iter()
            .any(|v| v > MAX_US)
        {
            return Err(ClockSampleIgnored::TimestampOutOfRange);
        }
        if send_us < receive_us {
            return Err(ClockSampleIgnored::ServerTimeReversed);
        }
        if now_us < client_us {
            return Err(ClockSampleIgnored::LocalTimeReversed);
        }
        let index = nonce as usize % 4;
        let Some(probe) = self.probes[index] else {
            return Err(ClockSampleIgnored::ProbeNotRetained);
        };
        if probe.nonce != nonce {
            return Err(ClockSampleIgnored::ProbeNotRetained);
        }
        if probe.sent != client_us {
            return Err(ClockSampleIgnored::ProbeTimestampMismatch);
        }
        if now_us - client_us > MAX_PROBE_AGE_US {
            return Err(ClockSampleIgnored::ProbeExpired);
        }
        if let Some(a) = self.anchor {
            if send_us <= a.server_us {
                return Err(ClockSampleIgnored::ServerTimeNotNewer);
            }
            if !sequence_newer(tick, a.tick) && tick != a.tick {
                return Err(ClockSampleIgnored::TickNotCurrentOrNewer);
            }
        }
        let mut sync = self.sync;
        if !sync.observe(
            client_us as f64 / 1e6,
            receive_us as f64 / 1e6,
            send_us as f64 / 1e6,
            now_us as f64 / 1e6,
        ) {
            return Err(ClockSampleIgnored::RttOutOfRange);
        }
        let extended = if let Some(a) = self.anchor {
            let Some(n) = a.extended.checked_add(u64::from(tick.wrapping_sub(a.tick))) else {
                return Err(ClockSampleIgnored::TickOverflow);
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
        self.last_observed_us = Some(now_us);
        Ok(())
    }

    /// Inspect retained probes, smoothed RTT/offset age, jitter and delay without
    /// expiring probes or advancing the estimator. Supply the same local time
    /// domain used for `probe`/`observe`; age is unknown before the first sample.
    /// `hz` selects delay exactly as [`Self::render_delay_ticks`] does. Estimate
    /// methods still independently require 1..=1000 Hz and valid local time.
    pub fn diagnostics(&self, local_us: u64, hz: u16) -> ClockDiagnostics {
        ClockDiagnostics {
            local_us,
            tick_rate_hz: hz,
            pending_probes: self.probes.map(|probe| {
                probe.map(|p| PendingClockProbe {
                    nonce: p.nonce,
                    client_us: p.sent,
                    age: age_at(Some(p.sent), local_us),
                })
            }),
            last_observed_us: self.last_observed_us,
            sample_age: age_at(self.last_observed_us, local_us),
            rtt_seconds: self.rtt_seconds(),
            offset_seconds: self.anchor.map(|_| self.sync.offset_seconds),
            jitter_seconds: self.jitter,
            render_delay_ticks: self.render_delay_ticks(hz),
        }
    }

    /// Clear probes, estimates, jitter, freshness and tick anchor for reconnect.
    /// The first accepted new probe rebases time/ticks, even to lower values.
    /// Keeps the nonce cursor to avoid immediate reuse, but wrapping/recreated
    /// clocks can reuse nonces: validate the outer session before observation.
    /// This is not a session/epoch authenticator. Discard old-session traffic and
    /// reset the consumer's history and [`TickUnwrapper`] at the same boundary.
    pub fn reset(&mut self) {
        *self = Self {
            next_nonce: self.next_nonce,
            ..Self::default()
        };
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
    /// Forget the old tick epoch; the next observation starts from its raw u32.
    /// Call alongside [`ClientClock::reset`] and history reset at reconnect,
    /// after ensuring old-session ticks cannot be delivered. See module recipe.
    pub fn reset(&mut self) {
        self.latest = None;
    }
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

    fn ignored(
        c: &mut ClientClock,
        sample: (u32, u64, u64, u64, u32, u64),
        why: ClockSampleIgnored,
    ) {
        let before = format!("{c:?}");
        let mut legacy = c.clone();
        let (id, client, receive, send, tick, now) = sample;
        assert_eq!(
            c.observe_detailed(id, client, receive, send, tick, now),
            Err(why)
        );
        // Debug includes the private ClockSync sample counter, not just the
        // visible estimates. Failure must leave that smoothing weight intact.
        assert_eq!(format!("{c:?}"), before);
        assert!(!legacy.observe(id, client, receive, send, tick, now));
        assert_eq!(format!("{legacy:?}"), before);
    }

    #[test]
    fn all_malformed_timestamps_preserve_the_probe_and_full_estimator() {
        let mut c = ClientClock::default();
        let (id, client) = c.probe(1_000_000).unwrap();
        for sample in [
            (id, MAX_US + 1, 11_050_000, 11_051_000, 1, 1_101_000),
            (id, client, MAX_US + 1, 11_051_000, 1, 1_101_000),
            (id, client, 11_050_000, MAX_US + 1, 1, 1_101_000),
            (id, client, 11_050_000, 11_051_000, 1, MAX_US + 1),
        ] {
            ignored(&mut c, sample, ClockSampleIgnored::TimestampOutOfRange);
        }
        ignored(
            &mut c,
            (id, client, 11_051_000, 11_050_000, 1, 1_101_000),
            ClockSampleIgnored::ServerTimeReversed,
        );
        ignored(
            &mut c,
            (id, client, 11_050_000, 11_051_000, 1, client - 1),
            ClockSampleIgnored::LocalTimeReversed,
        );
        ignored(
            &mut c,
            (id, client + 1, 11_050_000, 11_051_000, 1, 1_101_000),
            ClockSampleIgnored::ProbeTimestampMismatch,
        );
        ignored(
            &mut c,
            (id, client, 11_000_000, 11_200_000, 1, 1_100_000),
            ClockSampleIgnored::RttOutOfRange,
        );
        assert!(c.observe(id, client, 11_050_000, 11_051_000, 1, 1_101_000));
        assert!((c.sync.offset_seconds - 10.0).abs() < 1e-9);
        assert!((c.sync.rtt_seconds - 0.1).abs() < 1e-9);

        // Repeat estimator rejection after a sample, with a second probe still
        // pending; a later valid sample must retain the normal 1/8 weighting.
        let (id, client) = c.probe(2_000_000).unwrap();
        ignored(
            &mut c,
            (id, client, 12_000_000, 12_200_000, 2, 2_100_000),
            ClockSampleIgnored::RttOutOfRange,
        );
        assert!(c.observe(id, client, 22_100_000, 22_101_000, 2, 2_201_000));
        assert!((c.sync.offset_seconds - 11.25).abs() < 1e-9);
        assert!((c.sync.rtt_seconds - 0.1125).abs() < 1e-9);
    }

    #[test]
    fn retention_is_bounded_and_unknown_overwritten_consumed_share_one_reason() {
        let mut c = ClientClock::default();
        ignored(
            &mut c,
            (1, 0, 10, 10, 0, 10),
            ClockSampleIgnored::ProbeNotRetained,
        );
        let first = c.probe(0).unwrap();
        for t in 1..4 {
            c.probe(t).unwrap();
        }
        let before = format!("{c:?}");
        assert_eq!(c.probe(MAX_US + 1), None);
        assert_eq!(format!("{c:?}"), before);
        assert_eq!(
            c.diagnostics(4, 64).pending_probes.iter().flatten().count(),
            4
        );
        let newest = c.probe(4).unwrap();
        ignored(
            &mut c,
            (first.0, first.1, 10, 10, 0, 10),
            ClockSampleIgnored::ProbeNotRetained,
        );
        assert!(c.observe(newest.0, newest.1, 10, 10, 0, 10));
        ignored(
            &mut c,
            (newest.0, newest.1, 10, 10, 0, 10),
            ClockSampleIgnored::ProbeNotRetained,
        );
        assert_eq!(
            c.diagnostics(10, 64)
                .pending_probes
                .iter()
                .flatten()
                .count(),
            3
        );
    }

    #[test]
    fn expiration_is_inclusive_and_ignored_expired_samples_do_not_consume() {
        let mut c = ClientClock::default();
        let (id, client) = c.probe(1_000_000).unwrap();
        ignored(
            &mut c,
            (id, client, 11_000_000, 11_000_000, 1, 6_000_001),
            ClockSampleIgnored::ProbeExpired,
        );
        let p = c.diagnostics(6_000_001, 64).pending_probes[id as usize % 4].unwrap();
        assert_eq!(p.age, ClockAge::AgeMicros(5_000_001));
        // There is deliberately no time watermark or implicit expiry mutation.
        assert!(c.observe(id, client, 11_000_000, 11_000_000, 1, 6_000_000));
        assert_eq!(c.rtt_seconds(), Some(5.0));
    }

    #[test]
    fn newer_server_anchor_does_not_impose_a_new_local_receive_watermark() {
        let mut c = ClientClock::default();
        let (early, early_sent) = c.probe(1_000_000).unwrap();
        let (later, later_sent) = c.probe(1_050_000).unwrap();
        assert!(c.observe(later, later_sent, 11_050_000, 11_051_000, 10, 1_200_000));
        // The existing contract orders anchors by server send and serial tick,
        // not probe issue order or prior local receive time. Keep that domain.
        assert!(c.observe(early, early_sent, 11_060_000, 11_061_000, 11, 1_190_000));
        assert_eq!(
            c.diagnostics(1_200_000, 64).last_observed_us,
            Some(1_190_000)
        );
        assert_eq!(
            c.diagnostics(1_200_000, 64).sample_age,
            ClockAge::AgeMicros(10_000)
        );
    }

    #[test]
    fn reorder_ambiguous_ticks_and_overflow_preserve_every_field() {
        let mut c = ClientClock::default();
        let (id, client) = c.probe(1_000_000).unwrap();
        assert!(c.observe(id, client, 11_050_000, 11_051_000, u32::MAX, 1_101_000));
        let (id, client) = c.probe(2_000_000).unwrap();
        ignored(
            &mut c,
            (id, client, 11_050_000, 11_051_000, 0, 2_101_000),
            ClockSampleIgnored::ServerTimeNotNewer,
        );
        for tick in [u32::MAX - 1, u32::MAX.wrapping_add(1 << 31)] {
            ignored(
                &mut c,
                (id, client, 12_050_000, 12_051_000, tick, 2_101_000),
                ClockSampleIgnored::TickNotCurrentOrNewer,
            );
        }
        // Equal ticks stay accepted with strictly newer server time.
        assert!(c.observe(id, client, 12_050_000, 12_051_000, u32::MAX, 2_101_000));
        let (id, client) = c.probe(3_000_000).unwrap();
        assert!(c.observe(id, client, 13_050_000, 13_051_000, 0, 3_101_000));
        assert_eq!(c.anchor.unwrap().extended, 1 << 32);

        // Reach the impractically large extended boundary without 2^64 ticks.
        c.anchor.as_mut().unwrap().extended = u64::MAX;
        let (id, client) = c.probe(4_000_000).unwrap();
        ignored(
            &mut c,
            (id, client, 14_050_000, 14_051_000, 1, 4_101_000),
            ClockSampleIgnored::TickOverflow,
        );
    }

    #[test]
    fn timestamp_and_nonce_extremes_keep_legacy_domains() {
        let mut c = ClientClock::default();
        let (id, sent) = c.probe(MAX_US).unwrap();
        assert!(c.observe(id, sent, MAX_US, MAX_US, u32::MAX, MAX_US));
        assert_eq!(c.rtt_seconds(), Some(0.0));
        assert_eq!(c.diagnostics(MAX_US, 64).offset_seconds, Some(0.0));
        assert_eq!(c.estimated_tick(MAX_US, 1), Some(f64::from(u32::MAX)));
        assert_eq!(c.estimated_tick(MAX_US + 1, 64), None);
        assert_eq!(c.estimated_tick(MAX_US, 0), None);
        assert_eq!(c.estimated_tick(MAX_US, 1001), None);
        assert!(c.estimated_tick(MAX_US, 1000).is_some());
        c.reset();
        c.next_nonce = u32::MAX - 1;
        let (last, sent) = c.probe(0).unwrap();
        assert_eq!(last, u32::MAX);
        let (zero, _) = c.probe(1).unwrap();
        assert_eq!(zero, 0);
        assert!(c.observe(last, sent, 0, 0, 0, 0));
        assert!(c.observe(zero, 1, 1, 1, 1, 1));
    }

    #[test]
    fn estimates_match_existing_sync_jitter_delay_and_elapsed_clamps() {
        let mut c = ClientClock::default();
        let mut sync = ClockSync::default();
        let mut previous_rtt: Option<f64> = None;
        let mut jitter = 0.0;
        for (i, duration) in [100_000, 200_000, 4_000_000, 50_000]
            .into_iter()
            .enumerate()
        {
            let local = 10_000_000 * (i as u64 + 1);
            let server_receive = local + 10_000_000 + duration / 2;
            let server_send = server_receive + 1_000;
            let now = local + duration + 1_000;
            let (id, sent) = c.probe(local).unwrap();
            assert!(sync.observe(
                sent as f64 / 1e6,
                server_receive as f64 / 1e6,
                server_send as f64 / 1e6,
                now as f64 / 1e6
            ));
            assert_eq!(
                c.observe_detailed(id, sent, server_receive, server_send, i as u32, now),
                Ok(())
            );
            let rtt = duration as f64 / 1e6;
            if let Some(previous) = previous_rtt {
                jitter += ((rtt - previous).abs().min(0.25) - jitter) * 0.125;
            }
            previous_rtt = Some(rtt);
            assert_eq!(c.rtt_seconds(), Some(sync.rtt_seconds));
            assert_eq!(
                c.diagnostics(now, 64).offset_seconds,
                Some(sync.offset_seconds)
            );
            assert!((c.jitter_seconds() - jitter).abs() < 1e-14);
            for hz in [0, 1, 64, 1000, u16::MAX] {
                let delay = (6.0 + (jitter * f64::from(hz) * 2.0).ceil()).clamp(6.0, 12.0) as u32;
                assert_eq!(c.render_delay_ticks(hz), delay);
                assert_eq!(c.diagnostics(now, hz).render_delay_ticks, delay);
            }
            assert_eq!(c.estimated_tick(0, 64), Some(i as f64 - 16.0));
            assert_eq!(
                c.estimated_tick(now + 10_000_000, 64),
                Some(i as f64 + 320.0)
            );
        }
    }

    #[test]
    fn unwrapper_reset_wrap_half_space_and_overflow_remain_explicit() {
        let mut u = TickUnwrapper::default();
        assert_eq!(u.project(0), None);
        assert_eq!(u.observe(u32::MAX), Some(u64::from(u32::MAX)));
        assert_eq!(u.observe(0), Some(1 << 32));
        assert_eq!(u.project(u32::MAX), Some((1 << 32) - 1));
        let before = u.latest;
        assert_eq!(u.observe(0), None);
        assert_eq!(u.observe(1 << 31), None);
        assert_eq!(u.latest, before);
        u.latest = Some((0, u64::MAX));
        assert_eq!(u.observe(1), None);
        assert_eq!(u.project(1), None);
        assert_eq!(u.latest, Some((0, u64::MAX)));
        u.reset();
        assert_eq!(u.project(u32::MAX), None);
        assert_eq!(u.observe(0), Some(0));
        assert_eq!(u.project(u32::MAX), None);
    }

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
