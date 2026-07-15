## Project snapshot

- `pontia` is a Rust console/control plane for coding agents with a web dashboard and client integrations.
- Backend: Rust 2024, Axum, Tokio, SQLx/SQLite.
- Frontend/dashboard and client plugins use pnpm.
- Key paths: `control/`, `control/*/tests/`, `apps/dashboard/`, `apps/dashboard/tests/`, `clients/pi/`, `README.md`.

## Local instructions

If `AGENTS.local.md` exists, read it before making changes.

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

Notes:

- SQLx compile-time query checks use a temporary SQLite database generated from `control/storage-sqlite/migrations/*.sql` by `scripts/sqlx-check-db.sh` / `just sqlx-db`.
- Do not commit `.sqlx/`; run backend cargo commands through the `just` targets so `DATABASE_URL` points at the generated check database.
- Client plugin packages currently have `test` and `typecheck` scripts, not `build` scripts.
