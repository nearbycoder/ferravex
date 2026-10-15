# Build the documentation site

The Ferravex site is generated from repository Markdown, crate READMEs and the game catalog. Python's standard library builds the pages; the browser uses native CSS and JavaScript.

## Build everything

```sh
python3 scripts/build-site.py
python3 scripts/serve-site.py --port 8080
```

The build compiles all fifteen browser examples, stages the matching legacy loader, generates the owned hosts, builds Rust API documentation, and writes the complete site to dist/site.

For content-only changes after a full build:

```sh
python3 scripts/build-site.py --skip-build
```

The content-only command requires existing example and API artifacts. It fails if required inputs are missing.

## Deploy to Railway

The repository Dockerfile builds Wasm and API docs in a Rust builder, then copies only the generated site and the original static server into the runtime image. Railway supplies the PORT environment variable; the server listens on all interfaces and exposes /healthz.

The custom domain stays registered with Vercel. DNS routes it to the Railway docs service. No engine build artifacts contain deployment credentials.

## Add an example

Legacy adapters use examples/catalog.json. Add its crate to the workspace and its catalog entry, then provide a README and screenshot. Owned examples have explicit host staging in scripts/build-owned.py and entries in the site builder.

Example pages load the game only after Play is selected. This avoids downloading fifteen Wasm modules or running fifteen render loops on a gallery page.

## Update documentation

Edit the relevant Markdown source or crate README. Search, navigation and the sitemap are regenerated with the pages. Keep claims about feature support aligned with the tests and the implementation status document.
