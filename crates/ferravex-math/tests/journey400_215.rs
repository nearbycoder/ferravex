//! Projection behavior through the registered production module.
use ferravex_math::projection::{
    FarPlane, NdcDepth, PreparedProjector3D, ProjectionError, ScreenYAxis, Viewport,
};
use ferravex_math::{Mat4, MatrixError, Vec2, Vec3, Vec4, vec2, vec3, vec4};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

fn viewport() -> Viewport {
    Viewport {
        origin: vec2(20., 80.),
        size: vec2(800., 400.),
    }
}

fn prepare(matrix: Mat4, depth: NdcDepth, y: ScreenYAxis) -> PreparedProjector3D {
    PreparedProjector3D::try_new(matrix, viewport(), y, depth, FarPlane::Finite).unwrap()
}

fn near(a: Vec3, b: Vec3, tolerance: f32) {
    assert!(a.abs_diff_eq(b, tolerance), "{a:?} != {b:?}");
}

#[test]
fn offset_edges_y_flips_and_pixel_centers_share_one_mapping() {
    for (y, top, bottom) in [(ScreenYAxis::Up, -1., 1.), (ScreenYAxis::Down, 1., -1.)] {
        let p = prepare(Mat4::IDENTITY, NdcDepth::NegativeOneToOne, y);
        for (world, screen) in [
            (vec3(-1., top, 0.), vec2(20., 80.)),
            (vec3(1., bottom, 0.), vec2(820., 480.)),
            (Vec3::ZERO, vec2(420., 280.)),
        ] {
            let result = p.project(world).unwrap();
            assert_eq!(result.screen, screen);
            assert!(result.inside_viewport && result.inside_depth);
            near(p.unproject(screen, 0.).unwrap(), world, 1e-6);
        }
        // Integer coordinates are edges; no half-pixel is inserted by the API.
        let center = p.unproject(vec2(20.5, 80.5), 0.).unwrap();
        assert!((center.x - (-1. + 1. / 800.)).abs() < 1e-7);
        let expected_y = top + (bottom - top) * 0.5 / 400.;
        assert!((center.y - expected_y).abs() < 1e-7);
        assert!(
            p.project(center)
                .unwrap()
                .screen
                .abs_diff_eq(vec2(20.5, 80.5), 2e-5)
        );
        // Both maximum edges are accepted, but their next representable values are not.
        assert!(p.pick_ray(vec2(820., 480.)).is_ok());
        assert_eq!(
            p.pick_ray(vec2(820_f32.next_up(), 480.)),
            Err(ProjectionError::OutsideViewport)
        );
    }
}

#[test]
fn fractional_offset_uses_the_same_rounded_edges_for_paint_and_picking() {
    let vp = Viewport {
        origin: vec2(0.1, -0.2),
        size: vec2(800.3, 400.2),
    };
    let p = PreparedProjector3D::try_new(
        Mat4::IDENTITY,
        vp,
        ScreenYAxis::Up,
        NdcDepth::NegativeOneToOne,
        FarPlane::Finite,
    )
    .unwrap();
    let screen = p.project(Vec3::ONE).unwrap().screen;
    assert_eq!(screen, vp.origin + vp.size);
    assert_eq!(p.unproject(screen, 1.), Ok(Vec3::ONE));
}

