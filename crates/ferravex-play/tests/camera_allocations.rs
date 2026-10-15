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
fn all_camera_modes_queries_and_layered_shake_allocate_nothing() {
    use ferravex_math::{Quat, Vec2, Vec3};
    use ferravex_play::camera::*;
    let mut camera = Rig3::new(Settings3 {
        obstruction: Some(ObstructionSettings::default()),
        ..Default::default()
    })
    .unwrap();
    let mut planar = Follow2::default();
    let obstacles = [Blocker::Box {
        center: Vec3::Z * 6.,
        half: Vec3::new(1., 2., 0.1),
        rotation: Quat::from_rotation_y(0.3),
    }];
    RECORD.with(|r| r.set(true));
    CALLS.with(|n| n.set(0));
    for i in 0..1200 {
        let yaw = i as f32 * 0.01;
        let mode = match i % 400 {
            0..100 => Mode3::Follow {
                eye_offset: Vec3::new(2., 3., 8.),
                look_offset: Vec3::ZERO,
            },
            100..200 => Mode3::Orbit {
                yaw,
                pitch: 0.5,
                distance: 10.,
                pivot: Vec3::Y,
            },
            200..300 => Mode3::FirstPerson {
                eye_offset: Vec3::Y,
                yaw,
                pitch: 0.2,
            },
            _ => Mode3::Chase {
                distance: 8.,
                height: 3.,
                look_ahead: 2.,
                look_height: 1.,
                speed_distance: 0.01,
                speed_height: 0.01,
                prediction: 0.1,
                level: true,
            },
        };
        if i % 20 == 0 {
            camera
                .shake
                .push(ShakeImpulse {
                    translation: Vec3::splat(0.1),
                    rotation: Vec3::splat(0.01),
                    duration: 0.2,
                    frequency: 20.,
                })
                .unwrap();
        }
        let frame = camera
            .update(
                1. / 120.,
                Input3 {
                    target: Target3::at(Vec3::new(yaw.sin() * 2., 2., 0.)),
                    mode,
                    fov_y: 1.,
                },
                &obstacles[..],
            )
            .unwrap();
        assert!(frame.queries <= 3);
        planar
            .update(1. / 120., Vec2::new(yaw, 0.), Vec2::X)
            .unwrap();
    }
    RECORD.with(|r| r.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}
