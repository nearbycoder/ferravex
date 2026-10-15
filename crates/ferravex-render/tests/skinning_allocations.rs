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
fn validated_palettes_bounds_and_recorded_skin_frames_allocate_nothing_after_warmup() {
    use ferravex_render::{
        Camera3D, GREEN, Mat4, Vec3, draw_skinned,
        input::host_frame,
        mesh::Mesh3d,
        set_camera,
        skinning::{SkinPalette, SkinWeights, SkinnedMesh},
    };
    let mesh = Mesh3d::cuboid(Vec3::ONE, GREEN).mesh;
    let skin =
        SkinnedMesh::new(&mesh, &vec![SkinWeights::rigid(0); mesh.vertices.len()], 32).unwrap();
    let mut matrices = [Mat4::IDENTITY.to_cols_array(); 32];
    let mut palette = SkinPalette::new(&matrices).unwrap();
    let mut run = |i: usize| {
        matrices[0] = Mat4::from_translation(Vec3::X * (i % 7) as f32 * 0.01).to_cols_array();
        palette.set(&matrices).unwrap();
        host_frame(1. / 60., 960, 600, || {
            set_camera(&Camera3D {
                position: Vec3::new(0., 2., 8.),
                target: Vec3::ZERO,
                ..Default::default()
            });
            for j in 0..16 {
                assert!(draw_skinned(
                    &skin,
                    Mat4::from_translation(Vec3::X * (j % 3) as f32 * 0.1),
                    &palette
                ));
            }
        });
    };
    for i in 0..4 {
        run(i);
    }
    CALLS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    for i in 0..1000 {
        run(i);
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}
