# Original game runtime

`ferravex-render` implements the shared runtime for thirteen recovered Ferravex games. It depends only on the original core, math, graphics and play crates. There is no re-export of a third-party engine or window library.

- Rust owns game futures, frame scheduling, keyboard/pointer/touch state, cameras, procedural meshes, 2D primitives, static bitmap glyph geometry, faceted directional lighting and distance fog.
- Immediate primitives/text use a clip-space triangle stream. Mesh3d and explicit ResidentMesh handles use persistent GPU buffers, GPU transforms/lighting/fog and instanced submission. Opaque retained draws group within a camera/material pass; transparent draws and immediate UI preserve ordering barriers.
- Linux uses original Rust FFI declarations for system X11, GLX and OpenGL. A persistent GPU program and streamed vertex buffer present the frame; pointer grabbing and GPU PNG readback support native FPS play and local validation.
- Web uses the original `web/runtime.js` host to call WebGL2 and forward DOM input through a direct Wasm ABI. Logical input pixels stay independent of framebuffer DPI. Hosts reset held input when focus is lost.

```rust,ignore
use ferravex_render::prelude::*;
fn config() -> Conf { ferravex_render::window_conf("My game") }
ferravex_render::game!(config, game);
async fn game() {
    loop {
        clear_background(BLACK);
        draw_circle(screen_width() * 0.5, screen_height() * 0.5, 32., ORANGE);
        next_frame().await;
    }
}
```

A native binary calls the library's `run()`. A Wasm library exports `ferravex_init` from the entry macro and the frame/input/geometry ABI from this crate. Set `crate-type = ["rlib", "cdylib"]` to build both.

`Demo` provides `--autoplay --smoke-frames N --screenshot path.png`. Simulation still uses `FixedClock`; an automated run supplies a reproducible 1/60-second render interval. Screenshots capture queued draws after GPU presentation, including the final frame. `?autoplay` enables each game's original pilot in the browser.

## Current limits

This runtime has both immediate and retained paths. Immediate vertex transforms, lighting/fog and text geometry are prepared on the CPU. Retained meshes keep immutable geometry on the GPU and stream instance transforms/normal matrices. See [retained rendering](../../docs/RETAINED_RENDERING.md) for storage limits, mutation invalidation, ordering and backend requirements. It supports authored vertex/fragment material hooks compiled by the GPU host, with per-draw parameters, blend modes and last-valid reload. The shared retained path supports a directional shadow map, bounded local lights and offline vertex AO. Retained GPU skinning supports four influences and up to 32 joints, with inverse-transpose normal palettes, matching shadows and conservative animated bounds. See [animation](../../docs/ANIMATION.md). General texture assets and deferred lighting remain future work. Courtyard retains its separate original instanced WebGL2/shadow renderer. The shared native runtime currently supports Linux/X11. Native Windows, macOS and iOS window/GPU bindings, lifecycle integration and device validation remain work to do. System graphics drivers, OS libraries, browser implementations and Rust std are the platform boundary.

Retained rendering requires WebGL2 or Linux OpenGL 3.3+. Draw-call and submission overhead can outweigh CPU savings for small independently authored meshes; use the local performance harness and the immediate escape hatch where appropriate.

Retained 3D draws use conservative camera-frustum culling by default. Optional distance limits, per-object hysteretic LOD and visibility counters are described in [visibility](../../docs/VISIBILITY.md). Dune Skimmer and Marble Run exercise authored LOD chains.

[Materials and hot reload](../../docs/MATERIALS.md) describes the shader contract, native file polling and optional browser editor. Default game URLs keep the normal HUD.

[Low-poly lighting](../../docs/LIGHTING.md) covers quality settings, local lights, shadow volumes, AO baking and driver GPU timing.

[Unified input](../../docs/INPUT.md) covers action contexts, buffered fixed-tick
edges, touch capture, gamepad response, local rebinding profiles and the F2
controls panel. All fifteen browser games use that core; custom renderer hosts
can drive it with `input::host_frame`. Linux controller hardware still needs
physical device verification.

[Runtime audio](../../docs/AUDIO.md) connects the shared Rust mixer to browser
AudioWorklet output and an optional Linux ALSA worker. Original surface/weapon
cues and managed engine loops are integrated across the examples; custom PCM16
WAV sounds use the same voice controls. Browser worklet queues are bounded and
reset on focus/mute changes. Windows/Apple device output remains iteration 20.
