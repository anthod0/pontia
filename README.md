# llmparty

`llmparty` is an external control system for coding agents. It keeps agent sessions, turns, events, and artifacts outside the agent process, so long-running work can be observed, controlled, interrupted, and resumed.

It is built for multi-client agent control, Web Dashboard operation, automation via HTTP APIs, and future DAG-based long-term task orchestration.

## Milestones

`llmparty` is being built in public. The current work focuses on the control plane foundation, then gradually moves toward long-running autonomous task execution.

- **Control Plane Foundation**: maintain authoritative session / turn / event / artifact state outside the agent process, with HTTP APIs for external control.
- **Multi-client Agent Control**: support different coding agent clients such as pi, Claude Code, and future runtimes through one shared model.
- **Web Dashboard Operation**: provide a browser interface to create sessions, submit work, inspect progress, review outputs, browse artifacts, and intervene when needed.
- **Operational Readiness**: improve deployment, diagnostics, API documentation, logging, metrics, CI, and compatibility checks.
- **DAG-based Long-term Planning**: represent larger goals as task graphs with dependencies, checkpoints, retries, and agent assignments.
- **Autonomous Agent Orchestration**: let the external control system decide what should run next, which agent should handle it, when to pause or retry, and how completed work feeds into later tasks.

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

### 1. Configure llmparty

llmparty can be configured with a TOML file. By default it looks for:

```text
~/.config/llmparty/config.toml
```

You can also pass an explicit path:

```bash
cargo run -- --config /path/to/config.toml
# or
LLMPARTY_CONFIG=/path/to/config.toml cargo run
```

Example:

```toml
bind_addr = "127.0.0.1:8080"
database_url = "sqlite://~/.local/share/llmparty/llmparty.db"
external_api_token = "dev-token"
run_migrations = true

[dashboard]
# Local Vite dist directory, or a remote .zip/.tar.gz/.tgz archive containing exactly one index.html.
source = "apps/web/dist"
cache_dir = "~/.cache/llmparty/dashboard"

[runtime.pi]
tui_command = "pi -e /absolute/path/to/llmparty/clients/pi"

[runtime.claude_code]
tui_command = "claude"

[workspace_browser]
roots = [
  { root_id = "projects", label = "Projects", path = "/home/me/projects" }
]
```

Environment variables and `.env` still work and take precedence over TOML values:

```bash
cp .env.example .env
```

The default configuration listens on `127.0.0.1:8080`.

Dashboard `source` may be a local built dashboard directory or a remote archive URL. If `source` is missing, or a local source does not contain `index.html`, `/dashboard` returns a plain unavailable message instead of falling back. Remote archives are refreshed on startup into `cache_dir`; if refresh fails, llmparty serves the previous cache when one exists. The archive must contain exactly one `index.html` entry, whose parent directory is treated as the dashboard root.

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
   - `claude_code`: for using Claude Code with the llmparty Claude Code plugin
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

### Workspace browser roots

Restricted workspace browsing is configured with `LLMPARTY_WORKSPACE_ROOTS`:

```bash
export LLMPARTY_WORKSPACE_ROOTS='projects|Projects|/home/me/projects;tmp|Temporary|/tmp'
```

Each entry is `root_id|label|path`. `root_id` is only a configuration/API handle; it is not stored in the workspace database.

```bash
curl http://127.0.0.1:8080/external/v1/workspace-roots \
  -H 'Authorization: Bearer dev-token'

curl 'http://127.0.0.1:8080/external/v1/workspace-roots/projects/entries?path=llmparty' \
  -H 'Authorization: Bearer dev-token'

curl -X POST http://127.0.0.1:8080/external/v1/workspaces \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -d '{"root_id":"projects","path":"llmparty","name":"llmparty"}'
```

### Create a session

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: demo-session-1' \
  -d '{
    "client_type":"claude_code",
    "workspace":"/tmp/llmparty-demo",
    "initial_task":{"input":"Please introduce the current project"}
  }'
```

Web UI callers can use a previously registered workspace ID instead of a raw path:

```json
{"client_type":"claude_code","workspace_id":"wks_example"}
```

### List sessions

```bash
curl http://127.0.0.1:8080/external/v1/sessions \
  -H 'Authorization: Bearer dev-token'
```

### Submit the next message

Replace `sess_example` with the actual returned session ID. Messages submitted to the session inbox are dispatched as turns when the session is ready.

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/inbox/messages \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: demo-message-1' \
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
| `LLMPARTY_INTERNAL_EVENT_URL`   | Auto-derived or manually set                   | URL used by agents / hooks to report events      |
| `LLMPARTY_DASHBOARD_SOURCE`     | Not set                                        | Local dashboard dist directory or remote archive |
| `LLMPARTY_DASHBOARD_CACHE_DIR`  | `~/.cache/llmparty/dashboard`                  | Cache directory for remote dashboard archives    |
| `LLMPARTY_PI_TUI_COMMAND`       | `pi`                                           | Startup command used for pi sessions             |

## Development Commands

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
pnpm --dir apps/web typecheck
pnpm --dir apps/web build
```
