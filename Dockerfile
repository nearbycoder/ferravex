# Build all public examples and API documentation from this exact source revision.
FROM rust:1.90-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown
WORKDIR /build
COPY . .
RUN python3 scripts/build-site.py

# Runtime contains only the generated site and the original standard-library host.
FROM python:3.13-alpine
WORKDIR /app
COPY --from=build /build/dist/site ./dist/site
COPY scripts/serve-site.py ./scripts/serve-site.py
ENV PORT=8080 PYTHONUNBUFFERED=1
EXPOSE 8080
USER 65534:65534
CMD ["python3", "scripts/serve-site.py"]
