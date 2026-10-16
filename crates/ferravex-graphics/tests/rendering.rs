use ferravex_graphics::*;
const RED: Color = Color::new(1.0, 0.0, 0.0, 1.0);
const BLUE: Color = Color::new(0.0, 0.0, 1.0, 1.0);
fn triangle(depth: f32, color: Color) -> Mesh {
    Mesh {
        vertices: vec![
            Vertex::new(vec3(-0.8, -0.8, depth), Vec3::Z, color),
            Vertex::new(vec3(0.8, -0.8, depth), Vec3::Z, color),
            Vertex::new(vec3(0.0, 0.8, depth), Vec3::Z, color),
        ],
        indices: vec![0, 1, 2],
    }
}
fn camera() -> Camera {
    Camera {
        view_projection: Mat4::IDENTITY,
        position: Vec3::ZERO,
        clip_depth: ClipDepth::ZeroToOne,
    }
}
fn renderer() -> SoftwareRenderer {
    let mut r = SoftwareRenderer::new(64, 64).unwrap();
    r.settings.ambient = 1.0;
    r.settings.diffuse = 0.0;
    r.settings.clear_color = Color::new(0.0, 0.0, 0.0, 1.0);
    r
}

#[test]
fn cached_handles_survive_updates_and_reject_stale_generations() {
    let mut cache = MeshCache::new();
    let first = cache.insert(triangle(0.5, RED)).unwrap();
    assert!(cache.get(first).is_some());
    assert_eq!(cache.stats().meshes, 1);
    let mut invalid = triangle(0.2, BLUE);
    invalid.indices[0] = 99;
    assert_eq!(
        cache.update(first, invalid),
        Err(MeshError::IndexOutOfBounds)
    );
    assert_eq!(cache.get(first).unwrap().vertices[0].color, RED);
    assert!(cache.remove(first).is_some());
    assert!(cache.get(first).is_none());
    let second = cache.insert(triangle(0.2, BLUE)).unwrap();
    assert_eq!(second.index, first.index);
    assert_ne!(second.generation, first.generation);
    assert!(cache.remove(first).is_none());
    assert!(cache.get(second).is_some());
    assert_eq!(cache.stats().allocated_slots, 1);
}
#[test]
fn primitive_winding_and_flat_normals_are_consistent() {
    for mesh in [
        Mesh::cuboid(Vec3::ONE, RED),
        Mesh::sphere(1.0, 8, 12, RED),
        Mesh::cylinder(1.0, 2.0, 9, RED),
        Mesh::plane(4.0, 3.0, 5, RED),
    ] {
        mesh.validate().unwrap();
        for indices in mesh.indices.as_chunks::<3>().0 {
            let a = mesh.vertices[indices[0] as usize];
            let b = mesh.vertices[indices[1] as usize];
            let c = mesh.vertices[indices[2] as usize];
            let n = (b.position - a.position)
                .cross(c.position - a.position)
                .normalize();
            assert!(n.dot(a.normal) > 0.999, "normal {:?} vs {:?}", n, a.normal);
            assert!(a.normal.abs_diff_eq(b.normal, 1.0e-5));
            assert!(a.normal.abs_diff_eq(c.normal, 1.0e-5));
        }
    }
}
#[test]
fn z_buffer_produces_order_independent_opaque_occlusion() {
    let mut cache = MeshCache::new();
    let far = cache.insert(triangle(0.8, BLUE)).unwrap();
    let near = cache.insert(triangle(0.2, RED)).unwrap();
    let mut r = renderer();
    for pair in [[far, near], [near, far]] {
        let stats = r
            .render(
                &cache,
                &pair.map(|mesh| RenderItem::new(mesh, Mat4::IDENTITY)),
                camera(),
            )
            .unwrap();
        assert_eq!(r.framebuffer.pixel(32, 32), Some([255, 0, 0, 255]));
        assert!(stats.shaded_pixels > 500);
        assert!((r.framebuffer.depths()[32 * 64 + 32] - 0.2).abs() < 1.0e-6);
    }
}
#[test]
fn frustum_culls_far_objects_and_keeps_intersecting_bounds() {
    let mut cache = MeshCache::new();
    let id = cache.insert(triangle(0.5, RED)).unwrap();
    let mut r = renderer();
    let items = [
        RenderItem::new(id, Mat4::IDENTITY),
        RenderItem::new(id, Mat4::from_translation(Vec3::X * 100.0)),
        RenderItem::new(id, Mat4::from_translation(Vec3::X * 1.0)),
    ];
    let stats = r.render(&cache, &items, camera()).unwrap();
    assert_eq!(stats.culled_instances, 1);
    assert_eq!(stats.visible_instances, 2);
    let f = camera().frustum().unwrap();
    assert!(f.contains_point(vec3(0.0, 0.0, 0.5)));
    assert!(!f.contains_point(vec3(0.0, 0.0, -0.5)));
}
#[test]
fn clipping_crosses_near_plane_without_projection_explosion() {
    let mut cache = MeshCache::new();
    let mut mesh = triangle(-1.0, RED);
    mesh.vertices[2].position = vec3(0.0, 0.02, -0.02);
    let id = cache.insert(mesh).unwrap();
    let camera = Camera::perspective(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y, 1.2, 1.0, 0.1, 10.0);
    let mut r = renderer();
    let stats = r
        .render(&cache, &[RenderItem::new(id, Mat4::IDENTITY)], camera)
        .unwrap();
    assert!(stats.clipped_triangles >= 2);
    assert!(stats.shaded_pixels > 0);
    assert!(stats.shaded_pixels <= 64 * 64 * stats.clipped_triangles);
    assert!(
        r.framebuffer
            .depths()
            .iter()
            .all(|d| d.is_infinite() || (0.0..=1.0).contains(d))
    );
}
#[test]
fn winding_culling_and_mirrored_transforms_are_explicit() {
    let mut cache = MeshCache::new();
    let mut mesh = triangle(0.5, RED);
    mesh.indices.swap(0, 1);
    let backwards = cache.insert(mesh).unwrap();
    let forwards = cache.insert(triangle(0.5, RED)).unwrap();
    let mut r = renderer();
    assert_eq!(
        r.render(
            &cache,
            &[RenderItem::new(backwards, Mat4::IDENTITY)],
            camera()
        )
        .unwrap()
        .shaded_pixels,
        0
    );
    assert!(
        r.render(
            &cache,
            &[RenderItem::new(
                forwards,
                Mat4::from_scale(vec3(-1.0, 1.0, 1.0))
            )],
            camera()
        )
        .unwrap()
        .shaded_pixels
            > 0
    );
    r.settings.cull_backfaces = false;
    assert!(
        r.render(
            &cache,
            &[RenderItem::new(backwards, Mat4::IDENTITY)],
            camera()
        )
        .unwrap()
        .shaded_pixels
            > 0
    );
}
#[test]
fn stale_handles_and_singular_instances_are_skipped() {
    let mut cache = MeshCache::new();
    let id = cache.insert(triangle(0.5, RED)).unwrap();
    let stale = MeshId {
        index: id.index,
        generation: 99,
    };
    let mut r = renderer();
    let stats = r
        .render(
            &cache,
            &[
                RenderItem::new(stale, Mat4::IDENTITY),
                RenderItem::new(id, Mat4::ZERO),
            ],
            camera(),
        )
        .unwrap();
    assert_eq!(stats.invalid_instances, 2);
    assert!(r.framebuffer.depths().iter().all(|d| d.is_infinite()));
}
#[test]
fn batching_reuses_geometry_and_ao_only_changes_color() {
    let mut cache = MeshCache::new();
    let mut mesh = Mesh::cuboid(Vec3::ONE, RED);
    let before = mesh.vertices[0];
    mesh.bake_vertex_ao(|_, _| 0.5);
    assert_eq!(mesh.vertices[0].position, before.position);
    assert_eq!(mesh.vertices[0].normal, before.normal);
    assert_eq!(mesh.vertices[0].color.r, 0.5);
    let a = cache.insert(mesh).unwrap();
    let b = cache.insert(triangle(0.5, BLUE)).unwrap();
    let batches = instance_batches(&[
        RenderItem::new(a, Mat4::IDENTITY),
        RenderItem::new(b, Mat4::IDENTITY),
        RenderItem::new(a, Mat4::IDENTITY),
    ]);
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].transforms.len(), 2);
    assert_eq!(cache.stats().meshes, 2);
}
#[test]
fn fog_and_light_change_observed_pixels() {
    let mut cache = MeshCache::new();
    let id = cache.insert(triangle(0.5, RED)).unwrap();
    let mut r = renderer();
    r.settings.fog = Some(Fog {
        color: BLUE,
        start: 0.0,
        end: 0.1,
    });
    r.render(&cache, &[RenderItem::new(id, Mat4::IDENTITY)], camera())
        .unwrap();
    assert_eq!(r.framebuffer.pixel(32, 32), Some([0, 0, 255, 255]));
    r.settings.fog = None;
    r.settings.ambient = 0.0;
    r.settings.diffuse = 1.0;
    r.settings.light_direction = Vec3::NEG_Z;
    r.render(&cache, &[RenderItem::new(id, Mat4::IDENTITY)], camera())
        .unwrap();
    assert_eq!(r.framebuffer.pixel(32, 32), Some([0, 0, 0, 255]));
}
#[test]
fn png_and_ppm_headers_and_invalid_dimensions() {
    let frame = Framebuffer::new(3, 2).unwrap();
    let png = frame.to_png();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[16..20], &3_u32.to_be_bytes());
    assert_eq!(&png[20..24], &2_u32.to_be_bytes());
    assert!(frame.to_ppm().starts_with(b"P6\n3 2\n255\n"));
    assert!(Framebuffer::new(0, 5).is_err());
    assert!(Framebuffer::new(u32::MAX, 1).is_err());
}

