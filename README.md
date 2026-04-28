# llmparty

`llmparty` is an MVP backend-only Coding Agent Control Plane. The current implementation includes the Rust project skeleton, SQLite/SQLx wiring, configuration, health check, domain session/turn/event models, event store, reducer-driven state projections, Internal Event API v1, the authenticated External API query surface, session creation/startup through a minimal generic runtime binding, External API turn submission with event-driven execution projection, runtime lifecycle controls for interrupt/terminate/restart, artifact content reads, SSE event streams, a generic client adapter contract validation substitute, and repeatable end-to-end MVP orchestration acceptance tests.

## Requirements

- Rust toolchain with Cargo
- SQLite is embedded through SQLx / `libsqlite3-sys`
- `tmux` for the M1 runtime manager and runtime lifecycle tests

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
# generic runtime returns HTTP 422 with {"error":{"code":"capability_unavailable",...}}

curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/restart \
  -H 'Authorization: Bearer dev-token' \
  -H 'Idempotency-Key: demo-restart-1'
# {"data":{"session":{..."state":"idle"...}},"meta":{},"error":null}

curl -X DELETE http://127.0.0.1:8080/external/v1/sessions/sess_example \
  -H 'Authorization: Bearer dev-token' \
  -H 'Idempotency-Key: demo-terminate-1'
# {"data":{"session":{..."state":"exited"...}},"meta":{},"error":null}
```

## MVP end-to-end acceptance

The MVP end-to-end acceptance coverage lives in `tests/mvp_e2e_acceptance.rs`. The test drives the Control Plane through the same backend-only HTTP polling model expected from an upper Orchestrator:

1. create a session through `POST /external/v1/sessions`
2. submit a turn through `POST /external/v1/sessions/{session_id}/turns`
3. simulate the generic adapter returning facts through `POST /internal/v1/events`
4. poll turn, event, artifact metadata, and artifact content through External API
5. verify unsupported generic runtime interrupt degrades with `capability_unavailable`
6. terminate the session through `DELETE /external/v1/sessions/{session_id}`

The same acceptance test also verifies stable External API error envelopes for authentication failure, invalid requests, missing resources, state conflicts, and unavailable capabilities. Idempotency is verified for retried session and turn creation requests using `Idempotency-Key`.

## M3 SSE event stream validation

Milestone 3 adds read-only External Event Stream API endpoints using Server-Sent Events (SSE). Polling remains supported and keeps the same semantics; streams are a realtime read optimization over the same persisted event store.

```bash
curl -N http://127.0.0.1:8080/external/v1/sessions/sess_example/events/stream \
  -H 'Authorization: Bearer dev-token'

curl -N 'http://127.0.0.1:8080/external/v1/sessions/sess_example/events/stream?after=evt_last_seen' \
  -H 'Authorization: Bearer dev-token'

curl -N http://127.0.0.1:8080/external/v1/sessions/sess_example/turns/turn_example/events/stream \
  -H 'Authorization: Bearer dev-token'
```

Each SSE message uses `id: <event_id>`, `event: domain_event`, and JSON `data` matching `EventView`. Clients can reconnect with `?after=<last received SSE id>` to resume after the last processed event. Invalid or out-of-scope cursors return `400 invalid_request`; unauthenticated requests return `401 authentication_failed`. The stream never reads runtime logs or client internals as a state source.

Run the automated M3 coverage with:

```bash
cargo test --test external_event_stream_api
```

## M2 artifact discovery validation

Milestone 2 adds explicit workspace artifact discovery. Orchestrators can trigger a safe rescan of a session workspace:

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/artifacts/discover \
  -H 'Authorization: Bearer dev-token'
```

Discovery scans only the canonical session workspace, skips `.llmparty`, does not follow symlinks that escape the workspace, and upserts registered `file://` artifacts. It enriches metadata with `relative_path`, `modified_at`, `content_fingerprint`, inferred `kind`, `size_bytes`, and a small text `preview`. Discovery is auxiliary indexing only: it does not emit domain events and does not change session or turn state.

Artifact content reads remain limited to registered sources. Metadata size mismatches return `state_conflict`, unsupported source schemes return `invalid_request`, and files larger than the current content API limit return an explicit `invalid_request` instead of loading the file into a response.

