# Original game runtime

`ferravex-render` implements the shared runtime for thirteen recovered Ferravex games. It depends only on the original core, math and graphics crates. There is no re-export of a third-party engine or window library.

- Rust owns game futures, frame scheduling, keyboard/pointer/touch state, cameras, procedural meshes, 2D primitives, static bitmap glyph geometry, faceted directional lighting and distance fog.
- Geometry is transformed to homogeneous clip space and submitted as a contiguous triangle stream. Adjacent draws with the same depth mode share a GPU batch; transparent UI retains submission order.
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

This is an immediate triangle renderer optimized for the small low-poly examples. Vertex transforms, lighting, fog and text geometry are prepared on the CPU; meshes are not yet resident/instanced in this shared backend. It has no user shader compiler, textures, skeletal animation, realtime shadows or deferred lighting. Courtyard retains its separate original instanced WebGL2/shadow renderer. The shared native runtime currently supports Linux/X11. Native Windows, macOS and iOS window/GPU bindings, lifecycle integration and device validation remain work to do. System graphics drivers, OS libraries, browser implementations and Rust std are the platform boundary.
