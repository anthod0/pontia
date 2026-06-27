<p align="center">
  <img src="https://raw.githubusercontent.com/anthod0/pontia/assets/assets/logo/dark/logo-transparent.png" width="120" alt="Pontia Logo" />
</p>

<p align="center">An experimental control plane for coding agents.</p>

> The project is in development, changes quickly, and is not stable yet. Breaking changes should be expected.

## What pontia is

`pontia` is for developers who want coding agents to keep working beyond one terminal window.

It aims to provide:

- **Real agent TUI runtime** — use real agent TUIs as runtimes instead of short-lived subprocess prompts, allowing sessions to stay alive for a long time while preserving official client behavior.
- **One long-lived session, control from anywhere** — start, continue, observe, or steer the same agent session from desktop, Web, mobile, or TUI surfaces.
- **Observable long-running tasks** — let agents plan large tasks as DAGs, then expose each planning and implementation node so developers can understand, intervene, retry, and repair the work.

In short: `pontia` keeps official agent work alive, visible, controllable, and fixable.

## Core product ideas

### Real agent TUI runtime

Each vendor has its own TUI agent and subscription model. `pontia` therefore uses real agent TUIs as runtimes instead of short-lived subprocess prompts, allowing sessions to stay alive for a long time while preserving official client behavior.

Current state: uses pi to implement the control model.

### One session, many control surfaces

A coding-agent session should not belong to one terminal window.

Start it from a desktop TUI, check it from a phone, continue it from the Web UI, or interrupt it from another device. The same session should stay alive, keep its context, and remain observable wherever the developer is.

Current state: partially implemented with the Web Dashboard, tmux-backed runtimes, pi integrations, and pi runtime binding upsert. A pi TUI that loads the pontia extension binds to a pontia session at startup: tmux-backed pi sessions can accept Web input through their bound pane, while non-tmux pi sessions remain observable but are not writable from the Web UI.

### Agent-planned WorkItem DAGs

Long tasks should not be opaque prompts that run for hours with no structure.

`pontia` models long-running work as a WorkItem DAG: an ordered dependency graph, similar to a structured todo list. A Planner creates the execution graph. Worker agents execute work items mechanically along that graph.

The goal is to concentrate intelligence in planning and replanning, while keeping workers simple and predictable. Developers can inspect the DAG, understand what happened, and repair the task at the node level.

Current state: early task, DAG, work-item, proposal, scheduler, and provenance models exist; the full long-running autonomous workflow is not complete.

## Roadmap

- [x] Control plane foundation: backend, SQLite state, events, sessions, turns, artifacts
- [x] Real agent TUI runtime through tmux
- [x] pi agent integrations
- [x] Basic Web Dashboard
- [x] TUI session creation, conversation, termination, and resume
- [x] Basic DAG planning and execution
- [ ] Bidirectional control from anywhere
- [ ] Human approval / review gates
- [ ] Artifact browsing and diff/review workflow
- [ ] Long-running DAG task scheduler
- [ ] API stability and versioned documentation
- [ ] More agent client integrations
- [ ] Ready-to-use builds: binaries and Docker images

## Architecture principles

`pontia` is designed around a simple split:

- **Use real agent TUIs as runtimes**: pi and future clients run as long-lived real TUI processes, currently hosted through tmux, rather than short-lived subprocess prompts. This keeps sessions alive while preserving official client behavior.
- **pontia owns the durable control state**: sessions, turns, tasks, DAG nodes, events, artifacts, and projections live outside the agent process.
- **Every UI is a control surface**: desktop TUI, Web Dashboard, mobile Web, HTTP API, and future clients should attach to the same underlying session instead of creating separate worlds. Runtime bindings and capabilities describe what each live client can do; for example, a pi TUI outside tmux is still observable but reports `accept_task = false`, so the Web composer is disabled.
- **Long-running tasks are WorkItem DAGs**: large tasks are represented as ordered dependency graphs. A Planner creates and repairs the graph, while Workers stay intentionally simple and execute work items mechanically. Failures, new information, or human interruptions should patch the DAG into a new execution path, with each node remaining inspectable, retryable, and repairable.

## Local development quick start

### Prerequisites

Install:

- Rust / Cargo
- just
- sqlite3 CLI
- tmux
- pnpm
- pi CLI if you want to run the current client integration locally

### SQLx compile-time checks

Pontia uses SQLx compile-time query macros for SQLite repository SQL. The project does not commit `.sqlx/` offline cache files. Instead, backend commands generate a temporary SQLite check database from `control/storage-sqlite/migrations/*.sql` and set `DATABASE_URL` for compilation.

Use the `just` targets for backend development:

```bash
just sqlx-db      # create /tmp/pontia_sqlx_check.db and print its DATABASE_URL
just sqlx-check   # compile with SQLx query checks against the temporary database
just test
just clippy
just check
```

If you run cargo directly after editing SQL or migrations, set `DATABASE_URL` yourself:

```bash
DATABASE_URL="$(./scripts/sqlx-check-db.sh)" cargo test
```

### Build the dashboard

```bash
pnpm --dir=apps/dashboard install
pnpm --dir=apps/dashboard run build
```

### Configure pontia

`pontia` reads configuration only from `$PONTIA_HOME/config.toml` (default `~/.pontia/config.toml`) plus non-path environment variable overrides.

Set `PONTIA_HOME` to move the whole pontia home root; the database, graph data, logs, and dashboard cache live under that root.

Minimal example:

```toml
bind_addr = "127.0.0.1:8080"
external_api_token = "dev-token"
run_migrations = true

[dashboard]
source = "apps/dashboard/dist"

[runtime.pi]
tui_command = "pi --approve -e /absolute/path/to/pontia/clients/pi"


[workspace_browser]
roots = [
  { root_id = "projects", label = "Projects", path = "/home/me/projects" }
]

[file_picker]
enabled = true
min_query_chars = 0
max_results = 100
max_candidates = 100000
timeout_ms = 1500
respect_gitignore = true
respect_ignore_files = true
respect_git_exclude = true
include_hidden = false
follow_symlinks = false
ignore_globs = [
  ".git/**",
  "node_modules/**",
  "target/**",
  "dist/**",
  "build/**",
  ".svelte-kit/**",
  ".next/**"
]
```

Environment variables and `.env` are also supported. See [`.env.example`](.env.example) if present in your checkout.

### Run the server

```bash
just backend
```

Or, when running cargo directly:

```bash
DATABASE_URL="$(./scripts/sqlx-check-db.sh)" cargo run
```

Check health:

```bash
curl http://127.0.0.1:8080/healthz
```

Open the dashboard:

```text
http://127.0.0.1:8080/dashboard
```

Enter the configured External API token, for example `dev-token`.
