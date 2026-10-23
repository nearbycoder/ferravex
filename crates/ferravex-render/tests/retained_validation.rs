use ferravex_render::{
    Camera, Camera3D, Mat4, Mesh, Vec3, WHITE, draw_resident,
    input::host_frame,
    mesh::Mesh3d,
    retained::{
        DrawError, MeshError, ResidentMesh, draw_resident_checked, validate_mesh,
        validate_transform,
    },
    set_camera, set_default_camera,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct CountingAllocator;
fn count_allocation(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = COUNT_ALLOCATIONS.try_with(|enabled| {
            if enabled.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        });
    }
}
// SAFETY: Forward the identical allocation contract to System. The counters
// use allocation-free TLS and tolerate thread-local teardown.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        count_allocation(pointer);
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        count_allocation(pointer);
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        count_allocation(pointer);
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn source() -> Mesh {
    Mesh3d::cuboid(Vec3::ONE, WHITE).mesh
}

#[test]
fn checked_mesh_errors_locate_bad_indices_and_uvs_and_implement_error() {
    let mut source = source();
    source.indices[2] = u16::MAX;
    let expected = MeshError::IndexOutOfBounds {
        index_position: 2,
        index: u16::MAX,
        vertex_count: source.vertices.len(),
    };
    assert_eq!(ResidentMesh::try_new(&source).err(), Some(expected));
    assert_eq!(validate_mesh(&source), Err(expected));
    let error: &dyn std::error::Error = &expected;
    assert!(error.to_string().contains("index[2] is 65535"));
    source.indices[2] = 0;
    source.vertices[0].uv.y = f32::INFINITY;
    let error = ResidentMesh::try_new(&source).err().unwrap();
    assert_eq!(
        error,
        MeshError::NonFiniteVertex {
            vertex: 0,
            attribute: "uv",
            component: 1,
        }
    );
    assert!(error.to_string().contains("vertex[0].uv[1]"));
    assert_eq!(
        ResidentMesh::new(&source).err(),
        Some("invalid retained mesh geometry")
    );
    source.vertices[0].uv.y = -4.; // Wrapping coordinates remain valid.
    assert!(ResidentMesh::try_new(&source).is_ok());
}

#[test]
fn transform_errors_cover_each_matrix_component_and_noninvertible_scales() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for column in 0..4 {
            for row in 0..4 {
                let mut model = Mat4::IDENTITY;
                model[column][row] = value;
                assert_eq!(
                    validate_transform(model),
                    Err(DrawError::NonFiniteModel { column, row })
                );
            }
        }
    }
    for column in 0..4 {
        let mut model = Mat4::IDENTITY;
        model[column][3] = 0.5;
        let error = validate_transform(model).unwrap_err();
        assert_eq!(error, DrawError::NonAffineModel { column });
        assert!(error.to_string().contains("put projection in the camera"));
    }
    for scale in [
        Vec3::ZERO,
        Vec3::new(1., 0., 1.),
        Vec3::new(f32::from_bits(1), 1., 1.),
    ] {
        assert_eq!(
            validate_transform(Mat4::from_scale(scale)),
            Err(DrawError::NonInvertibleModel)
        );
    }
    let mut shear = Mat4::IDENTITY;
    shear.y_axis.x = 0.75;
    for model in [
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::new(5., -3., 9.)),
        Mat4::from_scale(Vec3::new(-2., 4., 8.)),
        Mat4::from_scale(Vec3::splat(1e-20)),
        shear,
    ] {
        assert_eq!(validate_transform(model), Ok(()));
    }
}

#[test]
fn checked_draw_distinguishes_camera_errors_and_culling_without_a_gpu() {
    struct MatrixCamera(Mat4);
    impl Camera for MatrixCamera {
        fn matrix(&self, _: f32, _: f32) -> Mat4 {
            self.0
        }
        fn depth(&self) -> bool {
            false
        }
    }
    let mesh = ResidentMesh::try_new(&source()).unwrap();
    host_frame(1. / 60., 800, 600, || {
        for column in 0..4 {
            for row in 0..4 {
                let mut camera = Mat4::IDENTITY;
                camera[column][row] = f32::NAN;
                set_camera(&MatrixCamera(camera));
                assert_eq!(
                    draw_resident_checked(&mesh, Mat4::IDENTITY),
                    Err(DrawError::NonFiniteCamera { column, row })
                );
                assert!(!draw_resident(&mesh, Mat4::IDENTITY));
            }
        }
        set_camera(&Camera3D {
            position: Vec3::new(0., 2., 8.),
            target: Vec3::ZERO,
            ..Default::default()
        });
        assert_eq!(draw_resident_checked(&mesh, Mat4::IDENTITY), Ok(true));
        let outside = Mat4::from_translation(Vec3::new(10_000., 0., 0.));
        assert_eq!(draw_resident_checked(&mesh, outside), Ok(false));
        // The same invisible object with a singular transform is an error.
        let singular = outside * Mat4::from_scale(Vec3::ZERO);
        assert_eq!(
            draw_resident_checked(&mesh, singular),
            Err(DrawError::NonInvertibleModel)
        );
        assert!(!draw_resident(&mesh, singular));
    });
}

#[test]
fn skinned_mesh_requires_its_palette_instead_of_silently_reporting_culling() {
    use ferravex_render::skinning::{SkinWeights, SkinnedMesh};
    let source = source();
    let skin = SkinnedMesh::new(
        &source,
        &vec![SkinWeights::rigid(0); source.vertices.len()],
        1,
    )
    .unwrap();
    host_frame(1. / 60., 800, 600, || {
        set_default_camera();
        assert_eq!(
            draw_resident_checked(skin.resident(), Mat4::IDENTITY),
            Err(DrawError::SkinPaletteMismatch {
                expected: 1,
                actual: 0
            })
        );
        assert!(!draw_resident(skin.resident(), Mat4::IDENTITY));
    });
}

#[test]
fn checked_frames_and_failed_validation_allocate_nothing_after_warmup() {
    let source = source();
    let mesh = ResidentMesh::try_new(&source).unwrap();
    let mut malformed = source.clone();
    malformed.vertices[0].normal.z = f32::NAN;
    let draw = || {
        host_frame(1. / 60., 800, 600, || {
            set_default_camera();
            for i in 0..16 {
                validate_mesh(&source).unwrap();
                assert!(ResidentMesh::try_new(&malformed).is_err());
                let model = Mat4::from_translation(Vec3::new(i as f32, 0., 0.));
                assert_eq!(draw_resident_checked(&mesh, model), Ok(true));
                assert_eq!(
                    draw_resident_checked(&mesh, Mat4::from_scale(Vec3::ZERO)),
                    Err(DrawError::NonInvertibleModel)
                );
            }
        })
    };
    for _ in 0..4 {
        draw();
    }
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            COUNT_ALLOCATIONS.with(|enabled| enabled.set(false));
        }
    }
    ALLOCATIONS.with(|count| count.set(0));
    COUNT_ALLOCATIONS.with(|enabled| enabled.set(true));
    let guard = StopCounting;
    for _ in 0..128 {
        draw();
    }
    drop(guard);
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
}
