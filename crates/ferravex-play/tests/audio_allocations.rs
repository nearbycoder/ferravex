use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {static RECORD:Cell<bool>=const {Cell::new(false)};static CALLS:Cell<usize>=const {Cell::new(0)};}
struct Allocator;
fn allocated(p: *mut u8) {
    if !p.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
}
// SAFETY: Every operation forwards the identical allocation contract to System;
// thread-local counters do not allocate and ignore destruction-time TLS failures.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        allocated(p);
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        allocated(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) };
        allocated(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[test]
fn validated_playback_mixing_parameters_and_retirement_allocate_nothing() {
    use ferravex_play::audio::*;
    let bank = SoundBank::procedural(24000).unwrap();
    let mut mixer = Mixer::new(48000, 32).unwrap();
    let mut out = [[0.; 2]; 128];
    let hum = mixer
        .start(
            bank.get(Cue::Engine),
            VoiceParams {
                looping: true,
                ..Default::default()
            },
        )
        .unwrap();
    mixer.render(&mut out);
    CALLS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    for i in 0..1000 {
        let p = VoiceParams {
            position: [i as f32 * 0.001, 0., 0.],
            pitch: 1. + (i % 5) as f32 * 0.1,
            looping: true,
            ..Default::default()
        };
        assert!(mixer.set_params(hum, p));
        if i % 4 == 0 {
            mixer.start(
                bank.get(Cue::Footstep(Surface::Stone)),
                VoiceParams::default(),
            );
        }
        if i % 7 == 0
            && let Some(handle) = mixer.start(
                bank.get(Cue::Fire),
                VoiceParams {
                    priority: 5,
                    ..Default::default()
                },
            )
        {
            mixer.stop(handle, 32);
        }
        if i % 97 == 0 {
            mixer.suspend();
        }
        mixer.set_bus_gain(Bus::Effects, 0.7);
        mixer.render(&mut out);
        mixer.skip(1);
    }
    mixer.stop_all();
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    assert!(out.iter().flatten().all(|v| v.is_finite() && v.abs() <= 1.));
}
