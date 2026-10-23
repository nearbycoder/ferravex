//! Task 315 public clock recipe, reconnect boundaries and allocation evidence.
use ferravex_net::clock::{ClientClock, ClockAge, ClockSampleIgnored, TickUnwrapper};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountAlloc;
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
fn count_allocation() {
    if TRACK.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }
}
// SAFETY: Every request is forwarded unchanged to System. Const-initialized
// thread-local counters track this thread only and do not allocate themselves.
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller supplies GlobalAlloc's valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller supplies GlobalAlloc's valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller supplies the existing System pointer and valid size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies the original System pointer and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

#[test]
fn provided_time_exchange_diagnostics_and_render_recipe() {
    let mut clock = ClientClock::default();
    let unknown = clock.diagnostics(0, 64);
    assert_eq!(unknown.sample_age, ClockAge::Unknown);
    assert_eq!(unknown.rtt_seconds, None);
    assert_eq!(unknown.offset_seconds, None);
    assert_eq!(unknown.last_observed_us, None);
    assert_eq!(unknown.render_delay_ticks, 6);
    assert_eq!(unknown.jitter_seconds, 0.0);
    assert!(unknown.pending_probes.iter().all(Option::is_none));
    assert_eq!(clock.render_tick(0, 64), None);

    let (nonce, sent) = clock.probe(1_000_000).unwrap();
    let pending = clock.diagnostics(1_050_000, 64);
    assert_eq!(pending.local_us, 1_050_000);
    assert_eq!(pending.tick_rate_hz, 64);
    assert_eq!(pending.sample_age, ClockAge::Unknown);
    let probe = pending.pending_probes[nonce as usize % 4].unwrap();
    assert_eq!((probe.nonce, probe.client_us), (nonce, sent));
    assert_eq!(probe.age, ClockAge::AgeMicros(50_000));
    assert_eq!(
        clock.observe_detailed(nonce, sent, 11_050_000, 11_051_000, 640, 1_101_000),
        Ok(())
    );

    let sample = clock.diagnostics(1_201_000, 64);
    assert_eq!(sample.last_observed_us, Some(1_101_000));
    assert_eq!(sample.sample_age, ClockAge::AgeMicros(100_000));
    assert!((sample.rtt_seconds.unwrap() - 0.1).abs() < 1e-9);
    assert!((sample.offset_seconds.unwrap() - 10.0).abs() < 1e-9);
    assert!(sample.pending_probes.iter().all(Option::is_none));
    // Estimated server time is 11.201 s, 150 ms after tick 640's anchor.
    assert!((clock.estimated_tick(1_201_000, 64).unwrap() - 649.6).abs() < 1e-9);
    assert!((clock.render_tick(1_201_000, 64).unwrap() - 643.6).abs() < 1e-9);
}

#[test]
fn freshness_is_explicit_about_unknown_preceding_and_invalid_inspection_times() {
    let mut c = ClientClock::default();
    // Without an accepted sample the age is unknown even for an invalid query.
    assert_eq!(c.diagnostics(u64::MAX, 64).sample_age, ClockAge::Unknown);
    let (nonce, sent) = c.probe(1_000_000).unwrap();
    let slot = nonce as usize % 4;
    assert_eq!(
        c.diagnostics(999_999, 64).pending_probes[slot].unwrap().age,
        ClockAge::BeforeTimestamp
    );
    assert_eq!(
        c.diagnostics((1 << 53) + 1, 64).pending_probes[slot]
            .unwrap()
            .age,
        ClockAge::TimestampOutOfRange
    );
    assert!(c.observe(nonce, sent, 11_050_000, 11_051_000, 10, 1_101_000));
    assert_eq!(
        c.diagnostics(1_100_999, 64).sample_age,
        ClockAge::BeforeTimestamp
    );
    assert_eq!(
        c.diagnostics(1_101_000, 64).sample_age,
        ClockAge::AgeMicros(0)
    );
    assert_eq!(
        c.diagnostics(6_101_001, 64).sample_age,
        ClockAge::AgeMicros(5_000_001)
    );
    assert_eq!(
        c.diagnostics(1 << 53, 64).sample_age,
        ClockAge::AgeMicros((1 << 53) - 1_101_000)
    );
    let invalid = c.diagnostics(u64::MAX, 0);
    assert_eq!(invalid.sample_age, ClockAge::TimestampOutOfRange);
    // Estimates remain inspectable; invalid query time doesn't invalidate them.
    assert!(invalid.rtt_seconds.is_some());
    assert!(invalid.offset_seconds.is_some());
    assert_eq!(invalid.render_delay_ticks, c.render_delay_ticks(0));
    assert_eq!(c.render_tick(1_101_000, 0), None);
}

