> The new owned examples use `scripts/build-owned.py` and direct browser APIs. Their native host currently runs headless; the native windowing and GPU details below describe the earlier Macroquad adapter. See [the migration and platform limits](FROM_SCRATCH.md).

# Platform builds and verification

Ferravex separates simulation, networking, physics, and rendering so a headless
server does not need graphics, and a game can omit unused physics dimensions.
The initial renderer uses Macroquad 0.4.16; its documented backends cover
Linux, Windows, macOS, iOS, and WebAssembly. That upstream support is a build
foundation, not evidence that every Ferravex example has run on every device.

| Platform | Build path | Verification for this initial implementation |
| --- | --- | --- |
| Linux x86-64 | Native Cargo binaries | 40 Rust tests including doctests and strict native Clippy passed locally; 3D demo and FPS custom shaders rendered and FPS screenshot captured |
| Browser / WebAssembly | `wasm32-unknown-unknown`, WebGL, local JS loader | All three examples compile and render locally with clean console checks; FPS deploy/fire and 2D spawning exercised in the browser. Short-screen menu fit checked; real mobile touch devices unverified |
| Windows | Native Cargo with the MSVC toolchain | CI configured for workspace checks and tests, including real loopback UDP; not run locally |
| macOS | Native Cargo on the host architecture | CI configured for workspace checks and tests; Metal shader/runtime checks require Apple hardware |
| iOS device | `aarch64-apple-ios`, Xcode SDK | CI compile check configured; device execution, signing, installation, and touch ergonomics unverified |
| iOS simulator | `aarch64-apple-ios-sim` (Apple Silicon) | CI compile check configured; simulator execution and packaging unverified |

The CI workflow is supplied in `.github/workflows/ci.yml`. Adding it to this
repository has not executed a remote CI run. A successful `cargo check` checks
types and target dependencies; it does not prove final linking, shader
compilation, GPU behavior, installation, or a playable frame on that platform.
Android is a possible future backend integration and has no Ferravex packaging or
verification path in this initial version.

## Native desktop

Install a current stable Rust toolchain. On Linux, the renderer may require
development packages for X11, Xi, and OpenGL; on Debian/Ubuntu:

```sh
sudo apt-get install pkg-config libx11-dev libxi-dev libgl1-mesa-dev
cargo test --workspace --locked
cargo run --release --locked -p physics-2d
cargo run --release --locked -p physics-3d
cargo run --release --locked -p strike
```

The same Cargo commands are used on Windows and macOS. Audio is currently
disabled, so the framework does not require an audio development library.
Run the server with `cargo run --release -p ferravex-fps --bin ferravex-server`.
Native multiplayer uses UDP. The workspace tests include a real local UDP
socket round trip on each native CI runner.

## Web

```sh
rustup target add wasm32-unknown-unknown
sh scripts/build-web.sh release
python3 scripts/serve-web.py --port 8000
```

Open `http://127.0.0.1:8000/?demo=strike`, or select `physics-2d` / `physics-3d`
from the Examples menu. The server binds to localhost by default. Use
`--bind 0.0.0.0` deliberately to preview from another device on your network.
`--directory PATH` serves a different staged output directory.

The build script compiles all thirteen examples and writes `dist/web`. It copies
the loader from the **resolved Cargo registry package** `macroquad 0.4.16`,
along with its licenses. A narrow compatibility patch adds a missing `var`
declaration to the bundled `quad_net` plugin's `register_plugin` assignment;
the original throws a `ReferenceError` under the loader's strict mode. The
patch requires exactly one version-specific match and preserves the plugin.
It does not implement browser multiplayer. There is no CDN request or implicit
download of a newer loader. `build-info.json` records the patch, upstream loader
digest, build profile, exact staged file sizes, and SHA-256 digests of the
patched output. Run `python3 -B scripts/test_stage_web.py` for patch regressions.
`web/index.html` remains the editable source page. Its `ferravex_ui` plugin supplies
Strike's `ferravex_set_playing` import and hides the Examples menu during play;
custom host pages must register this bridge before loading Strike.
The script respects Cargo's configured target directory. Python 3 is the only
additional scripting dependency.

On Windows without a POSIX shell, run the equivalent commands:

```sh
python3 scripts/build-web.py release
python3 scripts/stage-web.py release
python3 scripts/serve-web.py
```

`sh scripts/build-web.sh small` uses size optimization and fat LTO. It replaces
the staged demo binaries; their build profile is recorded in the manifest.
This profile can reduce throughput, so compare measured timings as well as
download size before choosing it for a game.

Browser examples currently run **offline**. A browser cannot use the existing
raw UDP client. A WebRTC or WebTransport adapter, connection setup, server
integration, and network testing remain future work. The packet codec and
simulation can be shared with that adapter; compiling them for WebAssembly
does not make browser multiplayer implemented.

## iOS build and packaging skeleton

Use macOS with a full Xcode installation and the matching iOS SDK. This initial
packaging configuration selects iOS 15.0 as its deployment baseline; it is not
a claim that earlier iOS releases cannot be supported.

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo check --workspace --locked --target aarch64-apple-ios
cargo check --workspace --locked --target aarch64-apple-ios-sim
python3 scripts/package-ios.py --example physics-2d --target aarch64-apple-ios-sim --dry-run
python3 scripts/package-ios.py --example physics-2d --target aarch64-apple-ios-sim
```

The packaging helper builds one example, copies its executable, and creates
`Info.plist` under `dist/ios/<target>/<profile>/<example>.app`. It uses the
template in `scripts/ios/Info.plist`, supports `--profile small`, and accepts
`--bundle-id com.yourcompany.yourgame`. Set `IPHONEOS_DEPLOYMENT_TARGET` to
change the baseline for both compilation and bundle metadata. Intel simulator
hosts can select `x86_64-apple-ios`; that target is not covered by this CI matrix.

The result is an **unsigned bundle skeleton**. Signing, provisioning, icons,
store metadata, installation, and device/simulator execution remain separate
Apple workflows. The helper does not deploy or publish. Its dry-run path and
plist parsing can be checked on Linux, but that does not verify an iOS build.
The games use procedural or embedded resources, so no external asset folder
needs to be copied in this version. Touch input exists, but native mobile
ergonomics and lifecycle behavior still require device testing.

The packaging structure follows the [upstream Macroquad iOS guide](https://macroquad.rs/articles/ios/).
The pinned dependency's [build instructions](https://docs.rs/crate/macroquad/0.4.16/source/README.md)
describe its desktop and browser backends.

## Artifact sizes

```sh
cargo build --locked --release -p strike -p physics-2d -p physics-3d -p ferravex-fps
python3 scripts/report-size.py --profile release
python3 scripts/report-size.py --profile small --json
```

The report reads the actual native example/server executables, WebAssembly
files, and staged loader. It reports file bytes and deterministic gzip level-9
bytes separately, and labels missing artifacts instead of treating them as
zero. Use `--target TARGET` for native cross-compiled binaries. Gzip is only a
transfer estimate; it does not measure installed footprint, startup time,
memory, frame time, or total browser downloads. The development HTTP server
serves ordinary uncompressed files. Configure compression on the eventual
static host if compressed transfers are desired.
