# llmparty

`llmparty` is an MVP backend-only, HTTP-only Coding Agent Control Plane. The current implementation is Milestone 0: Rust project skeleton, SQLite/SQLx wiring, configuration, health check, and development commands.

## Requirements

- Rust toolchain with Cargo
- SQLite is embedded through SQLx / `libsqlite3-sys`

## Local configuration

Copy the example environment file for local development if desired:

```bash
cp .env.example .env
```

Configuration is loaded from environment variables first. `.env` is only a local development convenience.

| Variable | Default | Description |
| --- | --- | --- |
| `LLMPARTY_BIND_ADDR` | `127.0.0.1:8080` | HTTP bind address |
| `LLMPARTY_DATABASE_URL` | `sqlite://./data/llmparty.db` | SQLite database URL |
| `LLMPARTY_EXTERNAL_API_TOKEN` | unset | Future External API bearer token source |
| `LLMPARTY_RUN_MIGRATIONS` | `true` | Run SQLx migrations on startup |

## Development commands

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Run locally

```bash
cargo run
```

Then verify the baseline HTTP transport:

```bash
curl http://127.0.0.1:8080/healthz
# {"status":"ok"}
```

## Project structure

```text
Cargo.toml           Single Rust backend crate and binary
migrations/          SQLx migrations for the backend
src/config.rs        Environment-based configuration boundary
src/error.rs         Shared error and Result types
src/ids.rs           UUID v7 based external ID helpers
src/time.rs          UTC time helper boundary
src/application/     Application use-case orchestration boundary
src/domain/          HTTP-free domain boundary placeholder
src/storage/         SQLite / SQLx storage boundary
src/transport/http/  Axum HTTP transport layer
src/runtime/         Runtime control boundary placeholder
src/adapters/        Agent client adapter boundary placeholder
tests/               Backend integration tests
apps/web/            Future Web UI application
docs/                Human-facing development/deployment/API docs
spec/                Product and architecture source-of-truth specs
```

## Architecture notes

Axum is restricted to `src/transport/http/` and `src/main.rs`. Domain, storage, application, runtime, and adapter modules do not depend on Axum transport types. The Web UI under `apps/web/` should use the External HTTP API only. The initial migration exists to validate migration wiring; session, turn, event, and artifact tables are intentionally deferred to later milestones.
