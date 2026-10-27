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

Visual styles are lightweight palette presets applied per draw, independent of
the optional [post-processing](#post-processing) pipeline below. Retained GPU
draws apply the palette after the material hook and lighting/fog. Immediate
primitives use the matching CPU transform; the two software examples use the
same palette definitions. A style adds no texture, render target or draw pass.
The original flat face normals and alpha remain intact. Emissive sprite effects
retain their authored accent colors. With post-processing enabled, the palette
is applied first and the post result (bloom, grade, vignette) is applied to the
paletted world, so Details previews, every look and screenshots include it.

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

## Post-processing

Games can opt into a modern, cheap full-screen pass with
[`ferravex_render::post`](../crates/ferravex-render/src/post.rs): bloom, a soft
or filmic tonemap, exposure, power-law contrast, saturation, lift/gamma/gain
(or a single tint), a vignette, and optional film grain, chromatic aberration
and scanlines. It is off by default, so a game that has not opted in renders
exactly as before through the direct path, with no extra target or pass.

```rust,ignore
use ferravex_render::post::{self, PostFx};
// Sticky: keep it until the scene changes, like set_look/set_quality.
post::set_post_fx(PostFx::neon().vignette(0.25, 0.6));
loop {
    clear_background(INK);
    set_camera(&world_camera);
    draw_world();
    post::glow_circle(x, y, 24., ORANGE, 1.5); // additive emissive accent
    post::with_glow(|| draw_circle(bx, by, 4., CYAN)); // any primitive, additive
    set_default_camera(); // world/HUD boundary: HUD below stays crisp
    draw_hud();
    next_frame().await;
}
```

Presets: `PostFx::off()`, `subtle()`, `neon()`, `warm_film()` and `crt()`.
Builder methods (`bloom(intensity, threshold, radius)`, `bloom_knee`, `tonemap`,
`exposure`, `contrast`, `saturation`, `lift_gamma_gain`, `tint`,
`vignette(strength, softness)`, `grain`, `aberration`, `scanlines`, `auto_hud`)
adjust any preset; values are clamped and non-finite values become neutral.
`post_fx_status()` reports the effective tier, bloom levels and capped settings.

**World/HUD boundary.** World draws go to an offscreen target. The first
boundary in a frame resolves it, runs bloom and composites the graded image to
the screen; later draws (HUD, menus, the Controls editor) are drawn directly on
top, un-bloomed and un-vignetted. The boundary is the first of
`post_fx_hud_begin()`, `set_world_colors(false)` while world colors are on, or
`set_default_camera()` after a `set_camera` in the same frame — the same rule
that already separates world palettes from neutral HUD colors. Disable the two
automatic triggers with `PostFx::auto_hud(false)`. Without any boundary the whole
frame is post-processed. The boundary is a `kind 4` command in the shared
command stream; hosts that predate it skip unknown kinds.

**Preferences and accessibility.** An explicit Low quality choice (Details,
`?quality=low` or `FERRAVEX_QUALITY=low`) turns post-processing off. Medium uses
four bloom levels and High five, starting at half resolution. With no player
choice, an authored `set_quality(High)` selects High and anything else Medium.
Adaptive performance pressure (budget level 2+) caps the tier at Medium.
Reduced flash caps bloom intensity at 0.6 and disables grain and aberration;
reduced motion freezes the grain pattern. For local comparison, force a preset
with `?post=off|subtle|neon|warm-film|crt` or `FERRAVEX_POST=…` without editing
the game; these session overrides beat authored settings.

**Glow.** Additive draws accumulate light and feed the bloom naturally:
`post::glow_circle(x, y, radius, color, intensity)` draws a soft radial disc
(112 unlit triangles, `intensity` may exceed one), and `post::with_glow(|| …)`
makes any immediate primitive additive (`EffectBlend::Additive` does the same for
particles, ribbons and beams). Both work, as plain additive blending, when
post-processing is off.

**Backends.** The browser (`web/post.js`) and native hosts
(`native/gpu/post.rs`) share the GLSL in `post.rs` (staged as
`post-shader.js`) and run the same passes: an RGBA16F world target when
renderable (`EXT_color_buffer_float` in WebGL2; always on desktop GL, RGBA8 on
iOS), with up to 4× MSAA matching the default framebuffer and a blit resolve;
a dual-filter (Kawase-style) bright-pass downsample and additive tent upsample
chain at half resolution and below; and one composite pass with a static
sub-LSB dither against 8-bit banding. Missing WebGL framebuffer APIs or
extensions, shader failures and incomplete targets fall back silently to the
direct path (`canvas.dataset.postError` records why); MSAA and half-float
downgrade independently first. Targets follow resize, device pixel ratio and
adaptive render scale, are rebuilt after context restoration, and allocate
nothing per frame. Native `--screenshot`, browser player screenshots and the
GPU timer include the post passes. `canvas.dataset.post` reports
`off|medium|high`.

Measured on this machine (AMD Radeon 8060S, Mesa radeonsi, 1280×800,
Neon Swarm autoplay; mean GPU time per frame from the existing timers):

| Path | Off | Subtle | Neon |
| --- | --- | --- | --- |
| Native GL, Medium (`FERRAVEX_PERF_OUTPUT`) | 0.10 ms | 0.27 ms | 0.29 ms |
| Native GL, High | 0.10 ms | 0.28 ms | 0.31 ms |
| Chrome WebGL2/ANGLE, Medium (`?profile`) | 0.10 ms | 0.35 ms | 0.41 ms |

Post adds 8 (Medium) or 10 (High) full-screen-triangle passes and no CPU
allocations (native allocation counters were unchanged). Bright scenes with a
low threshold (Neon on a pale sky) haze heavily; choose presets per scene.

See [performance](PERFORMANCE.md) for measured costs and open regressions.
Linux and the available browser are the measured platforms; no low-end physical
device, Windows/macOS runtime or iOS result is implied.
