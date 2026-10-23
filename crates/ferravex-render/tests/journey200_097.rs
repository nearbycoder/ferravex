use ferravex_render::{
    Mat4, Mesh, Vec3, WHITE,
    input::host_frame,
    mesh::Mesh3d,
    retained::{
        ResidentMesh, RetainedRejections, draw_resident_checked, reset_telemetry,
        telemetry_snapshot,
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
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
// SAFETY: Forward the allocator contract unchanged to System. The optional
// thread-local counter allocates nothing and tolerates thread teardown.
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
        unsafe { System.dealloc(pointer, layout) };
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn retained_telemetry_and_rejections_allocate_nothing_in_warmed_frames() {
    let mesh = ResidentMesh::try_new(&Mesh3d::cuboid(Vec3::ONE, WHITE).mesh).unwrap();
    let other = ResidentMesh::try_new(&Mesh3d::cuboid(Vec3::ONE, WHITE).mesh).unwrap();
    let invalid = Mesh::default();
    let frame = || {
        reset_telemetry();
        host_frame(1. / 60., 800, 600, || {
            for mesh in [&mesh, &other].into_iter().cycle().take(16) {
                assert_eq!(draw_resident_checked(mesh, Mat4::IDENTITY), Ok(true));
                assert!(draw_resident_checked(mesh, Mat4::ZERO).is_err());
                assert!(ResidentMesh::try_new(&invalid).is_err());
                let sample = telemetry_snapshot();
                assert_eq!(telemetry_snapshot(), sample);
            }
        });
        let sample = telemetry_snapshot();
        assert_eq!((sample.instances, sample.draws), (16, 16));
        assert_eq!(sample.rejections.mesh_validation, 16);
        assert_eq!(sample.rejections.draw_validation, 16);
        assert_eq!(reset_telemetry(), sample.rejections);
        let reset = telemetry_snapshot();
        assert_eq!(reset.rejections, RetainedRejections::default());
        assert_eq!(reset.buffers, sample.buffers);
        assert_eq!(reset.instances, sample.instances);
        assert_eq!(reset.live_meshes, sample.live_meshes);
    };
    for _ in 0..4 {
        frame();
    }
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            ALLOCATIONS.with(|cell| cell.set(None));
        }
    }
    ALLOCATIONS.with(|cell| cell.set(Some(0)));
    let guard = StopCounting;
    for _ in 0..128 {
        frame();
    }
    let allocations = ALLOCATIONS.with(Cell::get).unwrap();
    drop(guard);
    assert_eq!(allocations, 0);
}
