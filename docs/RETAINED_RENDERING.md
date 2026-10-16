# Retained rendering

`ferravex-render` shares one retained geometry/instance ABI between Linux OpenGL
and browser WebGL2. No renderer/window/shader library is added. Flat normals,
directional vertex lighting, fog, straight alpha and camera matrices follow the
immediate path's conventions. Both hosts use the same vertex shader body;
`build-owned.py` stages its browser module from the Rust source.

```rust,ignore
use ferravex_render::{mesh::Mesh3d, prelude::*};
let mut tree = Mesh3d::cone(1.0, 3.0, GREEN, 6);
// In the frame loop, after selecting the camera/material:
tree.draw(vec3(2.0, 0.0, 0.0), Quat::IDENTITY);
tree.draw(vec3(4.0, 0.0, 0.0), Quat::IDENTITY);
// If source geometry is edited, explicitly discard its old immutable snapshot:
tree.mesh.vertices[0].color = RED.to_rgba();
tree.invalidate_gpu();
```

`Mesh3d` lazily snapshots geometry on first draw. Custom generators construct it
with `Mesh3d::from_mesh(mesh)`. `ResidentMesh::new(&mesh)` and
`draw_resident(&resident, model)` expose the lower-level path. Cloned resident
handles share geometry; IDs never recycle. Recording retains referenced geometry
until frame completion. Native/browser hosts upload each new mesh ID once, reuse
its GPU buffer, and collect dead resources on subsequent retained/native or
browser frame submission. Window destruction releases all owned GPU resources.

The CPU snapshot expands indices into position/normal/color triangles, bounded
to 1024 live meshes and 64 MiB of retained CPU vertex storage. Original source
meshes, instance data and driver storage are additional memory. Each frame accepts
at most 65,536 instances. Invalid geometry is rejected; drawing a singular,
non-finite or non-affine transform returns false. Nonuniform scale uses the
inverse-transpose normal matrix. Mesh3d's convenience methods expect valid authored
geometry; applications loading data should handle `ResidentMesh::new` errors.

Opaque depth-tested draws can group by mesh within an uninterrupted camera/light
pass. Grouping may change equal-depth tie ordering. Transparent meshes and all
immediate commands stop reordering; compatible adjacent transparent instances
keep their original instance order. Use `Mesh3d::draw_immediate` or
`draw_mesh_transformed` for strict ordering, procedural frame geometry or cases
where small independent draws cost more than CPU preparation. Text/HUDs and 2D
primitives continue through their original immediate path.

The desktop retained path requires OpenGL 3.3+ (Apple uses a 4.1 core profile); the browser path requires WebGL2. The iOS source uses OpenGL ES 3. Platform implementation and device verification are tracked separately in [Platforms](PLATFORMS.md).
Custom vertex/fragment materials, blending and transactional reload are described
in [materials](MATERIALS.md); [visibility/LOD](VISIBILITY.md) is also implemented.
Directional shadows, local lights and vertex AO are described in [lighting](LIGHTING.md).
[GPU skinning](ANIMATION.md) adds immutable four-influence weights and bounded per-frame position/normal palettes, matching color/shadow passes and conservative animated bounds. Static vertex/instance layouts remain unchanged. General texture assets and indirect draws remain separate work. Courtyard keeps its separate
renderer. Native CPU savings and driver submission cost must be measured together:
more resident draw calls can offset savings in small scenes, especially on a
software GPU. This is not a blanket frame-rate improvement claim.

Checks include native instanced readback, shared mesh upload reuse, depth and HUD
ordering, normal matrix/layout, invalid transforms, resource lifetimes, instance
regrouping, the native catalog screenshot harness, and actual browser games.
The local performance harness records static mesh uploads plus instance/immediate
uploads, plus skin-palette uniform bytes for each actual color/shadow program. Browser canvas data attributes expose resident mesh, draw, vertex and
upload counters for inspection.


Native reconstruction and browser context restoration rebuild resident meshes,
instance/skin buffers, shadows and custom materials from CPU ownership. Native
reconstruction keeps swap policy and timer frame tags. Game code retains its
handles and resubmits the next frame; mutable/transient draw data is not a saved
GPU image. The shared native pixel tests cover skin/shadow and shader recovery.
