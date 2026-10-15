# Build the documentation site locally

The Ferravex site is generated from repository Markdown, crate READMEs and the first-party example catalog. Python's standard library builds the pages; the browser uses original CSS and JavaScript. The current recovery is local work: it has not been pushed or deployed, and building this site does not publish it.

## Build and inspect

```sh
python3 scripts/build-site.py
python3 -B scripts/test_site.py
python3 scripts/serve-site.py --port 8080
```

Open http://127.0.0.1:8080/. The build audits the workspace, compiles the fifteen catalog examples for WebAssembly, stages their original hosts and shared runtime, generates first-party Rust API documentation and writes `dist/site`. Web, API and site output are rebuilt from clean directories so stale adapters and loaders cannot survive. No external runtime packages are fetched.

For content-only changes after a complete local build:

```sh
python3 scripts/build-site.py --skip-build
```

This requires matching game and API artifacts and still runs the dependency audit. A successful content build does not establish gameplay correctness, browser input support or device performance; use the [example validation guide](/docs/validation/) separately.

## Hosting configuration

The repository's Dockerfile and static server remain available for a future publishing step. The image builds WebAssembly/API documentation, then serves generated files with the original Python server. It accepts `PORT` and exposes `/healthz`.

No push, deployment, DNS change or remote CI run is part of the recovery workflow. Existing hosting configuration or public URLs do not indicate that the restored catalog has been published. Toolchains and container base images are hosting tools, not bundled game runtime dependencies.

## Add an example

Add a first-party workspace crate and a browser host, then record its id, host directory, title, controls, description, source and screenshot in `examples/catalog.json`. The Wasm file uses the package name with hyphens replaced by underscores. The catalog drives staging, documentation pages and size reports. Keep every Cargo dependency within the workspace and original source tree.

The restored games share `web/runtime.js`; Courtyard and Orbit Breaker keep their specialized hosts. Example pages instantiate only the selected game after Play is pressed. The gallery does not download or run all fifteen modules automatically.

## Update documentation

Edit the relevant Markdown or crate README. Search, navigation and sitemap pages are regenerated. Keep support claims aligned with implementation and actual local test reports. Do not describe historical screenshots, measurements or external-backend test runs as evidence for the recovered runtime.