#[test]
fn reconnect_clears_pending_and_estimates_then_rebases_on_new_correlated_sample() {
    let mut c = ClientClock::default();
    let mut ticks = TickUnwrapper::default();
    let (first, first_sent) = c.probe(1_000_000).unwrap();
    assert!(c.observe(
        first,
        first_sent,
        11_050_000,
        11_051_000,
        u32::MAX,
        1_101_000
    ));
    ticks.observe(u32::MAX);
    let (second, sent) = c.probe(2_000_000).unwrap();
    assert!(c.observe(second, sent, 12_200_000, 12_201_000, 0, 2_401_000));
    assert!(c.jitter_seconds() > 0.0);
    assert_eq!(ticks.observe(0), Some(1 << 32));
    let (old, old_sent) = c.probe(3_000_000).unwrap();

    // The transport must also switch its session gate and clear its history.
    c.reset();
    ticks.reset();
    c.reset(); // repeated reset must remain safe
    let clean = c.diagnostics(0, 64);
    assert_eq!(clean.sample_age, ClockAge::Unknown);
    assert_eq!(clean.rtt_seconds, None);
    assert_eq!(clean.offset_seconds, None);
    assert_eq!(clean.jitter_seconds, 0.0);
    assert_eq!(clean.render_delay_ticks, 6);
    assert!(clean.pending_probes.iter().all(Option::is_none));
    assert_eq!(c.render_tick(0, 64), None);
    assert_eq!(ticks.project(0), None);
    assert_eq!(
        c.observe_detailed(old, old_sent, 13_050_000, 13_051_000, 1, 3_101_000),
        Err(ClockSampleIgnored::ProbeNotRetained)
    );
    assert_eq!(c.diagnostics(0, 64), clean);

    let (new, new_sent) = c.probe(0).unwrap();
    assert_eq!(new, old.wrapping_add(1));
    let before = c.diagnostics(101_000, 64);
    assert_eq!(
        c.observe_detailed(old, old_sent, 13_050_000, 13_051_000, 1, 3_101_000),
        Err(ClockSampleIgnored::ProbeNotRetained)
    );
    assert_eq!(c.diagnostics(101_000, 64), before);
    assert_eq!(
        c.observe_detailed(new, new_sent, 50_000, 51_000, 2, 101_000),
        Ok(())
    );
    assert_eq!(ticks.observe(2), Some(2));
    assert!(c.diagnostics(101_000, 64).offset_seconds.unwrap().abs() < 1e-9);
    assert!((c.rtt_seconds().unwrap() - 0.1).abs() < 1e-9);
    assert!((c.estimated_tick(101_000, 64).unwrap() - 5.2).abs() < 1e-9);
    assert_eq!(
        c.diagnostics(101_000, 64).sample_age,
        ClockAge::AgeMicros(0)
    );
}

#[test]
fn all_clock_operations_allocate_zero_including_overwrite_failures_and_reset() {
    // Include construction, not just warmed methods. Allocation counters and
    // test harness allocation are outside the measured region.
    TRACK.with(|v| v.set(true));
    ALLOCATIONS.with(|v| v.set(0));
    let mut c = ClientClock::default();
    let mut ticks = TickUnwrapper::default();
    let mut observations = 0;
    for i in 0..128_u64 {
        let local = 10_000_000 * i;
        for extra in 0..5 {
            c.probe(local + extra).unwrap();
        }
        let (nonce, sent) = c.probe(local + 10).unwrap();
        assert_eq!(
            c.observe_detailed(
                nonce,
                sent + 1,
                local + 50_000,
                local + 51_000,
                i as u32,
                local + 101_000
            ),
            Err(ClockSampleIgnored::ProbeTimestampMismatch)
        );
        assert_eq!(
            c.observe_detailed(
                nonce,
                sent,
                local + 50_000,
                local + 51_000,
                i as u32,
                local + 6_000_000
            ),
            Err(ClockSampleIgnored::ProbeExpired)
        );
        assert!(c.observe(
            nonce,
            sent,
            local + 50_000,
            local + 51_000,
            i as u32,
            local + 101_000
        ));
        observations += 1;
        assert_eq!(
            c.observe_detailed(
                nonce,
                sent,
                local + 50_000,
                local + 51_000,
                i as u32,
                local + 101_000
            ),
            Err(ClockSampleIgnored::ProbeNotRetained)
        );
        std::hint::black_box(c.diagnostics(local + 101_000, 64));
        std::hint::black_box(c.estimated_tick(local + 101_000, 64));
        std::hint::black_box(c.render_tick(local + 101_000, 64));
        std::hint::black_box(c.clone());
        assert_eq!(ticks.observe(i as u32), Some(i));
        assert_eq!(ticks.project(i as u32), Some(i));
        c.reset();
        ticks.reset();
    }
    TRACK.with(|v| v.set(false));
    assert_eq!(observations, 128);
    assert_eq!(
        ALLOCATIONS.with(Cell::get),
        0,
        "alloc/alloc_zeroed/realloc calls"
    );
}
