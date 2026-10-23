# Platforms

## Implementation and verification

The platform code is first-party Rust. Rust std, operating-system frameworks,
graphics drivers and browser APIs form the platform boundary. There are no
external Cargo packages. Source implementation, cross-target checking and device
verification are separate milestones:

| Platform | Host source in this repository | Release checks and remaining limits |
|---|---|---|
| Web | WebGL2, DOM keyboard/pointer/touch/gamepad, AudioWorklet, context restoration | All 17 Wasm builds/contracts; actual browser input, multiplayer and context loss/recovery |
| Linux | X11/GLX/OpenGL, keyboard/pointer capture, desktop DPI, minimize/resume, ALSA | All native builds, 15 gameplay/PNG smoke runs, GPU pixel/recovery/timing and audio checks |
| Windows | Win32/WGL OpenGL 3.3 core, raw mouse, keys/buttons/wheel, per-monitor DPI, minimize/resume, WinMM audio | Windows CI: native compilation/linking, Rust tests, runnable guide examples and strict Clippy; interactive windows, hardware input and audio listening remain unverified |
| macOS | AppKit/NSOpenGL 4.1 core, Retina metrics, keyboard/mouse, focus/minimize lifecycle, AudioQueue | macOS CI: native compilation/linking, Rust tests, runnable guide examples and strict Clippy; interactive windows, hardware input and audio listening remain unverified |
| iOS | UIKit scene delegate, CADisplayLink, EAGL/OpenGL ES 3, touch/hardware keys, rotation/drawable resizing, background suspension, AudioQueue | Apple SDK compilation for device and simulator targets in CI; app launch, signing, simulator/device play and audio remain unverified |

