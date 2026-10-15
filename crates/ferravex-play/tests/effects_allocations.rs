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
fn emitters_particles_trails_ribbons_and_beams_reuse_all_storage() {
    use ferravex_play::particles::*;
    let desc = EmitterDesc {
        particle: ParticleDesc {
            lifetime: 2.,
            start_size: 0.1,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 2.,
            angular_velocity: 3.,
            ..Default::default()
        },
        rate: 96.,
        velocity_jitter: [1.; 3],
        ..Default::default()
    };
    let mut emitter = Emitter::new(desc, 42).unwrap();
    let mut particles = ParticlePool::new(256);
    let mut trail = Trail::new(TrailConfig {
        capacity: 128,
        ..Default::default()
    })
    .unwrap();
    let mut beams = BeamPool::new(32, OverflowPolicy::ReplaceOldest);
    let mut path = RibbonPath::new(3).unwrap();
    let points = [
        RibbonPoint {
            position: [0., 0., 0.],
            color: [1.; 4],
            width: 0.1,
            break_before: true,
        },
        RibbonPoint {
            position: [1., 1., 0.],
            color: [1.; 4],
            width: 0.2,
            break_before: false,
        },
        RibbonPoint {
            position: [2., 0., 0.],
            color: [1.; 4],
            width: 0.1,
            break_before: false,
        },
    ];
    CALLS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    let mut triangles = 0;
    for i in 0..1000 {
        particles.update(1. / 60.).unwrap();
        emitter
            .update(
                1. / 60.,
                EmitterTransform::at([i as f32 * 0.05, 0., 0.]),
                &mut particles,
            )
            .unwrap();
        if i % 17 == 0 {
            emitter
                .burst(8, EmitterTransform::default(), &mut particles)
                .unwrap();
        }
        if i % 193 == 0 {
            emitter
                .burst(u32::MAX, EmitterTransform::default(), &mut particles)
                .unwrap();
        }
        trail
            .update(
                1. / 60.,
                Some(TrailSample {
                    position: [i as f32 * 0.05, 0., 0.],
                    color: [1.; 4],
                    width: 0.2,
                }),
            )
            .unwrap();
        path.set(&points).unwrap();
        triangles += RibbonTriangles::new(
            &trail,
            RibbonFacing::Plane {
                normal: [0., 0., 1.],
            },
            2.,
        )
        .unwrap()
        .count();
        triangles += RibbonTriangles::new(
            &path,
            RibbonFacing::Camera {
                eye: [0., 2., 5.],
                up: [0., 1., 0.],
            },
            2.,
        )
        .unwrap()
        .count();
        beams.update(1. / 60.).unwrap();
        beams.spawn(BeamDesc::default()).unwrap();
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    assert!(triangles > 1000);
}
