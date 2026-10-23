# Build the documentation site locally

The Ferravex site is generated from repository Markdown, crate READMEs and the first-party example catalog. Python's standard library builds the pages; the browser uses Ferravex CSS/JavaScript plus the explicitly requested, self-hosted Qikbar command menu. The current source stopping point and verification limits are recorded in the [release status](/docs/release-status/). Building this site creates local artifacts; publishing remains a separate step.

## Build and inspect

```sh
python3 scripts/build-site.py
python3 -B scripts/test_site.py
python3 scripts/serve-site.py --port 8080
```

Open http://127.0.0.1:8080/. The build audits the workspace, compiles the seventeen catalog examples for WebAssembly, stages their original hosts and shared runtime, generates first-party Rust API documentation and writes `dist/site`. Web, API and site output are rebuilt from clean directories so stale adapters and loaders cannot survive. No external runtime packages are fetched.

For content-only changes after a complete local build:

```sh
python3 scripts/build-site.py --skip-build
```

This requires matching game and API artifacts and still runs the dependency audit. A successful content build does not establish gameplay correctness, browser input support or device performance; use the [example validation guide](/docs/validation/) separately.

## Hosting configuration

The repository's Dockerfile and static server remain available for a future publishing step. The image builds WebAssembly/API documentation, then serves generated files with the original Python server. It accepts `PORT` and exposes `/healthz`.

The production service builds this Dockerfile from the merged main branch. Verify its exact deployment reaches SUCCESS, then check /healthz and the served game manifest before calling a release deployed. Existing hosting configuration or a local build alone does not establish publication. Toolchains and container base images are hosting tools, not bundled game runtime dependencies.

## Add an example

Add a first-party workspace crate and a browser host, then record its id, host directory, title, controls, description, source and screenshot in `examples/catalog.json`. The Wasm file uses the package name with hyphens replaced by underscores. The catalog drives staging, documentation pages and size reports. Keep every Cargo dependency within the workspace and original source tree.

The restored games share `web/runtime.js`; Courtyard and Orbit Breaker keep their specialized hosts. Example pages instantiate only the selected game after Play is pressed. The gallery does not download or run all seventeen modules automatically.

## Update documentation

Edit the relevant Markdown or crate README. Search, navigation and sitemap pages are regenerated. Keep support claims aligned with implementation and actual local test reports. Do not describe historical screenshots, measurements or external-backend test runs as evidence for the recovered runtime.

## Search the whole site

`Command K` on macOS or `Control K` on other systems opens Qikbar. The visible header trigger also works on touch devices. The shortcut stays available while an embedded example game has keyboard focus. Search covers whole guides, individual guide headings, all seventeen example READMEs, public Rust types/functions and locally defined API methods. Results link directly to their section or symbol. Generated Rustdoc pages include the same site-wide command menu alongside their native API search.

The shared corpus is generated into `search.json` from the current rendered pages and documentation. It loads only when the menu is opened, stays in memory for that page visit, and is searched locally. Source requests are debounced and canceled as the query changes. Failed index loads can be retried without reloading the page.

`website/static/vendor/qikbar` contains the official Qikbar 0.2.1 ESM distribution and license. No CDN or package installation is needed to build or use the site. To update it, replace the pinned official browser modules and chunks together, preserve the license, refresh the provenance manifest hashes, and run the search/browser checks. See the [dependency policy](/docs/dependencies/) for its separate website scope.

```sh
python3 -B scripts/test_search.py
node website/tests/search.mjs
```