#[test]
fn perspective_handedness_depth_conventions_and_reversed_depth_round_trip() {
    let gl = Mat4::try_perspective_rh_gl(1.2, 2., 0.5, 40.).unwrap();
    let zo = Mat4::try_perspective_rh(1.2, 2., 0.5, 40.).unwrap();
    let lh = Mat4::try_perspective_lh(1.2, 2., 0.5, 40.).unwrap();
    let reverse_zo = Mat4::from_cols(Vec4::X, Vec4::Y, -Vec4::Z, vec4(0., 0., 1., 1.));
    let reverse_gl = Mat4::from_scale(vec3(1., 1., -1.));
    for (lens, depth, sign) in [
        (gl, NdcDepth::NegativeOneToOne, -1.),
        (zo, NdcDepth::ZeroToOne, -1.),
        (lh, NdcDepth::ZeroToOne, 1.),
        (reverse_zo * zo, NdcDepth::ReverseZeroToOne, -1.),
        (reverse_gl * gl, NdcDepth::ReverseNegativeOneToOne, -1.),
    ] {
        let p = prepare(lens, depth, ScreenYAxis::Down);
        for world in [vec3(0.2, -0.1, sign * 2.), vec3(-1., 0.5, sign * 20.)] {
            let result = p.project(world).unwrap();
            assert!(result.inside_viewport && result.inside_depth);
            near(
                p.unproject(result.screen, result.ndc_depth).unwrap(),
                world,
                8e-5,
            );
        }
        let near_point = p.unproject(vec2(420., 280.), depth.near()).unwrap();
        let far_point = p.unproject(vec2(420., 280.), depth.far()).unwrap();
        near(near_point, vec3(0., 0., sign * 0.5), 1e-6);
        near(far_point, vec3(0., 0., sign * 40.), 0.001);
        let ray = p.pick_ray(vec2(420., 280.)).unwrap();
        near(ray.origin, near_point, 0.);
        near(ray.far_point, far_point, 0.);
        near(ray.direction, Vec3::Z * sign, 1e-6);
        let edge_ray = p.pick_ray(vec2(820., 80.)).unwrap();
        assert!(edge_ray.direction.x > 0. && edge_ray.direction.y > 0.);
        assert!(edge_ray.direction.z * sign > 0.);
        assert!((edge_ray.direction.length() - 1.).abs() < 1e-6);
        // Perspective rays start at the near plane and diverge, never at the eye.
        assert_ne!(edge_ray.origin, Vec3::ZERO);
        assert_ne!(edge_ray.direction, ray.direction);
    }
}

#[test]
fn orthographic_rays_have_distinct_origins_and_parallel_directions() {
    for (lens, depth, sign) in [
        (
            Mat4::try_orthographic_rh_gl(-4., 4., -2., 2., 0.5, 40.).unwrap(),
            NdcDepth::NegativeOneToOne,
            -1.,
        ),
        (
            Mat4::try_orthographic_rh(-4., 4., -2., 2., 0.5, 40.).unwrap(),
            NdcDepth::ZeroToOne,
            -1.,
        ),
        (
            Mat4::try_orthographic_lh(-4., 4., -2., 2., 0.5, 40.).unwrap(),
            NdcDepth::ZeroToOne,
            1.,
        ),
    ] {
        let p = prepare(lens, depth, ScreenYAxis::Down);
        let a = p.pick_ray(vec2(20., 80.)).unwrap();
        let b = p.pick_ray(vec2(820., 480.)).unwrap();
        near(a.origin, vec3(-4., 2., sign * 0.5), 1e-5);
        near(b.origin, vec3(4., -2., sign * 0.5), 1e-5);
        near(a.direction, Vec3::Z * sign, 1e-6);
        near(a.direction, b.direction, 1e-6);
        let world = vec3(0.5, -0.75, sign * 3.);
        let projected = p.project(world).unwrap();
        near(
            p.unproject(projected.screen, projected.ndc_depth).unwrap(),
            world,
            1e-5,
        );
    }
}

#[test]
fn composed_camera_uses_world_space_for_hud_and_picking() {
    let eye = vec3(4., 3., 8.);
    let view = Mat4::try_look_at_rh(eye, Vec3::ZERO, Vec3::Y).unwrap();
    let lens = Mat4::try_perspective_rh_gl(1., 2., 0.5, 50.).unwrap();
    let p = prepare(lens * view, NdcDepth::NegativeOneToOne, ScreenYAxis::Down);
    let expected_direction = (-eye).try_normalize().unwrap();
    let ray = p.pick_ray(vec2(420., 280.)).unwrap();
    near(ray.origin, eye + expected_direction * 0.5, 3e-6);
    near(ray.direction, expected_direction, 1e-6);
    for world in [Vec3::ZERO, vec3(0.5, -0.25, 1.), vec3(-1., 0.75, -2.)] {
        let anchor = p.project(world).unwrap();
        assert!(anchor.inside_depth && anchor.inside_viewport);
        near(
            p.unproject(anchor.screen, anchor.ndc_depth).unwrap(),
            world,
            3e-5,
        );
    }
}

