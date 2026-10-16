# Low-poly lighting

The shared retained renderer supports flat normals, one directional shadow volume,
bounded local lights and vertex ambient-occlusion baking. Rust owns scene data and
the shader templates; Linux OpenGL and browser WebGL2 use the same lighting math.
Immediate primitives and text keep the existing CPU directional-light/fog path.

## Frame setup

Configure the environment after clearing and before submitting the world. There
is one environment per frame, shared across cameras. Changes during submission
apply to the whole frame; use separate frames for separate lighting environments.
Local-light selection uses the latest 3D camera eye. Returning to a 2D HUD camera
does not change that selection.

```rust,ignore
use ferravex_render::{lighting::*, *};
set_quality(Quality::Medium);
set_sun_shadow(Some(SunShadow {
    center: player_position,
    direction: Vec3::new(-0.4, 0.85, 0.3),
    extent: 30.0,
    ..Default::default()
}))?;
set_point_lights(&[PointLight {
    position: torch_position, color: ORANGE, radius: 5.0, intensity: 1.5,
}])?;
// Select the camera and use the same direction for the world's directional light.
```

`set_point_lights` accepts at most eight finite, validated lights. It selects the
nearest lights up to the quality limit, without heap allocation during selection.
Light radius has a squared finite falloff; normals remain faceted when the mesh
uses flat face normals. Local light contribution is weighted by one minus ambient,
so fully ambient/emissive materials stay unchanged. Invalid setters preserve the
previous environment. Clearing resets points and the sun, preserving quality.

| Quality | Local lights | Directional shadow |
|---|---:|---|
| Low | 2 | Disabled |
| Medium | 4 | 512 × 512, one depth comparison |
| High | 8 | 1024 × 1024, nine comparisons per shaded fragment |

Low is the library default. Strike, Dune Skimmer and Crystal Cavern choose Medium.
Use `FERRAVEX_QUALITY=low|medium|high` on native or `?quality=low|medium|high` in
the browser to override game defaults. Crystal Cavern uses local lights and baked
occlusion without a sun. Dune keeps its cheap blob shadows only on Low.

## Shadow costs and limits

The sun uses a texel-snapped orthographic depth map, slope-scaled raster offset, normal offset and depth bias.
Tune extent/depth to the playable area, center to the player/camera, bias to avoid
acne and strength for the palette. A shadow is received only when the draw's
normalized directional light matches the sun direction. Outside the volume is lit.
The 24-bit depth payload is 0.75 MiB at Medium and 3 MiB at High; actual
driver allocation can differ. No per-frame readback is required.

Opaque retained meshes cast into this volume even when outside the main camera.
Transparent/additive materials do not cast. `MaterialOptions::casts_shadows` and
`receives_shadows` opt individual materials out. `draw_shadow(&resident, model)`
submits a shadow-only mesh, used for Strike's hidden first-person body. Custom
vertex/normal hooks and parameters also run in the shadow pass; depth and color
program replacements succeed together, or keep the previous valid pair.
Fragment discard/cutout shadows, point-light shadows, cascades, baked lightmaps,
probes and deferred lighting are outside this implementation.

A failed depth-target allocation preserves a previous target where available or
renders unshadowed; the browser exposes `canvas.dataset.lightingError`, and the
native host logs the failure once per requested size. Low releases the target.
The host releases depth textures, framebuffers, programs and mesh buffers with its
context. Browser context restoration rebuilds these GPU caches; see [platforms](PLATFORMS.md).

## Bake ambient occlusion once

`ao::AoScene::new` builds an owned world-space triangle BVH from `(mesh, model)`
pairs, with at most 100,000 nondegenerate triangles. `scene.bake(&mut mesh, model,
settings)` casts deterministic cosine-hemisphere rays and darkens RGB vertex
colors without changing alpha. Defaults are eight rays per vertex, radius 2,
bias 0.01 and strength 0.35. Samples are bounded to 1–32; traversal/triangle tests
have an explicit work budget. Invalid input or exhausted budget leaves all target
colors untouched. Repeated baking compounds darkness: start with authored colors.

Bake during scene preparation, before the first resident snapshot, then call
`Mesh3d::invalidate_gpu()` if an existing snapshot must be replaced. No rays run
per rendered frame. Strike bakes its packed courtyard geometry; Crystal Cavern
bakes its static rocks. Baked colors belong to that mesh placement; instanced
copies needing different occlusion must use separate baked geometry.

## Measure the full frame

Native opt-in performance recordings use a bounded pool of asynchronous OpenGL
TIME_ELAPSED queries, tagged with their presented frame. The query covers shadow,
clear and main drawing; uploads, swap, screenshots and reporting are outside it.
Missing/late results remain null. Final synchronization happens after recording.
CPU submission includes its own driver/swap cost and is reported separately.

Browser `?profile` enables a bounded pool of
EXT_disjoint_timer_query_webgl2 queries around the same draw passes. Unsupported
extensions report `data-gpu-source="unavailable"`; disjoint periods discard
pending/displayed timings. Valid results expose `data-gpu-us` and their
`data-gpu-frame`. Ordinary play has no query pool. Browser counters include shadow
passes in draw calls and vertices; `data-shadow-draw-calls` isolates that pass.
These are driver execution measurements, including a software driver where used.
See [performance](PERFORMANCE.md) for baseline coverage and quality contracts.

Palette presets and lower-cost resolution settings: [visual presets](VISUALS.md).
