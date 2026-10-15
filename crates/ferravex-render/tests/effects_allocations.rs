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
fn sorted_instanced_billboards_and_ribbon_recording_allocate_nothing_after_warmup() {
    use ferravex_play::particles::*;
    use ferravex_render::{
        Vec3,
        effects::{self, BillboardView, EffectBlend, EffectStyle, ParticleRenderer, ParticleStyle},
        input::host_frame,
    };
    let mut particles = ParticlePool::new(64);
    for i in 0..64 {
        particles
            .spawn(ParticleDesc {
                position: [i as f32 * 0.01, 0., 0.],
                lifetime: 30.,
                ..Default::default()
            })
            .unwrap();
    }
    let mut renderer = ParticleRenderer::new(64).unwrap();
    let view = BillboardView::new(Vec3::new(0., 2., 8.), Vec3::X, Vec3::Y).unwrap();
    let mut trail = Trail::new(TrailConfig {
        capacity: 128,
        lifetime: 30.,
        spacing: 0.1,
        teleport_distance: 10.,
        ..Default::default()
    })
    .unwrap();
    for i in 0..300 {
        trail
            .update(
                0.01,
                Some(TrailSample {
                    position: [i as f32 * 0.1, 0., 0.],
                    color: [1.; 4],
                    width: 0.1,
                }),
            )
            .unwrap();
    }
    let mut beams = BeamPool::new(32, OverflowPolicy::ReplaceOldest);
    for _ in 0..32 {
        beams
            .spawn(BeamDesc {
                lifetime: 30.,
                ..Default::default()
            })
            .unwrap();
    }
    let mut draw = || {
        host_frame(1. / 60., 960, 600, || {
            renderer
                .draw_3d(&particles, view, ParticleStyle::default())
                .unwrap();
            effects::draw_ribbon(
                &trail,
                RibbonFacing::Camera {
                    eye: view.eye.to_array(),
                    up: view.up.to_array(),
                },
                2.,
                EffectStyle::default(),
            )
            .unwrap();
            effects::draw_beams(
                &beams,
                RibbonFacing::Camera {
                    eye: view.eye.to_array(),
                    up: view.up.to_array(),
                },
                EffectStyle {
                    blend: EffectBlend::Additive,
                    ..Default::default()
                },
            )
            .unwrap();
        })
    };
    for _ in 0..4 {
        draw();
    }
    CALLS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    for _ in 0..1000 {
        draw();
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    assert!(effects::stats().vertices > 0);
}