#[test]
fn clipping_and_behind_camera_policy_do_not_silently_clamp() {
    let lens = Mat4::try_perspective_rh_gl(1., 2., 0.5, 40.).unwrap();
    let p = prepare(lens, NdcDepth::NegativeOneToOne, ScreenYAxis::Down);
    assert_eq!(p.project(Vec3::Z), Err(ProjectionError::BehindCamera));
    // The half-space rejection precedes division even when divided XY overflows.
    assert_eq!(
        p.project(vec3(f32::MAX, 0., f32::from_bits(1))),
        Err(ProjectionError::BehindCamera)
    );
    assert_eq!(
        p.project(vec3(f32::MAX, 0., -f32::from_bits(1))),
        Err(ProjectionError::Matrix(MatrixError::ResultOutOfRange))
    );
    assert_eq!(
        p.project(Vec3::ZERO),
        Err(ProjectionError::Matrix(MatrixError::ZeroHomogeneousW))
    );
    for point in [vec3(0., 0., -0.25), vec3(0., 0., -80.)] {
        let result = p.project(point).unwrap();
        assert!(result.inside_viewport);
        assert!(!result.inside_depth);
        assert_eq!(
            p.unproject(result.screen, result.ndc_depth),
            Err(ProjectionError::OutsideDepth)
        );
    }
    let side = p.project(vec3(100., 0., -1.)).unwrap();
    assert!(!side.inside_viewport);
    assert!(side.inside_depth);
    assert!(side.screen.x > 820.);
    assert_eq!(
        p.unproject(side.screen, side.ndc_depth),
        Err(ProjectionError::OutsideViewport)
    );

    // Orthographic W cannot identify a camera eye that is absent from the matrix.
    let ortho = prepare(
        Mat4::try_orthographic_rh_gl(-1., 1., -1., 1., 0.5, 40.).unwrap(),
        NdcDepth::NegativeOneToOne,
        ScreenYAxis::Up,
    );
    assert!(!ortho.project(Vec3::Z).unwrap().inside_depth);
}

#[test]
fn query_nonfinite_depth_and_viewport_failures_are_distinct() {
    let p = prepare(Mat4::IDENTITY, NdcDepth::ZeroToOne, ScreenYAxis::Up);
    for point in [Vec3::NAN, Vec3::INFINITY] {
        assert_eq!(
            p.project(point),
            Err(ProjectionError::Matrix(MatrixError::NonFiniteInput {
                input: "point"
            }))
        );
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            p.pick_ray(vec2(value, 80.)),
            Err(ProjectionError::NonFiniteScreen)
        );
        assert_eq!(
            p.unproject(vec2(20., value), 0.),
            Err(ProjectionError::NonFiniteScreen)
        );
        assert_eq!(
            p.unproject(vec2(20., 80.), value),
            Err(ProjectionError::NonFiniteDepth)
        );
    }
    for depth in [-f32::from_bits(1), 1_f32.next_up()] {
        assert_eq!(
            p.unproject(vec2(20., 80.), depth),
            Err(ProjectionError::OutsideDepth)
        );
    }
    for screen in [
        vec2(19.99, 80.),
        vec2(20., 79.99),
        vec2(820.01, 80.),
        vec2(20., 480.01),
    ] {
        assert_eq!(p.pick_ray(screen), Err(ProjectionError::OutsideViewport));
    }
    assert_eq!(p.unproject(vec2(20., 80.), 0.), Ok(vec3(-1., -1., 0.)));
    assert_eq!(p.unproject(vec2(820., 480.), 1.), Ok(Vec3::ONE));
}

