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

### Agent-planned WorkItem DAGs

Long tasks should not be opaque prompts that run for hours with no structure.

`pontia` aims to model long-running work as a WorkItem DAG: an ordered dependency graph, similar to a structured todo list. A Planner creates and repairs the execution graph, while Worker agents execute work items along that graph.

The goal is to concentrate intelligence in planning and replanning while keeping workers simple and predictable. Developers should be able to inspect the DAG, understand what happened, and intervene, retry, or repair the task at the node level.

DAG orchestration is a product direction, not a capability of the current release. Its earlier implementation was removed and needs to be redesigned before it is reintroduced.

## Current status

Pontia is experimental and intended for local development use. The current release supports:

- pi as the active agent client integration;
- session creation, conversation, termination, and resume;
- a Web Dashboard for viewing and controlling sessions;
- tmux-backed sessions for Web-based input.

Some workflows are incomplete, and configuration or data formats may change without notice.

## Roadmap

- [x] pi client integration
- [x] Basic Web Dashboard
- [x] Session creation, conversation, termination, and resume
- [x] Reliable bidirectional control across supported interfaces
- [ ] Human approval and review workflows
- [ ] Reintroduce agent-planned WorkItem DAGs
- [ ] Long-running DAG task scheduling, inspection, retry, and repair
- [ ] Stable, versioned product documentation
- [ ] More agent client integrations
- [ ] Ready-to-use binaries and Docker images

## Run locally from source

### Prerequisites

Install:

- Rust / Cargo
- just
- sqlite3 CLI
- tmux
- pnpm
- pi CLI if you want to run the current client integration locally

### Build the dashboard

```bash
pnpm --dir=apps/dashboard install
pnpm --dir=apps/dashboard run build
```

### Configure pontia

`pontia` reads configuration only from `$PONTIA_HOME/config.toml` (default `~/.pontia/config.toml`) plus non-path environment variable overrides.

Set `PONTIA_HOME` to move the whole pontia home root; the database, logs, and dashboard cache live under that root.

Minimal example:

```toml
bind_addr = "127.0.0.1:8080"
external_api_token = "dev-token"
run_migrations = true

[dashboard]
source = "apps/dashboard/dist"

[runtime.pi]
tui_command = "pi -e /absolute/path/to/pontia/clients/pi"

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

Check health:

```bash
curl http://127.0.0.1:8080/healthz
```

Open the dashboard:

```text
http://127.0.0.1:8080/dashboard
```

Enter the configured External API token, for example `dev-token`.
