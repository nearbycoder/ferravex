# Dependency policy

Every Rust package in this repository belongs to the Ferravex workspace. The entire resolved Cargo graph, including optional features, development dependencies and platform-specific dependencies, must contain zero external packages.

Engine systems, game rules, browser hosts, shaders and build scripts are authored in this repository. Do not vendor another engine, rename an external crate, add a package-manager runtime, or load a library from a CDN to bypass the Rust/game boundary. The website search integration below is an explicit, separately scoped exception requested by the user.

## Platform boundary

Rust's standard library, browser APIs and operating-system APIs are the platform boundary. Rust, Cargo, Python, Node for host tests, Git and container base images are development or hosting tools; their implementations are not bundled into the engine's source or browser games. Build scripts use the Python standard library. Browser hosts use standard WebAssembly, WebGL2, Canvas2D, Web Audio and WebRTC APIs directly.

Native host source binds Linux X11/GLX, Windows Win32/WGL, macOS AppKit/OpenGL and iOS UIKit/OpenGL ES through Rust FFI. Native audio uses ALSA, WinMM or AudioQueue adapters. These OS libraries and graphics/audio drivers remain platform dependencies; zero external Cargo packages does not include an operating system or driver. Release CI links and tests native Rust code on Linux, Windows and macOS and checks iOS compilation; that does not establish interactive device or audio verification. See [Platforms](docs/PLATFORMS.md). Courtyard and Orbit Breaker also provide headless native runs and PNG export.

## Enforce the boundary locally

```sh
python3 scripts/audit-owned.py
python3 -B scripts/test_audit_owned.py
cargo build --workspace --all-targets --all-features --locked --offline
```

The audit resolves the entire workspace with every feature enabled and without filtering to the current platform. It rejects registry dependencies, Git dependencies, non-workspace path packages and packages outside the repository. The web/site build entry points run it. No downloaded Cargo source is needed after installing the Rust toolchain and the requested standard-library target; native interactive builds also require the documented system libraries.

## Restored examples

The local catalog contains fifteen examples. Thirteen earlier games/labs were recovered from this repository's own history and ported to first-party renderer and physics APIs. Courtyard and Orbit Breaker remain available with their original first-party hosts. The old external adapters and loaders are not runtime dependencies of the restored games.

The recovered engine source and examples were pushed in commit 612bc13. Website changes are developed and checked locally before a separate publication step; public artifacts may reflect an earlier site revision. Historical Git commits retain previous implementations; Git history has not been rewritten. Historical test results, screenshots or recordings do not by themselves validate the recovered runtime.

## Website command menu

The documentation site uses `@qikbar/qikbar` **0.2.1**, from [nearbycoder/qikbar-next](https://github.com/nearbycoder/qikbar-next), at the user's explicit request. Its official ESM browser distribution is pinned and self-hosted in `website/static/vendor/qikbar`, with the MIT license, source/version manifest and SHA-256 hashes. Only browser modules and their local chunks are included. The build needs no npm installation, CDN access, framework, account or AI service.

This dependency powers the site's Command/Control-K menu, including generated Rustdoc pages. It is not linked into any Rust crate or loaded by the game hosts. The Cargo audit still requires zero external packages across every feature and platform. Search content is generated locally from guides, example READMEs and public Rust API documentation; search queries stay in the browser.