#[test]
fn invalid_viewports_singular_nonfinite_and_failed_updates_retain_last_good_state() {
    let mut p = prepare(
        Mat4::IDENTITY,
        NdcDepth::NegativeOneToOne,
        ScreenYAxis::Down,
    );
    let before = p;
    for vp in [
        Viewport {
            origin: Vec2::NAN,
            ..viewport()
        },
        Viewport {
            size: Vec2::INFINITY,
            ..viewport()
        },
        Viewport {
            size: vec2(0., 10.),
            ..viewport()
        },
        Viewport {
            size: vec2(10., -1.),
            ..viewport()
        },
        Viewport {
            size: vec2(f32::from_bits(1), 10.),
            ..viewport()
        },
        Viewport {
            size: vec2(0.999, 10.),
            ..viewport()
        },
        Viewport {
            origin: Vec2::splat(f32::MAX),
            size: Vec2::splat(f32::MAX),
        },
        Viewport {
            origin: Vec2::splat(1e20),
            size: Vec2::ONE,
        },
    ] {
        assert_eq!(
            p.try_update(
                Mat4::IDENTITY,
                vp,
                ScreenYAxis::Up,
                NdcDepth::ZeroToOne,
                FarPlane::Finite
            ),
            Err(ProjectionError::InvalidViewport)
        );
        assert_eq!(p, before);
    }
    for (matrix, error) in [
        (Mat4::ZERO, MatrixError::Singular { column: 0 }),
        (Mat4::NAN, MatrixError::NonFiniteInput { input: "matrix" }),
        (
            Mat4::from_scale(vec3(f32::from_bits(1), 1., 1.)),
            MatrixError::ResultOutOfRange,
        ),
    ] {
        assert_eq!(
            p.try_update(
                matrix,
                viewport(),
                ScreenYAxis::Up,
                NdcDepth::ZeroToOne,
                FarPlane::Finite
            ),
            Err(ProjectionError::Matrix(error))
        );
        assert_eq!(p, before);
    }
    assert_eq!(
        p.try_update(
            Mat4::IDENTITY,
            viewport(),
            ScreenYAxis::Up,
            NdcDepth::ZeroToOne,
            FarPlane::Infinite
        ),
        Err(ProjectionError::UnsupportedInfiniteFar)
    );
    assert_eq!(p, before);
    assert_eq!(
        p.try_update(
            Mat4::IDENTITY * -1.,
            viewport(),
            ScreenYAxis::Up,
            NdcDepth::ZeroToOne,
            FarPlane::Finite,
        ),
        Err(ProjectionError::InvalidClipVolume)
    );
    assert_eq!(p, before);
    // A successful replacement changes both mapping and depth; no stale cache remains.
    let matrix = Mat4::from_translation(vec3(0.25, 0., 0.));
    p.try_update(
        matrix,
        viewport(),
        ScreenYAxis::Up,
        NdcDepth::ZeroToOne,
        FarPlane::Finite,
    )
    .unwrap();
    assert_eq!(p.project(Vec3::ZERO).unwrap().screen, vec2(520., 280.));
    near(p.unproject(vec2(520., 280.), 0.).unwrap(), Vec3::ZERO, 1e-6);
    assert_ne!(p, before);
}

#[test]
fn finite_far_declaration_and_whole_clip_volume_validation_reject_infinity() {
    for (lens, depth) in [
        (
            Mat4::try_perspective_infinite_rh(1., 2., 0.5).unwrap(),
            NdcDepth::ZeroToOne,
        ),
        (
            Mat4::try_perspective_infinite_reverse_rh(1., 2., 0.5).unwrap(),
            NdcDepth::ReverseZeroToOne,
        ),
    ] {
        assert_eq!(
            PreparedProjector3D::try_new(
                lens,
                viewport(),
                ScreenYAxis::Down,
                depth,
                FarPlane::Infinite
            ),
            Err(ProjectionError::UnsupportedInfiniteFar)
        );
        // Misdeclaring an exactly infinite matrix as finite is also rejected.
        assert_eq!(
            PreparedProjector3D::try_new(
                lens,
                viewport(),
                ScreenYAxis::Down,
                depth,
                FarPlane::Finite
            ),
            Err(ProjectionError::InvalidClipVolume)
        );
    }
    // Inverse W is positive at the center but crosses zero toward a corner.
    let inverse = Mat4::from_cols(vec4(1., 0., 0., 2.), Vec4::Y, Vec4::Z, Vec4::W);
    for matrix in [inverse.checked_inverse().unwrap(), Mat4::IDENTITY * -1.] {
        assert_eq!(
            PreparedProjector3D::try_new(
                matrix,
                viewport(),
                ScreenYAxis::Up,
                NdcDepth::NegativeOneToOne,
                FarPlane::Finite
            ),
            Err(ProjectionError::InvalidClipVolume)
        );
    }
    // Finite camera coefficients can round to an infinite far plane; no invented endpoint.
    let rounded = Mat4::try_perspective_rh(1., 2., 0.5, f32::MAX).unwrap();
    assert_eq!(
        PreparedProjector3D::try_new(
            rounded,
            viewport(),
            ScreenYAxis::Down,
            NdcDepth::ZeroToOne,
            FarPlane::Finite
        ),
        Err(ProjectionError::InvalidClipVolume)
    );
}

