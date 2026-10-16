# Visual presets and demo polish

All fifteen examples share **Controls (F2) → Devices → Visual style**. Use the
page arrows to reach the second Devices page. Authored preserves each game's
palette. Warm shifts toward ochre; Cool emphasizes teal; Ink uses a blue-grey
luminance palette. The choice saves on the current device, separately for each
game. Existing version-1/2 preferences load with Authored; version 3 adds style.

The browser accepts `?look=authored|warm|cool|ink` and
`?quality=low|medium|high`. These explicit session overrides take priority over
saved settings without overwriting them. Remove the query to use saved settings.
For windowed native games, set `FERRAVEX_LOOK` and `FERRAVEX_QUALITY` to the same
names. Courtyard and Orbit Breaker's headless exporters accept `FERRAVEX_LOOK`;
quality changes do not accelerate those CPU reference renderers.

## Rendering cost and boundaries

These are lightweight palette shader presets, not full-screen post-processing.
Retained GPU draws apply the palette after the material hook and lighting/fog.
Immediate primitives use the matching CPU transform; the two software examples
use the same palette definitions. No additional texture, render target or draw
pass is required. The original flat face normals and alpha remain intact.
Emissive sprite effects retain their authored accent colors.

`set_camera` enables world palettes. `set_default_camera` restores neutral HUD
colors. Games drawing both in screen coordinates can call
`set_world_colors(true/false)` at their world/HUD boundary. Built-in menus,
health, ammo, objectives and instructional text remain readable in every preset.
Custom shaders still support transactional editing and graphics recovery;
presets do not replace those hooks. See [materials](MATERIALS.md).

Low limits the shared browser canvas to one framebuffer pixel per CSS pixel,
disables dynamic sun shadows and selects at most two point lights. Medium/High
allow up to two pixels per CSS pixel, with the shadow/light limits in
[lighting](LIGHTING.md). Courtyard's separate host caps Medium/High at 1.5 and
also skips its entire shadow pass on Low. These caps affect rendering resolution,
not gameplay coordinates, input sensitivity, simulation ticks or network rates.
Orbit Breaker keeps its fixed-resolution CPU canvas.

## Geometry and shadows

`Mesh3d::combine` bakes valid, opaque, untextured static pieces into a bounded
mesh at load time. It transforms positions and inverse-transpose normals,
corrects reflection winding, and rejects singular transforms, bad indices,
transparent pieces and more than 65,535 vertices. Group nearby pieces so the
combined bounds remain useful for culling. Crystal Cavern uses 3×3-tile chunks
after its original vertex AO bake, removing wall faces fully hidden by solid
neighbors after baking. This reduces draw calls but can submit extra
triangles at chunk edges; measure the tradeoff for the scene.

The shared native/browser shadow depth passes use slope-scaled polygon offset
in addition to the existing normal/depth bias. Strike removes overlapping baked
shadow quads and separates container ribs from their base surfaces. Its pistol,
rifle and sniper use separate original silhouettes and muzzle offsets.
`draw_skinned_viewmodel` draws an animated foreground prop after the world using
the current perspective and lighting, with depth testing and world shadow
participation disabled. Following draws retain their camera/depth state.

Courtyard reuses its CPU framebuffer/depth/scratch storage between frames.
Orbit Breaker fills opaque rectangles by row. The shared browser retained host
reuses shadow uniform buffers and rebuilds shader-error summaries only when
material status changes. These improvements retain the existing gameplay rules.

See [performance](PERFORMANCE.md) for measured costs and open regressions.
Linux and the available browser are the measured platforms; no low-end physical
device, Windows/macOS runtime or iOS result is implied.
