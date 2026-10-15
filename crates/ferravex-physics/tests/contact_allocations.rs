#![cfg(feature = "dim3")]
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
fn steady_contact_steps_reuse_storage_awake_and_asleep() {
    use ferravex_physics::{SolverSettings, d3::PhysicsWorld};
    for sleep in [false, true] {
        let mut w = PhysicsWorld::new([0., -9.81, 0.]);
        w.set_solver_settings(SolverSettings {
            allow_sleep: sleep,
            ..Default::default()
        })
        .unwrap();
        w.add_box([0., -0.5, 0.], [10., 0.5, 10.], false);
        for i in 0..4 {
            w.add_box([0., 0.5 + i as f32 * 1.002, 0.], [0.5; 3], true);
        }
        for _ in 0..1200 {
            w.step(1. / 120.);
        }
        CALLS.with(|n| n.set(0));
        RECORD.with(|r| r.set(true));
        for _ in 0..120 {
            w.step(1. / 120.);
        }
        RECORD.with(|r| r.set(false));
        assert_eq!(
            CALLS.with(Cell::get),
            0,
            "steady step allocated with allow_sleep={sleep}"
        );
    }
}

#[test]
fn steady_joints_and_compounds_reuse_storage() {
    use ferravex_math::Vec3;
    use ferravex_physics::{
        JointMotor, SolverSettings,
        d3::{ColliderBuilder, JointBuilder, PhysicsWorld, RigidBodyBuilder},
    };
    let mut w = PhysicsWorld::default();
    w.set_solver_settings(SolverSettings {
        allow_sleep: false,
        ..Default::default()
    })
    .unwrap();
    let anchor = w.insert_body(RigidBodyBuilder::fixed()).unwrap();
    for i in 0..8 {
        let x = i as f32 * 4.;
        let body = w
            .insert_compound(
                RigidBodyBuilder::dynamic().translation(Vec3::new(x, 0., 0.)),
                [
                    ColliderBuilder::cuboid(0.7, 0.1, 0.1),
                    ColliderBuilder::ball(0.3).translation(Vec3::X * 0.7),
                    ColliderBuilder::ball(0.3).translation(-Vec3::X * 0.7),
                ],
            )
            .unwrap();
        w.insert_joint(
            anchor,
            body,
            JointBuilder::hinge()
                .local_anchor1(Vec3::new(x, 0., 0.))
                .motor(JointMotor::velocity(2., 5.)),
        )
        .unwrap();
    }
    for _ in 0..240 {
        w.step(1. / 120.);
    }
    CALLS.with(|n| n.set(0));
    RECORD.with(|r| r.set(true));
    for _ in 0..120 {
        w.step(1. / 120.);
    }
    RECORD.with(|r| r.set(false));
    assert_eq!(CALLS.with(Cell::get), 0, "steady joint step allocated");
}
