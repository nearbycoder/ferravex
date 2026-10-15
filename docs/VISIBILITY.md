# Visibility and level of detail

Retained 3D draws now use cached local bounds and the current camera frustum.
The support-plane test conservatively handles rotation, nonuniform scale, mirrors
and shear. Objects intersecting a clipping plane remain submitted. Immediate
primitives/text are unchanged; the default 2D camera does not apply a 3D frustum.
This is geometric visibility, not occlusion culling: objects behind walls may
still be submitted.

```rust,ignore
use ferravex_render::{set_visibility, visibility::VisibilitySettings};
assert!(set_visibility(VisibilitySettings {
    frustum_culling: true,
    max_distance: Some(200.0),
}));
```

Distance is measured to the nearest point on a conservative transformed world
AABB, so large nearby terrain is not discarded because its center is far away.
Distance filtering is disabled by default. Negative/nonfinite distance settings
are rejected without changing the current policy. Custom camera implementations
can provide their eye position through the Camera trait's optional position
method; without it, distance filtering and LOD distance selection are unavailable.

Authored LOD chains reuse resident mesh handles:

```rust,ignore
use ferravex_render::{draw_lod, mesh::Mesh3d, prelude::*, visibility::{LodChain,LodState}};
let high = Mesh3d::sphere(1.0, GRAY, 12, 8);
let low = Mesh3d::sphere(1.0, GRAY, 4, 3);
let chain = LodChain::new(vec![high.resident().clone(), low.resident().clone()], vec![40.0], 0.12)?;
let mut state = LodState::default(); // One state per object, retained between frames.
// During drawing, after selecting a Camera3D:
draw_lod(&chain, Mat4::IDENTITY, &mut state);
```

Chains accept 1..=16 levels, strictly increasing positive distances and a
hysteresis ratio from zero through 0.5. The first selection uses the current
distance directly. Subsequent selections require crossing the outward/inward
threshold plus/minus hysteresis. Camera teleports can cross multiple levels in
one frame. Visibility uses the union of level bounds, preventing a smaller
selected level from incorrectly hiding another level's silhouette. With no eye
position, the highest-detail level is used.

The engine does not simplify arbitrary meshes automatically or cross-fade levels.
Authors supply coarse geometry and keep meaningful silhouettes. Dune Skimmer uses
three rock levels; Marble Run uses two levels for distant island decoration.
Physics geometry and game logic are unchanged. Existing Mesh3d/Strike retained
draws automatically receive frustum filtering.

`visibility_stats()` reports per-frame tested, visible, frustum-culled and
distance-culled objects. Counts reset at frame start or clear_background; visible
means passing the visibility test, before instance-budget validation. Browser
canvas `data-visible` and `data-culled` expose the totals without adding HUD noise.
Submitted vertices/draw calls and CPU costs remain available through the local
performance harness. Spatial trees, occlusion queries and automatic mesh
simplification are separate future work.