Run the automated M2 coverage with:

```bash
cargo test --test artifact_discovery_api
cargo test --test artifact_content_api
```

## M1 tmux runtime validation

Milestone 1 makes real `tmux` the runtime authority for Control Plane sessions. Creating a `generic` or `pi` session creates a long-lived tmux session, stores internal binding metadata (`backend`, `tmux_session`, `workspace`, `log_path`, `started_at`, `restart_count`), and keeps External API session state derived from domain events rather than tmux state.

Runtime lifecycle coverage uses real tmux, not a fake tmux command:

```bash
cargo test --test tmux_runtime_m1 -- --test-threads=1
```

The M1 tests verify session creation, terminate, restart, crash observation, active-turn failure on runtime crash, and pi session tmux binding. If `tmux -V` does not work in the environment, these tests fail because tmux is a required runtime dependency.

## pi adapter M0/M1.5 validation

Milestone 0 adds `client_type = "pi"` as the first real-client adapter boundary. Milestone 1.5 keeps pi as a long-running TUI inside the tmux runtime and dispatches turns into that existing tmux pane; it does **not** use `pi --mode rpc` and does **not** run one subprocess per turn.

The M0/M1.5 pi path validates that the Control Plane can:

1. create a pi session and runtime binding through External API
2. expose explicit pi capabilities in `SessionView`
3. preserve Control Plane-assigned `session_id` / `turn_id` on pi turns
4. dispatch submitted turn input into the corresponding long-running tmux pi TUI
5. project `turn.started` only after tmux dispatch succeeds
6. project `turn.output`, `turn.completed`, or adapter-reported `turn.failed` from confirmed non-RPC JSONL facts in `$LLMPARTY_ADAPTER_EVENT_LOG`
7. project `turn.failed` when dispatch cannot reach the runtime
8. report malformed adapter outbox records as explicit `session.error` adapter errors without forging turn completion/failure
9. preserve generic adapter behavior without using the generic test adapter for pi turns
10. avoid leaking client-specific fields into External API events

Run the automated pi coverage with:

```bash
cargo test --test pi_adapter_m0 -- --test-threads=1
cargo test --test pi_adapter_m15 -- --test-threads=1
```

For deterministic tests, set `LLMPARTY_PI_TUI_COMMAND` to a long-running TUI substitute. In normal local use the default pi runtime command is `pi`, launched inside the tmux session. The runtime exports `LLMPARTY_ADAPTER_EVENT_LOG=$LLMPARTY_WORKSPACE/.llmparty/adapter-events.jsonl`; a pi hook or wrapper may append newline-delimited JSON facts such as `{"session_id":"...","turn_id":"...","type":"turn.output","payload":{"output":{"summary":"..."}}}` and `turn.completed`. The Control Plane ingests only these explicit adapter facts and malformed records become adapter error events; it still does not infer completion from TUI internals.

## Generic adapter contract

The generic adapter contract validates that the Control Plane does not depend on a specific coding-agent client. It exposes capability metadata (`accept_task`, `report_turn_started`, `report_turn_finished`, `interrupt`, `stream_output`, `heartbeat`, `artifact_sources`), accepts Control Plane-assigned `session_id` / `turn_id` turn input, reports facts back through the Internal Event API, and registers artifact sources in the Control Plane artifact index.

The built-in generic test adapter is a validation substitute, not a pi / Claude Code / Codex deep adapter. Unsupported capabilities remain explicit; for example, the default generic runtime keeps `interrupt: false` and External API interrupt calls return `capability_unavailable` without forging interrupt events.

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
src/runtime/         Runtime control boundary for generic and M0 pi runtime handoff
src/adapters/        Agent client adapter capabilities and contract helpers
tests/               Backend integration tests
apps/web/            Future Web UI application
docs/                Human-facing development/deployment/API docs
spec/                Product and architecture source-of-truth specs
```

## Architecture notes

Axum is restricted to `src/transport/http/` and `src/main.rs`. Domain, storage, application, runtime, and adapter modules do not depend on Axum transport types. The Web UI under `apps/web/` should use the External HTTP API only. Session and turn states are reducer-driven from persisted domain events; runtime bindings and artifacts are auxiliary state and do not drive the primary domain projection.