#[test]
fn extreme_finite_values_tiny_positive_w_and_range_failures() {
    // Normalize a difference exceeding f32::MAX using f64 endpoint subtraction.
    let scale = f32::MAX * 0.75;
    let inverse = Mat4::from_scale(vec3(1., 1., scale));
    let p = prepare(
        inverse.checked_inverse().unwrap(),
        NdcDepth::NegativeOneToOne,
        ScreenYAxis::Up,
    );
    let ray = p.pick_ray(vec2(420., 280.)).unwrap();
    assert!(ray.origin.z < -f32::MAX * 0.5 && ray.far_point.z > f32::MAX * 0.5);
    assert!((ray.far_point.z - ray.origin.z).is_infinite());
    near(ray.direction, Vec3::Z, 1e-6);
    let projected = p.project(vec3(0., 0., scale * 0.25)).unwrap();
    near(
        p.unproject(projected.screen, projected.ndc_depth).unwrap() / scale,
        vec3(0., 0., 0.25),
        1e-6,
    );

    let tiny = Mat4::IDENTITY * 1e-30;
    let p = prepare(tiny, NdcDepth::NegativeOneToOne, ScreenYAxis::Up);
    assert_eq!(p.project(Vec3::ZERO).unwrap().screen, vec2(420., 280.));
    assert!(p.pick_ray(vec2(420., 280.)).is_ok());
    assert_eq!(
        p.project(Vec3::splat(f32::MAX)),
        Err(ProjectionError::ResultOutOfRange)
    );

    // Eight finite inverse corners can still exceed representable world coordinates.
    let inverse = Mat4::from_cols(
        Vec4::X * f32::MAX,
        Vec4::Y,
        Vec4::Z,
        vec4(f32::MAX, 0., 0., 1.),
    );
    assert_eq!(
        PreparedProjector3D::try_new(
            inverse.checked_inverse().unwrap(),
            viewport(),
            ScreenYAxis::Up,
            NdcDepth::NegativeOneToOne,
            FarPlane::Finite
        ),
        Err(ProjectionError::Matrix(MatrixError::ResultOutOfRange))
    );
}

#[test]
fn endpoints_collapsed_by_world_rounding_report_a_degenerate_ray() {
    // An invertible transform may map distinct depths to identical f32 points.
    let inverse = Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::Z * 1e-20, vec4(0., 0., 1., 1.));
    let p = prepare(
        inverse.checked_inverse().unwrap(),
        NdcDepth::ZeroToOne,
        ScreenYAxis::Up,
    );
    assert_eq!(
        p.pick_ray(vec2(420., 280.)),
        Err(ProjectionError::DegenerateRay)
    );
}

#[test]
fn errors_expose_matrix_sources() {
    use std::error::Error;
    let e = ProjectionError::from(MatrixError::ZeroHomogeneousW);
    assert!(e.source().unwrap().to_string().contains("homogeneous w"));
    assert!(ProjectionError::InvalidClipVolume.source().is_none());
}

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}

struct Allocator;

fn record_allocation() {
    let _ = RECORD.try_with(|record| {
        if record.get() {
            let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
        }
    });
}

// SAFETY: Forward unchanged allocation contracts to System. Allocation-free TLS
// counters tolerate teardown and isolate this test from concurrent harness work.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn allocations(action: impl FnOnce()) -> usize {
    struct StopRecording;
    impl Drop for StopRecording {
        fn drop(&mut self) {
            RECORD.set(false);
        }
    }
    CALLS.set(0);
    RECORD.set(true);
    let recording = StopRecording;
    action();
    drop(recording);
    CALLS.get()
}

#[test]
fn allocation_counter_observes_a_real_heap_allocation() {
    assert!(
        allocations(|| {
            black_box(Box::new(42));
        }) > 0
    );
}

#[test]
fn preparation_queries_and_error_recovery_allocate_nothing() {
    let lens = Mat4::try_perspective_rh_gl(1., 2., 0.5, 40.).unwrap();
    let count = allocations(|| {
        let mut p = prepare(lens, NdcDepth::NegativeOneToOne, ScreenYAxis::Down);
        for _ in 0..128 {
            let result = black_box(p.project(black_box(vec3(0.1, -0.2, -3.)))).unwrap();
            black_box(p.unproject(result.screen, result.ndc_depth)).unwrap();
            black_box(p.pick_ray(result.screen)).unwrap();
            assert_eq!(
                p.pick_ray(Vec2::ZERO),
                Err(ProjectionError::OutsideViewport)
            );
            assert_eq!(p.project(Vec3::Z), Err(ProjectionError::BehindCamera));
        }
        let before = p;
        assert!(
            p.try_update(
                Mat4::ZERO,
                viewport(),
                ScreenYAxis::Up,
                NdcDepth::ZeroToOne,
                FarPlane::Finite
            )
            .is_err()
        );
        assert_eq!(p, before);
        p.try_update(
            Mat4::IDENTITY,
            viewport(),
            ScreenYAxis::Up,
            NdcDepth::ZeroToOne,
            FarPlane::Finite,
        )
        .unwrap();
        black_box(p.pick_ray(vec2(420., 280.))).unwrap();
    });
    assert_eq!(count, 0);
}
