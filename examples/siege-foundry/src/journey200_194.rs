//! Presentation-only framing regressions; no game rules or outcomes are set.
use super::*;

fn inside(camera: &RangeCamera, w: f32, h: f32, point: Vec3) {
    let p = camera.matrix.project_point3(point);
    let x = (p.x + 1.0) * w * 0.5;
    let y = (1.0 - p.y) * h * 0.5;
    let scene = HudLayout::new(w, h).scene;
    assert!(
        x >= scene.x - 0.01
            && x <= scene.x + scene.w + 0.01
            && y >= scene.y - 0.01
            && y <= scene.y + scene.h + 0.01,
        "{w}x{h}: {point:?} projects to ({x}, {y}) outside {scene:?}"
    );
    assert!((-1.0..=1.0).contains(&p.z));
}

#[test]
fn initial_targets_cannon_and_support_arcs_fit_the_unobscured_scene() {
    for (w, h) in [(1280., 608.), (390., 652.), (320., 448.), (700., 448.)] {
        let camera = RangeCamera::new(w, h);
        let mut range = Range::new();
        for block in &range.blocks {
            for x in [-0.6, 0.6] {
                for y in [-0.6, 0.6] {
                    for z in [-0.6, 0.6] {
                        inside(&camera, w, h, block.home + vec3(x, y, z));
                    }
                }
            }
        }
        // Wheels/chassis and the muzzle at both legal extremes and center aim.
        for x in [-1.2, 1.2] {
            for y in [0., 1.3] {
                for z in [14., 16.] {
                    inside(&camera, w, h, vec3(x, y, z));
                }
            }
        }
        for yaw in [-0.55_f32, 0., 0.55] {
            for elevation in [0.025_f32, 0.13, 0.65] {
                range.yaw = yaw;
                range.elevation = elevation;
                let muzzle = PIVOT + range.direction() * 1.6;
                for offset in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
                    inside(&camera, w, h, muzzle + offset * 0.34);
                }
            }
        }
        // The actual collision-truncated dotted arcs to each authored support.
        for x in [-4., 0., 4.] {
            range.aim_at(vec3(x, 1.8, -4.));
            range.refresh_preview(false);
            assert!(range.preview_hit.is_some());
            for point in &range.preview {
                inside(&camera, w, h, *point);
            }
        }
    }
}

#[test]
fn fitting_keeps_eye_depth_and_uniform_pixel_scale() {
    let base = Camera3D {
        position: vec3(23., 18., 28.),
        target: vec3(0., 1.5, 3.5),
        fovy: 43.0_f32.to_radians(),
        z_near: 0.1,
        z_far: 130.,
        ..Default::default()
    };
    for (w, h) in [(1280., 608.), (390., 652.), (320., 448.)] {
        let fit = RangeCamera::new(w, h);
        assert_eq!(fit.position(), Some(base.position));
        assert!(fit.depth());
        let original = base.matrix(w, h);
        for point in [
            PIVOT,
            vec3(-4., 4.2, -4.),
            vec3(4., 0., -4.),
            vec3(0., 15., -20.),
        ] {
            let before = original * point.extend(1.);
            let after = fit.matrix * point.extend(1.);
            assert_eq!(before.z, after.z);
            assert_eq!(before.w, after.w);
        }
        // A screen-space unit displacement on either axis must scale equally.
        let transform = fit.matrix * original.inverse();
        let dx = transform * vec4(1., 0., 0., 0.);
        let dy = transform * vec4(0., 1., 0., 0.);
        assert!((dx.x - dy.y).abs() < 0.0001);
        assert!(dx.y.abs() < 0.0001 && dy.x.abs() < 0.0001);
    }
}