CI compilation and headless tests do not exercise an interactive window, its driver
or device lifecycle. The Windows and Apple interactive hosts remain provisional
until those paths receive device playtests. The [release scope](STOPPING_POINT.md)
and release PR record the exact checks. Apple's [OpenGL](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/OpenGL-MacProgGuide/UpdatinganApplicationtoSupportOpenGL3/UpdatinganApplicationtoSupportOpenGL3.html)
and [OpenGL ES](https://developer.apple.com/documentation/opengles/)
APIs are deprecated; the current source uses those APIs to share the existing
renderer. A first-party Metal backend remains future work.

The thirteen shared-renderer examples use the native hosts above. Courtyard and
Orbit Breaker keep their separate browser renderers and native headless/software
paths. Other native operating systems return an explicit unsupported-host error;
portable simulation libraries remain independently usable.

## Build and run

```sh
cargo build --workspace --bins --release --offline --locked
cargo run --release -p emberline
cargo run --release -p marble-run
cargo run --release -p strike
python3 scripts/build-web.py release
python3 scripts/serve-web.py --port 8000
```

Desktop builds need their installed OS toolchain/libraries and compatible graphics
driver. Linux needs X11 and GLX/OpenGL 3.3 or newer. Windows needs a vendor OpenGL
3.3 driver and its Rust native linker/toolchain. macOS requests Apple's OpenGL
4.1 core profile. The iOS source requests OpenGL ES 3 and a UIKit scene app bundle.
Windows/Apple multisample drawables and native controller-device discovery are
not implemented yet. Linux uses its joystick provider; browser gamepads are
polled through the browser API. Embedding hosts can supply controller samples
through `input::gamepad_sample`.

The catalog provides seventeen browser entries. The fifteen shared examples use
paths such as `/neon-swarm/`, `/marble-run/` and `/strike/`; Courtyard uses `/owned/`
and Orbit Breaker uses `/owned-orbit/`. The docs site's corresponding paths begin
with `/play/`. The builder stages required host files and fetches no runtime
packages or loaders. `python3 scripts/build-web.py small` selects the size-oriented
profile; measure throughput as well as bytes before choosing it.

## Logical pixels and frame pacing

Cameras, HUDs and input use logical pixels; GPU viewports/readback use framebuffer
pixels. `screen_dpi_scale()` reports the scale. Windows follows window DPI changes
and applies the suggested size; macOS follows its backing scale; UIKit uses scene
bounds and screen scale; web caps framebuffer scale at 2. Linux reads startup
`Xft.dpi`, with `FERRAVEX_DPI_SCALE` (0.5–4) available for an explicit override.
Linux per-monitor XRandR scale transitions are not implemented.

Native windows request vsync. `FERRAVEX_MAX_FPS=120` optionally caps desktop frame
rate; there is no fixed 60 Hz sleep in the default renderer. Fixed-step smoke and
performance runs disable vsync. The performance harness pins Linux scale to 1 so
its physical framebuffer workload matches earlier baselines; this does not change
interactive desktop scaling. Results do not establish performance on unavailable
hardware.

```sh
FERRAVEX_QUALITY=low FERRAVEX_DPI_SCALE=1 FERRAVEX_MAX_FPS=120 cargo run --release -p strike
python3 scripts/smoke-games.py --frames 360
```

## Suspension and recovery

Minimize/background events release held input, stop native output queues and
suspend game-frame/GPU submission. Resume resets elapsed-frame timing and opens
fresh audio output. UIKit finishes GPU work before marking a scene inactive and
pauses its display link; it retains Rust game state across scene reconnection.
Memory warnings request a GPU rebuild on the next active frame. These UIKit paths
still require SDK/device verification.

```rust,ignore
// Keep the Rust future, mesh handles, shader descriptors and game state.
ferravex_render::request_graphics_reset();
```

Native reconstruction replaces the OS window/context and rebuilds GPU caches;
its logical size, swap policy and GPU timing frame tags survive. Window position
is not preserved. CPU geometry/material ownership survives, including the last
successfully compiled shader source if a later edit failed. The GPU must accept
that source again; otherwise rendering uses its fallback and reports the error.
Manual reconstruction and lifecycle handling do not cover every possible driver
reset or device removal.

WebGL context restoration rebuilds programs, buffers, meshes, shadows, palettes,
sprites and timer state without reinitializing the Wasm instance. Pending render
callbacks are canceled on loss, preventing competing loops after recovery. Audio
is suspended rather than permanently disposed. The optional `?graphics-tools`
panel exercises real `WEBGL_lose_context` loss/recovery when the browser exposes
that extension. `?material-tools` adds the shader editor; both are absent from the
normal game HUD. Native GPU pixel tests and real browser interaction exercise
recovery; the browser/Wasm material ABI also has a last-valid-source regression.

## Apple app bundle helper

On an Apple host with the appropriate installed SDK and Rust target:

```sh
python3 scripts/build-apple-app.py strike --platform macos
python3 scripts/build-apple-app.py strike --platform ios
python3 scripts/build-apple-app.py strike --platform ios-simulator
```

The helper packages one of the thirteen windowed demos, writes its bundle/scene
metadata, uses the installed Apple SDK/linker and optionally accepts an existing
codesign identity/provisioning profile. It does not install, upload, notarize or
provision certificates. Device signing, entitlements and installation need the
app owner's Apple development setup. The generated iOS manifest selects
`FerravexSceneDelegate`, a single `UIWindowScene`, OpenGL ES 3, orientations and
local-network permission text. The helper targets iOS 14 or newer.

For metadata review without an SDK invocation:

```sh
python3 scripts/build-apple-app.py strike --platform ios --manifest-only
```

Only that metadata-only path ran here. No iOS SDK, simulator or device work ran.

## Local checks and artifacts

```sh
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-gnu --offline --locked
cargo check --workspace --all-targets --all-features --target aarch64-apple-darwin --offline --locked
cargo test -p ferravex-render --all-features --offline --locked -- --ignored --test-threads=1
python3 scripts/report-size.py --profile release --json
```

Cross-target commands require the corresponding Rust standard-library target;
they do not link or run an app. The size report follows the catalog, separates
native/Wasm modules and shared/per-game host files, and labels missing artifacts.
Gzip is a transfer estimate. Builds and the app helper do not publish anything.
