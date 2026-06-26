> [!IMPORTANT]
> DEVELOPMENT MODE
> This project is still in active development. When choosing an approach, prefer long-term planning over short-term gains.
> Major changes are allowed if necessary. Old branches/approaches should be deprecated and removed promptly; backward compatibility is not required unless explicitly requested.
> Client adaptation: Only the pi client requires adaptation; adaptation for the Claude Code client is suspended.

## Project snapshot

- `pontia` is a Rust console/control plane for coding agents with a web dashboard and client integrations.
- Backend: Rust 2024, Axum, Tokio, SQLx/SQLite.
- Frontend/dashboard and client plugins use pnpm.
- Key paths: `control/`, `control/*/tests/`, `apps/dashboard/`, `apps/dashboard/tests/`, `clients/pi/`, `clients/claude-code/`, `specs/`, `TODO.md`, `README.md`.

## Architecture rules

- External API state must come from the event store/projections. Do not treat tmux state, runtime logs, pi/Claude internals, or workspace files as authoritative.
- Dashboard/orchestrators should use `/external/v1/*` only; Web UI must not read SQLite, runtime dirs, workspace files, or `/internal/v1/*` directly. SSE is only a realtime observation optimization, not a replacement for External API snapshots.
- `/internal/v1/events` is for runtime / adapter / agent-client confirmed facts only.
- Keep client-specific behavior inside adapter/runtime/client-plugin boundaries (`control/runtime/`, `control/agent-clients/`, `control/application/`, `control/http/`, `clients/*/`). Do not leak pi/Claude-specific fields into generic domain events or External API view models.
- pi and Claude Code turn output/completion/failure must be reported by hooks through the Internal Event API. Do not parse TUI screen contents, runtime logs, or tmux process exit as turn completion facts.
- Strong bidirectional binding principle: Web UI and TUI differ only in startup and input delivery. Web UI control agent client through the backend API. TUI control agent client manually by the user. After the agent client starts, pontia behavior such as lifecycle tracking, turn facts, event reporting, projections, capabilities, and diagnostics must be identical for WebUI-launched and TUI-launched sessions.
- Preserve idempotency behavior for mutating External API routes that accept `Idempotency-Key`.
- Use the capability model to represent client differences. When a client cannot support an action or fact source, return an explicit unsupported/degraded result rather than fabricating success events.

## Dashboard UI rules

- Dashboard UI uses shadcn-svelte-style components under `apps/dashboard/src/lib/components/ui/`.
- When a new basic UI primitive is needed, first check the shadcn-svelte component catalog and add the component through the shadcn-svelte CLI instead of hand-rolling it.
- Prefer extending or composing existing `ui/` components before writing one-off markup for common primitives such as popovers, dialogs, checkboxes, progress bars, collapsibles, selects, menus, tabs, tables, and form fields.
- Hand-written UI primitives are acceptable only when the component is project-specific or shadcn-svelte does not provide a suitable primitive.
- Use pnpm for shadcn-svelte CLI commands in the dashboard, for example: `pnpm dlx shadcn-svelte@latest add <component> --cwd apps/dashboard`.

## Database migration rules

- Never modify an existing SQL migration file after it has been committed or may have been applied to any database.
- SQLx migration checksums are authoritative: changing existing `control/storage-sqlite/migrations/*.sql` files causes `VersionMismatch` failures for users with existing databases.
- Database schema/data fixes must be implemented by appending a new numbered SQL migration only.
- If a historical migration appears wrong, preserve it and add a follow-up migration that transforms existing databases from the old state to the desired state.

## Domain model and data ownership

- `Task`: user's global intent and primary Web UI object; may exist before workspace/session routing.
- `Workspace`: execution context/cwd and artifact discovery scope, not pontia's state storage location.
- `Session`: long-running agent runtime bound to a workspace; one workspace may have multiple sessions.
- `Turn`: one concrete execution submitted to a session; do not conflate with task.
- Ownership: workspace `1 -> N` sessions, session `1 -> N` turns, task `1 -> 0/1` workspace/session/turn.
- SQLite owns structured state/facts/projections. Filesystem owns large or process-local material such as artifacts, logs, specs, patches, reports, current-turn context, and diagnostics.
- Graph storage, if enabled, stores planning/provenance refs only; do not mirror SQLite wholesale.
- Artifact discovery must not implicitly change session/turn primary state.

## Runtime diagnostics

- Runtime diagnostics are global log files under the pontia state directory (default `${PONTIA_HOME:-$HOME/.pontia}/state/`), including `runtime.log` and client hook logs such as `pi-hook.log`.

## Common commands

- Backend checks/tests:
  - `just fmt-check`
  - `just test`
  - `just clippy`
  - `just sqlx-check`
- Backend + dashboard check:
  - `just check`
- Dashboard:
  - `pnpm --dir=apps/dashboard run check`
  - `pnpm --dir=apps/dashboard run build`
- Client packages:
  - `pnpm --dir clients/pi test`
  - `pnpm --dir clients/pi typecheck`
  - `pnpm --dir clients/claude-code test`
  - `pnpm --dir clients/claude-code typecheck`

## Resource index

- Claude Code plugin reference: <https://code.claude.com/docs/en/plugins-reference> — technical reference for plugin schemas, hooks, commands, and components.

Notes:

- SQLx compile-time query checks use a temporary SQLite database generated from `control/storage-sqlite/migrations/*.sql` by `scripts/sqlx-check-db.sh` / `just sqlx-db`.
- Do not commit `.sqlx/`; run backend cargo commands through the `just` targets so `DATABASE_URL` points at the generated check database.
- Client plugin packages currently have `test` and `typecheck` scripts, not `build` scripts.
