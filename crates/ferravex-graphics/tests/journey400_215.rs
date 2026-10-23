use ferravex_graphics::{Camera, ClipDepth, Mat4, Vec3, vec2, vec3};
use ferravex_math::projection::{FarPlane, ProjectionError, ScreenYAxis, Viewport};

#[test]
fn camera_depth_conventions_produce_the_same_world_picking_segment() {
    let eye = vec3(0., 0., 5.);
    let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
    let viewport = Viewport {
        origin: vec2(30., 90.),
        size: vec2(600., 300.),
    };
    let mut rays = Vec::new();
    for (clip_depth, lens) in [
        (ClipDepth::ZeroToOne, Mat4::perspective_rh(1., 2., 0.5, 50.)),
        (
            ClipDepth::NegativeOneToOne,
            Mat4::perspective_rh_gl(1., 2., 0.5, 50.),
        ),
    ] {
        let camera = Camera {
            view_projection: lens * view,
            position: eye,
            clip_depth,
        };
        let projector = camera
            .prepare_projector(viewport, ScreenYAxis::Down, FarPlane::Finite)
            .unwrap();
        let anchor = projector.project(vec3(1., 0.5, 0.)).unwrap();
        assert!(anchor.inside_viewport && anchor.inside_depth);
        let restored = projector
            .unproject(anchor.screen, anchor.ndc_depth)
            .unwrap();
        assert!(restored.abs_diff_eq(vec3(1., 0.5, 0.), 0.0001));
        let ray = projector.pick_ray(vec2(330., 240.)).unwrap();
        assert!(ray.origin.abs_diff_eq(vec3(0., 0., 4.5), 0.0001));
        assert!(ray.direction.abs_diff_eq(Vec3::NEG_Z, 0.0001));
        rays.push(ray);
    }
    assert!(rays[0].far_point.abs_diff_eq(rays[1].far_point, 0.001));
}

#[test]
fn failed_preparation_retains_camera_and_explicit_far_intent() {
    let camera = Camera::perspective(vec3(0., 0., 5.), Vec3::ZERO, Vec3::Y, 1., 2., 0.5, 50.);
    let matrix = camera.view_projection;
    let viewport = Viewport {
        origin: vec2(0., 0.),
        size: vec2(600., 300.),
    };
    assert_eq!(
        camera.prepare_projector(viewport, ScreenYAxis::Up, FarPlane::Infinite),
        Err(ProjectionError::UnsupportedInfiniteFar)
    );
    assert_eq!(
        camera.prepare_projector(
            Viewport {
                size: vec2(0., 300.),
                ..viewport
            },
            ScreenYAxis::Up,
            FarPlane::Finite
        ),
        Err(ProjectionError::InvalidViewport)
    );
    assert_eq!(camera.view_projection, matrix);
    let projector = camera
        .prepare_projector(viewport, ScreenYAxis::Up, FarPlane::Finite)
        .unwrap();
    assert!(projector.project(Vec3::Y).unwrap().screen.y > 150.);
}
