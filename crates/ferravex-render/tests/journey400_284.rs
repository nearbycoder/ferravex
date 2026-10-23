//! Allocation evidence for the registered public atomic resident submission API.
use ferravex_render::{
    Camera, Mat4, Vec3, WHITE,
    input::host_frame,
    mesh::Mesh3d,
    retained::{
        InstanceGroupReport, ResidentMesh, draw_residents_checked, reset_telemetry,
        telemetry_snapshot,
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct CountingAllocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = ALLOCATIONS.try_with(|cell| {
            if let Some(count) = cell.get() {
                cell.set(Some(count + 1));
            }
        });
    }
}
// SAFETY: Forward the identical allocation/deallocation contracts to System.
// The optional thread-local counter allocates nothing and tolerates teardown.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        allocated(pointer);
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        allocated(pointer);
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        allocated(pointer);
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct StopCounting;
impl Drop for StopCounting {
    fn drop(&mut self) {
        ALLOCATIONS.with(|cell| cell.set(None));
    }
}
fn allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|cell| cell.set(Some(0)));
    let guard = StopCounting;
    run();
    let count = ALLOCATIONS.with(Cell::get).unwrap();
    drop(guard);
    count
}
struct IdentityCamera;
impl Camera for IdentityCamera {
    fn matrix(&self, _: f32, _: f32) -> Mat4 {
        Mat4::IDENTITY
    }
    fn depth(&self) -> bool {
        true
    }
}

#[test]
fn warmed_groups_culling_errors_telemetry_and_finalization_allocate_nothing() {
    assert!(
        allocations(|| {
            black_box(vec![black_box(1_u8); 256]);
        }) > 0
    );
    let source = Mesh3d::cuboid(Vec3::ONE, WHITE).mesh;
    let a = ResidentMesh::try_new(&source).unwrap();
    let b = ResidentMesh::try_new(&source).unwrap();
    let material = ferravex_render::material::ShaderMaterial::new(
        Default::default(),
        ferravex_render::material::MaterialOptions {
            blend: ferravex_render::material::BlendMode::Opaque,
            ..Default::default()
        },
    )
    .unwrap();
    let mut input = [(&a, Mat4::IDENTITY); 32];
    for (i, pair) in input.iter_mut().enumerate() {
        *pair = (
            if i % 2 == 0 { &a } else { &b },
            Mat4::from_translation(Vec3::X * if i < 16 { i as f32 * 0.01 } else { 100. }),
        );
    }
    let invalid = [(&a, Mat4::IDENTITY), (&b, Mat4::from_scale(Vec3::ZERO))];
    let frame = || {
        reset_telemetry();
        host_frame(0., 800, 600, || {
            ferravex_render::set_camera(&IdentityCamera);
            material.begin([[0.25; 4]; 4]).unwrap();
            assert_eq!(
                draw_residents_checked(&input),
                InstanceGroupReport {
                    accepted: 16,
                    visible: 16,
                    culled: 16,
                    ..Default::default()
                }
            );
            let before = telemetry_snapshot();
            let failed = draw_residents_checked(&invalid);
            assert_eq!(
                (
                    failed.accepted,
                    failed.rejected,
                    failed.error.unwrap().index
                ),
                (0, 2, 1)
            );
            assert_eq!(telemetry_snapshot().instances, before.instances);
            assert_eq!(telemetry_snapshot().buffers, before.buffers);
            assert_eq!(telemetry_snapshot().rejections.draw_validation, 1);
            assert_eq!(draw_residents_checked(&[]), InstanceGroupReport::default());
        });
        // Opaque finalization regroups alternating handles through existing scratch.
        assert_eq!(telemetry_snapshot().draws, 2);
        material.end();
    };
    for _ in 0..4 {
        frame();
    }
    assert_eq!(
        allocations(|| {
            for _ in 0..128 {
                frame();
            }
        }),
        0
    );
}

#[test]
fn warmed_capacity_failure_and_full_frame_culls_allocate_nothing() {
    let mesh = ResidentMesh::try_new(&Mesh3d::cuboid(Vec3::ONE, WHITE).mesh).unwrap();
    let limit = telemetry_snapshot().instance_limit;
    let input = vec![(&mesh, Mat4::IDENTITY); limit];
    let outside = [(&mesh, Mat4::from_translation(Vec3::X * 100.))];
    let frame = || {
        reset_telemetry();
        host_frame(0., 800, 600, || {
            ferravex_render::set_camera(&IdentityCamera);
            assert_eq!(draw_residents_checked(&input).accepted, limit);
            let before = telemetry_snapshot();
            let failed = draw_residents_checked(&input[..2]);
            assert_eq!(
                (
                    failed.accepted,
                    failed.rejected,
                    failed.error.unwrap().index
                ),
                (0, 2, 0)
            );
            assert_eq!(telemetry_snapshot().instances, before.instances);
            assert_eq!(telemetry_snapshot().buffers, before.buffers);
            assert_eq!(telemetry_snapshot().rejections.instance_capacity, 1);
            assert_eq!(
                draw_residents_checked(&outside),
                InstanceGroupReport {
                    culled: 1,
                    ..Default::default()
                }
            );
        });
    };
    for _ in 0..3 {
        frame();
    }
    assert_eq!(
        allocations(|| {
            for _ in 0..3 {
                frame();
            }
        }),
        0
    );
}
