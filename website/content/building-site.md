# Build the documentation site

The Ferravex site is generated from repository Markdown, crate READMEs and the first-party game catalog. Python's standard library builds the pages; the browser uses original CSS and JavaScript.

## Build everything

```sh
python3 scripts/build-site.py
python3 -B scripts/test_site.py
python3 scripts/serve-site.py --port 8080
```

The build audits the entire Cargo workspace, compiles both browser games, stages their original hosts, generates first-party Rust API documentation and writes dist/site. Web, API and site output are rebuilt from clean directories so removed artifacts cannot survive. No external packages or runtime loaders are fetched.

For content-only changes after a full build:

```sh
python3 scripts/build-site.py --skip-build
```

This requires matching first-party game and API artifacts and still runs the dependency audit.

## Deploy to Railway

The Dockerfile builds WebAssembly and API docs in a Rust builder, then copies only the generated site and original static server into the runtime image. Railway supplies PORT; the host listens on all interfaces and exposes /healthz. Pushes to main deploy automatically.

The custom domain stays registered with Vercel; its DNS routes to Railway. The repository contains no deployment credentials. Rust/Python toolchains and container base images are hosting tools, not game runtime dependencies.

## Add an example

Add a first-party workspace crate and browser host, then put its id, host directory, title, controls, description, source and screenshot in examples/catalog.json. The Wasm file is the package name with hyphens replaced by underscores. The catalog drives staging, documentation pages and size reports. Keep every Cargo dependency inside the workspace and original source tree.

Example pages instantiate the game only after Play is selected. The gallery does not download or run both game modules automatically.

## Update documentation

Edit the relevant Markdown or crate README. Search, navigation and sitemap pages are regenerated. Keep support claims aligned with the tests, implementation status and dependency policy.
