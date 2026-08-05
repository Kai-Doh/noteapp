# noteapp

A local-first, AI-native notes vault. SQLite is the source of truth, a
Rust/axum HTTP API sits in front of it, and every write — whether from a
human using the desktop app or from an AI agent over the API — goes through
the same code path and the same audit log. Notes support wikilinks,
full-text search, folders, a graph view, and a governed AI memory system
(hot memory + user profile, with a propose/approve queue for anything
sensitive or inferred).

## Architecture

One Rust crate (`src-tauri/`), two binaries, built from feature flags:

- **`noteapp`** — the Tauri desktop app. A thin client: it embeds no
  database or business logic itself, it just talks to a server over HTTP.
- **`server`** — the same backend, headless, with `--no-default-features`
  so the `desktop` Cargo feature (Tauri, GTK/webview deps) never enters the
  build. This is what actually runs the database, the API, and all the
  domain logic. Runs as a Docker container so it can be shared by the
  desktop app *and* an AI agent talking to it directly.

```
Desktop app (Windows/macOS/Linux) ──┐
                                     ├──► HTTP + bearer token ──► server (Docker, SQLite)
AI agent (CLI bridge or direct API)─┘
```

## Repo layout

```
src/                    React + TypeScript frontend (the desktop app's UI)
src-tauri/               Rust backend — both binaries live here
  src/                    domain logic, API routes, auth, backup, export
  src/bin/server.rs        entrypoint for the headless server binary
  Dockerfile               builds ONLY the server binary (see below)
  docker-compose.yml        deploy config — build context is this repo's
                             src-tauri/ subfolder, pulled straight from
                             GitHub (see "Docker deployment" below)
cli/vault_api.py          Python CLI bridge for AI agents (context/search/
                           note/review commands over the HTTP API)
migration/migrate_vault.py  one-time importer from an Obsidian-style vault
docs/AI_INTEGRATION_GUIDE.md   full API reference, written for an AI agent
.github/workflows/release.yml  builds desktop installers on a version tag
```

## Two independent build/deploy paths

This repo feeds two completely separate pipelines that never touch each
other's files:

### 1. Docker server (Dokploy / any Docker host)

`src-tauri/docker-compose.yml`'s build context is a **git URL**, not a
local path:

```yaml
build:
  context: https://github.com/Kai-Doh/noteapp.git#main:src-tauri
  dockerfile: Dockerfile
```

That `#main:src-tauri` means Docker clones this repo and uses *only* the
`src-tauri/` subfolder as build context — nothing from `src/`, `.github/`,
or anything else at the repo root is ever pulled in. The `Dockerfile`
itself is narrower still: it `COPY`s just `Cargo.toml`, `Cargo.lock`,
`migrations/`, and `src/`, and builds with `--no-default-features`, so the
image contains the compiled `server` binary and nothing Tauri-related.

To deploy: put `src-tauri/docker-compose.yml` on the host and run
`docker compose up -d --build`. It always builds fresh from this repo's
`main` branch — no local checkout of the rest of the repo is needed on the
deploy host.

### 2. Desktop installers (GitHub Actions)

Pushing a version tag (`vX.Y.Z`) triggers `.github/workflows/release.yml`,
which builds the `noteapp` desktop binary for Windows, macOS (Apple
Silicon + Intel), and Linux, and attaches the installers to a **draft**
GitHub Release — nothing goes public until you click "Publish release"
yourself.

To cut a release:

```bash
# bump "version" in package.json and src-tauri/tauri.conf.json first
git tag v0.1.1
git push origin v0.1.1
```

This workflow builds from the whole repo (frontend + Tauri backend); it's
unrelated to and never runs on the Docker/server side.

## Local development

```bash
npm install
npm run tauri dev        # desktop app, hot-reloading
```

Requires the Rust toolchain and Tauri's platform prerequisites (see the
[Tauri docs](https://tauri.app/start/prerequisites/)) in addition to Node.

To run the server standalone (e.g. to point a local dev build of the
desktop app at it, or to test API changes without Docker):

```bash
cd src-tauri
cargo run --bin server --no-default-features
```

## For AI agents

See [`docs/AI_INTEGRATION_GUIDE.md`](docs/AI_INTEGRATION_GUIDE.md) for the
full API reference (auth, endpoints, the memory compiler, the review
queue) and [`cli/vault_api.py`](cli/vault_api.py) for a ready-made CLI
bridge.
