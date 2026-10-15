# ferravex-graphics

First-party reusable geometry and a working headless 3D software renderer, depending only on Ferravex's own `ferravex-math`. No external crates, GPU libraries, image libraries, copied vendor code, windowing bindings, or unsafe code. This is the CPU reference/fallback path; it does not claim GPU acceleration, GPU resource caching, or production frame rates.

```sh
cargo run --release -p ferravex-graphics --example render_scene -- scene.png
cargo test -p ferravex-graphics
cargo clippy -p ferravex-graphics --all-targets -- -D warnings
cargo check -p ferravex-graphics --target wasm32-unknown-unknown
```

The example creates 32 instances from four cached procedural meshes and renders a lit, fogged low-poly grove to PNG. `scene.png` in this directory is an actual generated render.

## Public path

```rust
use ferravex_graphics::*;
let mut cache = MeshCache::new();
let cube = cache.insert(Mesh::cuboid(Vec3::ONE, Color::from_hex(0xc79567))).unwrap();
let items = [RenderItem::new(cube, Mat4::IDENTITY)];
let camera = Camera::perspective(
    vec3(3.0, 2.0, 4.0), Vec3::ZERO, Vec3::Y, 0.9, 1.6, 0.1, 100.0,
);
let mut renderer = SoftwareRenderer::new(640, 400).unwrap();
let stats = renderer.render(&cache, &items, camera).unwrap();
assert!(stats.shaded_pixels > 0);
let png = renderer.framebuffer.to_png();
```

- `Vertex { position, normal, color }` and `Mesh { vertices, indices }` are public portable scene data. Vertices use right-handed coordinates. Triangles have counter-clockwise exterior winding. `Mesh::cuboid`, `plane`, `sphere`, and `cylinder` construct indexed geometry; faceted shapes duplicate vertices to preserve hard normals.
- `MeshCache::insert` validates finite data, bounds, indices and resource limits. Generation-bearing `MeshId` handles survive `update`; removed slots can be reused without reviving stale handles. Invalid updates preserve the previous mesh. `stats` reports CPU storage. `instance_batches` groups transforms sharing a handle; it does not issue hardware instancing commands.
- `Frustum` extracts normalized clip planes and conservatively tests transformed AABBs. Support-radius calculation handles nonuniform scale and shear. `Camera` accepts a public view-projection matrix, camera position, and an explicit [-1,1] or [0,1] depth convention.
- `SoftwareRenderer::render` clears color/depth, culls instances, transforms cached geometry, applies inverse-transpose normals, clips triangles against all six homogeneous clip planes plus positive w, and rasterizes into an opaque RGBA/depth buffer. Attributes use perspective-correct interpolation. Depth uses screen-linear z/w. Mirrored transforms preserve front-face rendering. Invalid handles, singular/non-affine transforms, and degenerate geometry are skipped and counted.
- `RenderSettings` controls directional/ambient light, backface culling, background, and optional distance fog. `Mesh::bake_vertex_ao` accepts an application-provided occlusion function and modifies RGB once; it does not estimate scene occlusion automatically.
- `Framebuffer::width`, `height`, `pixels`, `depths`, and `pixel` expose read access. `to_png` and `to_ppm` work without filesystem access. Native-only `save_png` and `save_ppm` write files. PNG export uses original CRC32/Adler32 routines and uncompressed DEFLATE blocks; files favor simplicity over compression.

## OBJ import

`Mesh::from_obj(text, color)` supports finite XYZ positions, positive and relative negative face indices, and triangles or convex planar polygons. UV/normal records and slash indices are validated, while output intentionally uses the supplied solid color and freshly generated flat normals. Fan triangulation is used only after checking planarity and convexity. Concave, self-intersecting, degenerate, invalid-index and unsupported geometric records return an error with a line number. Object/group/smoothing/material names are metadata; the importer never follows paths or loads MTL files.

`ObjLimits` caps input bytes, positions, faces, face corners and expanded output vertices. Defaults allow 8 MiB of text, 100,000 source positions, 100,000 faces, 32 corners per face, and 750,000 output vertices. glTF, textures, materials, skeletal animation and arbitrary concave tessellation are not implemented here.

## Bounds and verification

Framebuffers are limited to 8192 pixels per side and 16,777,216 total pixels. Dimensions are immutable after construction. Clipping uses fixed arrays of 12 vertices with explicit bounds checks; raster loops are clamped to framebuffer dimensions. Scratch transformed vertices are reused between frames. Framebuffers allocate once, while explicit batch construction and OBJ import allocate owned data.

The renderer is an opaque single-sample forward rasterizer: no transparency sorting/blending, textures, shadows, MSAA, SIMD, threading, GPU pipelines, gamma conversion, or skeletal animation. Alpha <= 0 discards a fragment; all other fragments are opaque. Scene owners decide when to update meshes; there is no automatic asset hot reload. These limits are explicit extension points, not placeholder implementations.

Tests verify winding/normals of every primitive, cache lifetime and invalid updates, order-independent depth occlusion, near-plane clipping, frustum visibility, mirrored transforms, backface culling, invalid transforms, instance batches, AO baking, lighting/fog, image signatures/checksums, importer indices/limits/nonplanarity/concavity, and invalid framebuffer sizes. The committed scene PNG was decoded and visually inspected.
