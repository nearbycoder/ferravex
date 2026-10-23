use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    static DEALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Allocator;

fn record(allocation: bool) {
    let _ = RECORD.try_with(|enabled| {
        if enabled.get() {
            let counter = if allocation {
                &ALLOCATIONS
            } else {
                &DEALLOCATIONS
            };
            let _ = counter.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: All operations forward the identical allocation contract to System.
// Thread-local counters do not allocate and tolerate TLS destruction.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        record(true);
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record(true);
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        record(true);
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(false);
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[test]
fn reservations_admission_stealing_and_fades_use_only_preallocated_storage() {
    use ferravex_play::audio::{Bus, Clip, Mixer, Sound, VoiceParams};

    // The external sound owner stays alive while voices are replaced/retired.
    let sound = Sound::new(Clip::tone(8000, 440., 0.1).unwrap()).unwrap();
    let mut mixer = Mixer::new(8000, 2).unwrap();
    assert!(mixer.set_voice_policy(Some(Default::default())));
    let policy = mixer.voice_policy().unwrap();
    let mut out = [[0.; 2]; 16];
    ALLOCATIONS.with(|count| count.set(0));
    DEALLOCATIONS.with(|count| count.set(0));
    RECORD.with(|enabled| enabled.set(true));
    for i in 0..1000 {
        let cosmetic = VoiceParams {
            priority: (i % 127) as u8,
            looping: true,
            ramp_frames: 8,
            bus: Bus::Music,
            ..Default::default()
        };
        let old = mixer.start_with_outcome(&sound, cosmetic).handle().unwrap();
        assert!(
            mixer
                .start_with_outcome(&sound, cosmetic)
                .rejection()
                .is_some()
        );
        let replacement = mixer.start_with_outcome(
            &sound,
            VoiceParams {
                priority: 127,
                ..cosmetic
            },
        );
        assert_eq!(replacement.replaced(), Some(old));
        let critical = VoiceParams {
            priority: 128,
            ..cosmetic
        };
        let first = mixer.start_with_outcome(&sound, critical).handle().unwrap();
        let second = mixer.start_with_outcome(&sound, critical);
        assert_eq!(second.replaced(), replacement.handle());
        assert!(
            mixer
                .start_with_outcome(&sound, critical)
                .rejection()
                .is_some()
        );
        assert!(mixer.set_params(first, cosmetic));
        assert!(!mixer.set_params(second.handle().unwrap(), cosmetic));

        assert!(mixer.set_voice_policy(None));
        assert!(mixer.set_voice_policy(Some(policy)));
        assert!(mixer.fade_master_gain(0., 0.001));
        assert!(mixer.fade_bus_gain(Bus::Music, 0.25, 0.001));
        mixer.render(&mut out);
        assert_eq!(out[15], [0.; 2]);
        assert!(mixer.fade_master_gain(1., 0.001));
        assert!(mixer.stop(first, 8));
        mixer.render(&mut out);
        assert!(mixer.params(first).is_none());
        assert!(mixer.stop(second.handle().unwrap(), 8));
        mixer.skip(8);
        assert_eq!(mixer.active_voices(), 0);
        assert!(mixer.start(&sound, cosmetic).is_some());
        mixer.suspend();
        mixer.stop_all();
    }
    RECORD.with(|enabled| enabled.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
    assert_eq!(DEALLOCATIONS.with(Cell::get), 0);
    assert_eq!(mixer.voice_limit(), 2);
    assert_eq!(mixer.stats().stolen, 2000);
    assert_eq!(mixer.stats().dropped, 2000);
    assert!(out.iter().flatten().all(|value| value.is_finite()));
}
