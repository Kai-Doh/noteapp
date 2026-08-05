# Build context is src-tauri/ (this directory) — see docker-compose.yml.
#
# Builds only the headless `server` binary, with `--no-default-features` so
# the `desktop` feature (Tauri, tauri-build, keyring) never enters the
# dependency graph at all. That matters on Linux specifically: the `tauri`
# crate's own build script requires webkit2gtk/gtk3 dev packages via
# pkg-config just to *compile*, which this container has no business needing
# — it's the same backend the desktop app used to embed, running headless so
# multiple clients (the desktop app, an AI agent) can share one instance.

# ---- build stage ----
FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release --bin server --no-default-features

# ---- runtime stage ----
FROM debian:bookworm-slim
RUN groupadd -r noteapp && useradd -r -g noteapp -d /data noteapp
COPY --from=builder /app/target/release/server /usr/local/bin/noteapp-server
RUN mkdir -p /data && chown -R noteapp:noteapp /data

USER noteapp
ENV NOTEAPP_DATA_DIR=/data
ENV NOTEAPP_BIND_ADDR=0.0.0.0:47823
EXPOSE 47823
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/noteapp-server"]
