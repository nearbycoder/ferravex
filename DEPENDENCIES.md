# Dependency policy

Every Rust package in this repository belongs to the Ferravex workspace. The entire resolved Cargo graph, including optional features, development dependencies and platform-specific dependencies, must contain zero external packages.

Engine systems, game rules, browser hosts, shaders, website code and build scripts are authored in this repository. Do not vendor another engine, rename an external crate, add a package-manager runtime, or load a library from a CDN to bypass this boundary.

## Platform boundary

Rust's standard library, browser APIs and operating-system APIs are the platform boundary. Rust, Cargo, Python, Node for host tests, Git and container base images are development or hosting tools; their implementations are not bundled into the engine's source or browser games. Build scripts use the Python standard library. Browser hosts use standard WebAssembly, WebGL2, Canvas2D, Web Audio and WebRTC APIs directly.

The original Linux window/GPU host binds system X11 and GLX/OpenGL through Rust FFI. These system libraries and the graphics driver remain platform dependencies; zero external Cargo packages does not mean an executable includes its own operating system or GPU driver. Native Windows, macOS and iOS window/GPU hosts and native audio sinks are not implemented. Courtyard and Orbit Breaker also provide headless native runs and PNG export.

## Enforce the boundary locally

```sh
python3 scripts/audit-owned.py
python3 -B scripts/test_audit_owned.py
cargo build --workspace --all-targets --all-features --locked --offline
```

The audit resolves the entire workspace with every feature enabled and without filtering to the current platform. It rejects registry dependencies, Git dependencies, non-workspace path packages and packages outside the repository. The web/site build entry points run it. No downloaded Cargo source is needed after installing the Rust toolchain and the requested standard-library target; native interactive builds also require the documented system libraries.

## Restored examples

The local catalog contains fifteen examples. Thirteen earlier games/labs were recovered from this repository's own history and ported to first-party renderer and physics APIs. Courtyard and Orbit Breaker remain available with their original first-party hosts. The old external adapters and loaders are not runtime dependencies of the restored games.

This recovery is local and has not been pushed or deployed. Public artifacts may still reflect an earlier catalog. Historical Git commits retain previous implementations; Git history has not been rewritten. Historical test results, screenshots or recordings do not by themselves validate the recovered runtime.
