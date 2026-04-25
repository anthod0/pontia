# llmparty

`llmparty` is an MVP backend-only, HTTP-only Coding Agent Control Plane. The current implementation includes Milestone 6: Rust project skeleton, SQLite/SQLx wiring, configuration, health check, domain session/turn/event models, event store, reducer-driven state projections, Internal Event API v1, the authenticated External API query surface, session creation/startup through a minimal generic runtime binding, External API turn submission with event-driven execution projection, and runtime lifecycle controls for interrupt, terminate, and restart.

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
| `LLMPARTY_EXTERNAL_API_TOKEN` | unset | Bearer token required by `/external/v1/*` APIs |
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

Post an internal domain event:

```bash
curl -X POST http://127.0.0.1:8080/internal/v1/events \
  -H 'Content-Type: application/json' \
  -d '{
    "event_id":"evt_example",
    "session_id":"sess_example",
    "turn_id":null,
    "source":"agent_adapter",
    "client_type":"generic",
    "type":"session.created",
    "time":"2026-04-24T12:00:00Z",
    "seq":1,
    "payload":{}
  }'
# {"accepted":true,"duplicate":false,"event_id":"evt_example",...}
```

Create and query External API state with a configured bearer token:

```bash
LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run
curl -X POST http://127.0.0.1:8080/external/v1/sessions \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: demo-session-1' \
  -d '{"client_type":"generic","workspace":"/repo","initial_task":{"input":"Start here"}}'
# {"data":{"session":{...},"initial_turn":{...}},"meta":{},"error":null}

curl http://127.0.0.1:8080/external/v1/sessions \
  -H 'Authorization: Bearer dev-token'
# {"data":{"sessions":[...]},"meta":{},"error":null}

curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/turns \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: demo-turn-1' \
  -d '{"input":"Continue with the next task","metadata":{"source":"demo"}}'
# {"data":{"turn":{..."state":"queued"...}},"meta":{},"error":null}

curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/interrupt \
  -H 'Authorization: Bearer dev-token'
# generic runtime returns {"error":{"code":"capability_unavailable",...}}

curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/restart \
  -H 'Authorization: Bearer dev-token' \
  -H 'Idempotency-Key: demo-restart-1'
# {"data":{"session":{..."state":"idle"...}},"meta":{},"error":null}

curl -X DELETE http://127.0.0.1:8080/external/v1/sessions/sess_example \
  -H 'Authorization: Bearer dev-token' \
  -H 'Idempotency-Key: demo-terminate-1'
# {"data":{"session":{..."state":"exited"...}},"meta":{},"error":null}
```

## Project structure

```text
Cargo.toml           Single Rust backend crate and binary
migrations/          SQLx migrations for the backend
src/config.rs        Environment-based configuration boundary
src/error.rs         Shared error and Result types
src/ids.rs           UUID v7 based external ID helpers
src/time.rs          UTC time helper boundary
src/application/     Application use-case orchestration and event ingest service
src/domain/          HTTP-free domain models and reducer
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

Axum is restricted to `src/transport/http/` and `src/main.rs`. Domain, storage, application, runtime, and adapter modules do not depend on Axum transport types. The Web UI under `apps/web/` should use the External HTTP API only. Session and turn states are reducer-driven from persisted domain events; runtime bindings and artifacts are auxiliary state and do not drive the primary domain projection.
