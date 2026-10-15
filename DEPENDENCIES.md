# Dependency policy

Every Rust package in this repository belongs to the Ferravex workspace. The entire resolved Cargo graph, including optional features, development dependencies and platform-specific dependencies, must contain zero external packages.

Engine systems, game rules, browser hosts, shaders, website code and build scripts are authored in this repository. Do not vendor another engine, rename an external crate, add a package-manager runtime, or load a library from a CDN to bypass this boundary.

## Platform boundary

Rust's standard library, browser APIs and operating-system APIs are the platform boundary. Rust, Cargo, Python, Node for host tests, Git, CI actions and container base images are development or hosting tools; their implementations are not bundled into the engine's source or browser games. Build scripts use the Python standard library. Browser hosts use standard WebAssembly, WebGL2, Canvas2D, Web Audio and WebRTC APIs directly.

Native window, GPU and audio bindings remain future work. Native examples currently run headless and export frames. There is no optional external rendering or physics backend.

## Enforce the boundary

```sh
python3 scripts/audit-owned.py
python3 -B scripts/test_audit_owned.py
cargo build --workspace --all-targets --all-features --locked --offline
```

The audit resolves the entire workspace with every feature enabled and without filtering to the current platform. It rejects registry dependencies, Git dependencies, non-workspace path packages and packages outside the repository. CI and both web/site build entry points run it. A fresh Cargo cache is sufficient to build after installing the Rust toolchain and requested standard-library target.

The published catalog contains Courtyard and Orbit Breaker. Adapter-dependent games, their generated documentation, bundled loaders, screenshots and recordings have been removed from the current source tree and deployment. Historical Git commits retain the previous implementation; Git history has not been rewritten.
