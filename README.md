# llmparty

`llmparty` is a console and control plane for coding agents. It lets you start and manage agent sessions, submit tasks to agents, inspect execution progress, results, and artifacts, and interrupt, restart, or terminate sessions when needed.

It is designed for scenarios such as:

- Operating local or remote coding agents from a browser
- Integrating agents into scripts, automation workflows, or higher-level orchestrators via an HTTP API
- Managing different agent clients with a shared session / turn / event / artifact model
- Keeping agent runtimes alive for long-running work via `tmux`, instead of starting a temporary process for every task

Currently supported clients are the `generic` test client and the `pi` client.

## Feature Overview

- Create, inspect, and manage agent sessions
- Submit multi-turn tasks / prompts to a session
- View event streams and turn output in real time
- Browse file artifacts produced by a session
- Interrupt, restart, and terminate sessions
- Web Dashboard browser interface
- HTTP External API for script and system integration
- Local execution and result reporting for the pi client

## Prerequisites

Install the following:

- Rust / Cargo
- tmux
- pnpm (for the Web Dashboard)
- pi CLI (if you want to use `client_type = "pi"`)

## Quick Start

### 1. Configure environment variables

```bash
cp .env.example .env
```

The default configuration listens on `127.0.0.1:8080` and uses `dev-token` as the token for Dashboard and External API access.

### 2. Install dependencies and build the Dashboard

```bash
pnpm --dir apps/web install
pnpm --dir apps/web build
```

### 3. Start llmparty

```bash
cargo run
```

After the service starts, check its health status:

```bash
curl http://127.0.0.1:8080/healthz
```

Expected response:

```json
{ "status": "ok" }
```

### 4. Open the Dashboard

Visit:

```text
http://127.0.0.1:8080/dashboard
```

Enter the token on the page, for example:

```text
dev-token
```

You can then create sessions, submit tasks, and view events and results.

## Using the Dashboard

The Dashboard is the recommended way to use llmparty locally.

Common workflow:

1. Open `/dashboard`
2. Enter the External API token
3. Create a session
4. Choose a client type:
   - `generic`: for validating the workflow and API
   - `pi`: for using the real pi client
5. Enter the workspace path
6. Submit a task
7. View the agent response in the event stream and output areas
8. If needed, inspect artifacts, restart the session, or terminate it

If a session is currently running a turn, the Dashboard temporarily disables new submissions until the current turn completes or fails.

## Using the pi client

Before using pi, make sure it can run directly on your machine:

```bash
pi
```

When starting llmparty, it is recommended to explicitly configure the internal event reporting URL:

```bash
LLMPARTY_EXTERNAL_API_TOKEN=dev-token \
LLMPARTY_INTERNAL_EVENT_URL=http://127.0.0.1:8080/internal/v1/events \
cargo run
```

Then create a session with `client_type = "pi"` in the Dashboard.

If you need llmparty to use a specific pi command or local extension, set:

```bash
LLMPARTY_PI_TUI_COMMAND='pi -e /absolute/path/to/llmparty/clients/pi'
```

During a pi session, llmparty stores its runtime state files under the global runtime directory:

```text
~/.local/share/llmparty/runtimes/<session_id>/current-turn.json
~/.local/share/llmparty/runtimes/<session_id>/pi-hook.log
```

The workspace is used only as the runtime current working directory; llmparty no longer creates an in-project `.llmparty/` directory. If the Dashboard does not receive pi output or completion events, first check the corresponding `pi-hook.log` file in the runtime directory.

## Using the HTTP API

All external APIs are under `/external/v1/*` and require a Bearer token.

The examples below assume the service is running at `127.0.0.1:8080` with token `dev-token`.

### Create a session

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: demo-session-1' \
  -d '{
    "client_type":"generic",
    "workspace":"/tmp/llmparty-demo",
    "initial_task":{"input":"Please introduce the current project"}
  }'
```

### List sessions

```bash
curl http://127.0.0.1:8080/external/v1/sessions \
  -H 'Authorization: Bearer dev-token'
```

### Submit the next turn

Replace `sess_example` with the actual returned session ID.

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/turns \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: demo-turn-1' \
  -d '{"input":"Continue with the next step"}'
```

### View events

```bash
curl http://127.0.0.1:8080/external/v1/sessions/sess_example/events \
  -H 'Authorization: Bearer dev-token'
```

### Subscribe to the event stream in real time

```bash
curl -N http://127.0.0.1:8080/external/v1/sessions/sess_example/events/stream \
  -H 'Authorization: Bearer dev-token'
```

### View artifacts

```bash
curl http://127.0.0.1:8080/external/v1/sessions/sess_example/artifacts \
  -H 'Authorization: Bearer dev-token'
```

### Discover workspace artifacts

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/artifacts/discover \
  -H 'Authorization: Bearer dev-token'
```

### Terminate a session

```bash
curl -X DELETE http://127.0.0.1:8080/external/v1/sessions/sess_example \
  -H 'Authorization: Bearer dev-token' \
  -H 'Idempotency-Key: demo-terminate-1'
```

## Common Configuration

| Variable                      | Default                                        | Description                                      |
| ----------------------------- | ---------------------------------------------- | ------------------------------------------------ |
| `LLMPARTY_BIND_ADDR`          | `127.0.0.1:8080`                               | Service bind address                             |
| `LLMPARTY_DATABASE_URL`       | `sqlite://~/.local/share/llmparty/llmparty.db` | SQLite database URL                              |
| `LLMPARTY_EXTERNAL_API_TOKEN` | Not set                                        | Bearer token for the Dashboard and External API  |
| `LLMPARTY_RUN_MIGRATIONS`     | `true`                                         | Automatically run database migrations on startup |
| `LLMPARTY_INTERNAL_EVENT_URL` | Auto-derived or manually set                   | URL used by agents / hooks to report events      |
| `LLMPARTY_PI_TUI_COMMAND`     | `pi`                                           | Startup command used for pi sessions             |

## Development Commands

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
pnpm --dir apps/web typecheck
pnpm --dir apps/web build
```
