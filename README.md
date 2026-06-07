<p align="center">
  <img src="https://raw.githubusercontent.com/anthod0/pilotfy/assets/assets/logo/dark/logo-transparent.png" width="120" alt="Pilotfy Logo" />
</p>

<p align="center">An experimental control plane for coding agents.</p>

> The project is in development, changes quickly, and is not stable yet. Breaking changes should be expected.

## What pilotfy is

`pilotfy` is for developers who want coding agents to keep working beyond one terminal window.

It aims to provide:

- **Real agent TUI runtime** — use real agent TUIs as runtimes instead of subprocesses such as `claude -p`, allowing sessions to stay alive for a long time while using vendor subscriptions compliantly.
- **One long-lived session, control from anywhere** — start, continue, observe, or steer the same agent session from desktop, Web, mobile, or TUI surfaces.
- **Observable long-running tasks** — let agents plan large tasks as DAGs, then expose each planning and implementation node so developers can understand, intervene, retry, and repair the work.

In short: `pilotfy` keeps official agent work alive, visible, controllable, and fixable.

## Core product ideas

### Real agent TUI runtime

Each vendor has its own TUI agent and subscription model. Since June 15, `claude -p` usage has also moved toward usage-based billing. It is reasonable to expect subscriptions to become more constrained over time and more strongly tied to official TUI clients.

`pilotfy` therefore uses real agent TUIs as runtimes instead of subprocesses such as `claude -p`, allowing sessions to stay alive for a long time while using vendor subscriptions compliantly.

Current state: uses pi to implement the control model.

### One session, many control surfaces

A coding-agent session should not belong to one terminal window.

Start it from a desktop TUI, check it from a phone, continue it from the Web UI, or interrupt it from another device. The same session should stay alive, keep its context, and remain observable wherever the developer is.

Current state: partially implemented with the Web Dashboard, tmux-backed runtimes, pi integrations.

### Agent-planned WorkItem DAGs

Long tasks should not be opaque prompts that run for hours with no structure.

`pilotfy` models long-running work as a WorkItem DAG: an ordered dependency graph, similar to a structured todo list. A Planner creates the execution graph. Worker agents execute work items mechanically along that graph.

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

`pilotfy` is designed around a simple split:

- **Use real agent TUIs as runtimes**: pi, Claude Code, and future clients run as long-lived real TUI processes, currently hosted through tmux, rather than short-lived subprocess commands such as `claude -p`. This keeps sessions alive while preserving official client behavior and legitimate subscription-based usage.
- **pilotfy owns the durable control state**: sessions, turns, tasks, DAG nodes, events, artifacts, and projections live outside the agent process.
- **Every UI is a control surface**: desktop TUI, Web Dashboard, mobile Web, HTTP API, and future clients should attach to the same underlying session instead of creating separate worlds.
- **Long-running tasks are WorkItem DAGs**: large tasks are represented as ordered dependency graphs. A Planner creates and repairs the graph, while Workers stay intentionally simple and execute work items mechanically. Failures, new information, or human interruptions should patch the DAG into a new execution path, with each node remaining inspectable, retryable, and repairable.

## Local development quick start

### Prerequisites

Install:

- Rust / Cargo
- tmux
- pnpm
- pi CLI and/or Claude Code if you want to run those clients locally

### Build the dashboard

```bash
pnpm --dir=apps/dashboard install
pnpm --dir=apps/dashboard run build
```

### Configure pilotfy

`pilotfy` can read configuration from `~/.config/pilotfy/config.toml`, from an explicit `--config` path, or from environment variables.

Minimal example:

```toml
bind_addr = "127.0.0.1:8080"
database_url = "sqlite://~/.local/share/pilotfy/pilotfy.db"
external_api_token = "dev-token"
run_migrations = true

[dashboard]
source = "apps/dashboard/dist"
cache_dir = "~/.cache/pilotfy/dashboard"

[runtime.pi]
tui_command = "pi -e /absolute/path/to/pilotfy/clients/pi"

[runtime.claude_code]
tui_command = "claude"

[workspace_browser]
roots = [
  { root_id = "projects", label = "Projects", path = "/home/me/projects" }
]
```

Environment variables and `.env` are also supported. See [`.env.example`](.env.example) if present in your checkout.

### Run the server

```bash
cargo run
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
