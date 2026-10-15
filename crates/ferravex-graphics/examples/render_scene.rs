use ferravex_graphics::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ferravex-software-scene.png".into());
    let mut cache = MeshCache::new();
    let ground = cache.insert(Mesh::plane(32.0, 32.0, 8, Color::from_hex(0x73745b)))?;
    let cube = cache.insert(Mesh::cuboid(Vec3::ONE, Color::from_hex(0xc79567)))?;
    let leaf = cache.insert(Mesh::sphere(1.0, 3, 5, Color::from_hex(0x57745a)))?;
    let trunk = cache.insert(Mesh::cylinder(0.18, 1.8, 5, Color::from_hex(0x755345)))?;
    let mut items = vec![RenderItem::new(ground, Mat4::IDENTITY)];
    for (p, s) in [
        (vec3(-1.0, 1.0, 0.0), vec3(3.0, 2.0, 3.0)),
        (vec3(2.0, 0.6, 1.0), vec3(1.8, 1.2, 1.8)),
        (vec3(-2.0, 2.7, 0.0), vec3(1.0, 1.4, 1.0)),
    ] {
        items.push(RenderItem::new(
            cube,
            Mat4::from_scale_rotation_translation(s, Quat::from_rotation_y(0.2), p),
        ));
    }
    for i in 0..14 {
        let angle = i as f32 * 0.55;
        let radius = 5.0 + (i % 3) as f32;
        let p = vec3(angle.cos() * radius, 0.9, angle.sin() * radius);
        items.push(RenderItem::new(trunk, Mat4::from_translation(p)));
        items.push(RenderItem::new(
            leaf,
            Mat4::from_scale_rotation_translation(
                vec3(1.0, 1.7, 1.0),
                Quat::from_rotation_y(angle),
                p + Vec3::Y * 1.7,
            ),
        ));
    }
    let mut renderer = SoftwareRenderer::new(640, 400)?;
    renderer.settings.clear_color = Color::from_hex(0xc5c2a4);
    renderer.settings.fog = Some(Fog {
        color: Color::from_hex(0xc5c2a4),
        start: 15.0,
        end: 38.0,
    });
    let camera = Camera::perspective(
        vec3(12.0, 9.0, 14.0),
        vec3(0.0, 1.0, 0.0),
        Vec3::Y,
        0.85,
        1.6,
        0.1,
        80.0,
    );
    let stats = renderer.render(&cache, &items, camera)?;
    renderer.framebuffer.save_png(&output)?;
    println!(
        "Wrote {output}: {stats:?}; CPU mesh cache {:?}",
        cache.stats()
    );
    Ok(())
}