#[test]
fn perspective_color_and_fog_match_analytic_samples() {
    // Different depths make affine color interpolation observably incorrect.
    let positions = [
        vec3(-0.8, -0.8, -1.),
        vec3(1.6, -1.6, -2.),
        vec3(0., 3.2, -4.),
    ];
    let mut cache = MeshCache::new();
    let mesh = cache
        .insert(Mesh {
            vertices: positions
                .into_iter()
                .zip([RED, Color::new(0., 1., 0., 1.), BLUE])
                .map(|(position, color)| Vertex::new(position, Vec3::Z, color))
                .collect(),
            indices: vec![0, 1, 2],
        })
        .unwrap();
    for clip_depth in [ClipDepth::ZeroToOne, ClipDepth::NegativeOneToOne] {
        let projection = if clip_depth == ClipDepth::ZeroToOne {
            Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1., 0.1, 10.)
        } else {
            Mat4::perspective_rh_gl(std::f32::consts::FRAC_PI_2, 1., 0.1, 10.)
        };
        let camera = Camera {
            view_projection: projection,
            position: Vec3::ZERO,
            clip_depth,
        };
        for fog in [
            None,
            Some(Fog {
                color: Color::new(0.2, 0.3, 0.4, 1.),
                start: 0.3,
                end: 6.5,
            }),
        ] {
            let mut r = renderer();
            r.settings.fog = fog;
            r.render(&cache, &[RenderItem::new(mesh, Mat4::IDENTITY)], camera)
                .unwrap();
            for (x, y) in [(32, 32), (24, 40), (40, 44), (30, 20)] {
                // Solve the known projected triangle in f64, independent of the
                // rasterizer's f32 edge equations and reciprocal implementation.
                let nx = (f64::from(x) + 0.5) / 32. - 1.;
                let ny = 1. - (f64::from(y) + 0.5) / 32.;
                let top = (ny + 0.8) / 1.6;
                let right = (nx + 0.8 - 0.8 * top) / 1.6;
                let left = 1. - top - right;
                let mut weights = [left, right / 2., top / 4.];
                let sum = weights.iter().sum::<f64>();
                for w in &mut weights {
                    *w /= sum;
                }
                let mut expected = weights.map(|w| w as f32);
                if let Some(fog) = fog {
                    let position = positions[0] * weights[0] as f32
                        + positions[1] * weights[1] as f32
                        + positions[2] * weights[2] as f32;
                    let amount =
                        ((position.length() - fog.start) / (fog.end - fog.start)).clamp(0., 1.);
                    for (value, fog) in expected.iter_mut().zip(fog.color.to_array()) {
                        *value += (fog - *value) * amount;
                    }
                }
                let expected = Color::new(expected[0], expected[1], expected[2], 1.).to_rgba();
                let actual = r.framebuffer.pixel(x, y).unwrap();
                for (actual, expected) in actual.into_iter().zip(expected) {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "pixel {x},{y}: {actual} != {expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn flat_fog_free_surfaces_preserve_transparency_depth_and_color() {
    let mut cache = MeshCache::new();
    let solid = cache.insert(triangle(0.5, RED)).unwrap();
    let transparent = cache
        .insert(triangle(0.2, Color::new(0., 1., 0., 0.)))
        .unwrap();
    let mut r = renderer();
    // Every point is inside the fog start sphere, so the exact result is red.
    r.settings.fog = Some(Fog {
        color: BLUE,
        start: 2.,
        end: 4.,
    });
    r.render(
        &cache,
        &[solid, transparent].map(|mesh| RenderItem::new(mesh, Mat4::IDENTITY)),
        camera(),
    )
    .unwrap();
    assert_eq!(r.framebuffer.pixel(32, 32), Some([255, 0, 0, 255]));
    assert!((r.framebuffer.depths()[32 * 64 + 32] - 0.5).abs() < 1.0e-6);
    let translucent = cache
        .insert(triangle(0.2, Color::new(0., 1., 0., 0.5)))
        .unwrap();
    r.render(
        &cache,
        &[RenderItem::new(translucent, Mat4::IDENTITY)],
        camera(),
    )
    .unwrap();
    assert_eq!(r.framebuffer.pixel(32, 32), Some([0, 255, 0, 255]));
    // Outside vertices with an inside center must still use per-pixel distance.
    r.settings.fog = Some(Fog {
        color: BLUE,
        start: 0.55,
        end: 0.6,
    });
    r.render(&cache, &[RenderItem::new(solid, Mat4::IDENTITY)], camera())
        .unwrap();
    assert_eq!(r.framebuffer.pixel(32, 32), Some([255, 0, 0, 255]));
    assert_eq!(r.framebuffer.pixel(32, 50), Some([0, 0, 255, 255]));
}
