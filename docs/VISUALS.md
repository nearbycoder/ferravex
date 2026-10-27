# Visual presets and demo polish

All fifteen examples share **Controls (F2) → Devices → Visual style**. Use the
page arrows to reach the second Devices page. Authored preserves each game's
palette. Warm shifts toward ochre; Cool emphasizes teal; Ink uses a blue-grey
luminance palette. The choice saves on the current device, separately for each
game. Existing version-1/2 preferences load with Authored; version 3 adds style.
Version 4 adds independent resolution, shadow and local-light preferences;
older settings retain their saved choices and use the new detail defaults.

Every browser example also has a **Details** button beside Controls. Its shared
panel pauses play and saves choices for that game on this device. Lighting,
resolution, shadows, local lights and visual style can be adjusted separately.
Controls that the example's renderer or scene cannot use are disabled. Reset
details restores these five choices without changing audio or input bindings.

The browser accepts `?look=authored|warm|cool|ink` and
`?quality=low|medium|high`. These explicit session overrides take priority over
saved settings without overwriting them. Remove the query to use saved settings.
For windowed native games, set `FERRAVEX_LOOK` and `FERRAVEX_QUALITY` to the same
names. Courtyard and Orbit Breaker's headless exporters accept `FERRAVEX_LOOK`;
quality changes do not accelerate those CPU reference renderers.

For crisp-but-modern overlays, the opt-in [modern UI kit](UI.md#modern-ui-kit)
adds a mixed-case pixel face, anti-aliased rounded shapes, gradients, soft
shadows, additive glows and eased transitions that honor reduced motion. Games
keep their current look until they adopt it.

## Mobile fullscreen and touch

Every browser game has a Fullscreen button alongside Details. It requests native
fullscreen, including the toolbar so Exit and settings remain available. On
touch devices it requests landscape for wide games, and portrait for Orbit
Breaker and Salvage Pinball. Orientation is unlocked when fullscreen ends.
Browsers may reject either request: the gallery then expands the existing iframe
to the browser viewport without restarting the run. Browser bars can remain in
this fallback. A rotation hint appears if the viewport does not match the game;
the canvas is never visually rotated away from its input coordinate system.

Exit returns to the embedded player and restores page scrolling. Escape also
exits the expanded fallback. Text selection, touch callouts, drag selection and
tap highlighting are disabled inside the game/player only; scene-editor text
fields and the documentation remain selectable. Resizing, rotating or losing
pointer capture releases held input to prevent stuck movement or braking.

Fullscreen/orientation behavior is browser-dependent. Contract tests exercise
native/fallback transitions and denied orientation requests; physical iPhone
testing is still required.

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

Browser resolution defaults to the device pixel ratio, up to 3×, independently
of lighting quality. This keeps a low-lighting game sharp on a high-density
phone. Details offers explicit 0.75×, 1×, 1.5×, 2× and 3× choices. Framebuffers
are bounded to eight megapixels and the smaller of the driver limit and 8,192
pixels per dimension. Lower resolution reduces GPU work; 3× renders nine times
as many pixels as 1× at the same CSS size. These limits do not change gameplay
coordinates, input sensitivity, simulation ticks or network rates. Orbit Breaker
keeps its fixed-resolution CPU canvas and only exposes visual style.

With shadows and lights set to Follow preset, Low disables dynamic sun shadows
and selects at most two point lights; Medium/High use the limits in
[lighting](LIGHTING.md). Separate overrides select no shadows, 512-pixel hard
shadows or 1,024-pixel soft shadows, and zero, two, four or eight local lights.
Scenes must author a sun or point lights to use those settings. Courtyard uses
the same shadow tiers. Dune Skimmer uses blob shadows only when dynamic shadows
are disabled, including when using an explicit override.

Marble Run keeps its overhead follow distance beside poles. Its open course
does not shorten the camera boom against gameplay colliders. Narrow viewports
use a two-row HUD and touch controls sized to the available play area.

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
