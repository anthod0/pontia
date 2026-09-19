> [!IMPORTANT]
> DEVELOPMENT MODE
> This project is still in active development. When choosing an approach, prefer long-term planning over short-term gains.
> Major changes are allowed if necessary. Obsolete branches or approaches should be deprecated and removed promptly; backward compatibility is not required unless explicitly requested.

## Local instructions

If `AGENTS.local.md` exists, read it before making changes.

## Agent skills

### Issue tracker

Issues and specs are tracked as local Markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Triage uses the five canonical status strings. See `docs/agents/triage-labels.md`.

### Domain docs

Domain documentation uses the single-context layout. See `docs/agents/domain.md`.

## Project snapshot

`pontia` is a Rust console/control plane for coding agents with a web dashboard and client integrations.

## Dashboard UI rules

- When a new basic UI primitive is needed, first check the shadcn-svelte component catalog and add the component through the shadcn-svelte CLI instead of hand-rolling it.
- Prefer extending or composing existing `ui/` components before writing one-off markup for common primitives.

## Commands

- Run `just --list` to discover project commands and `just check` for the standard verification suite.
- Use pnpm for package-specific scripts not exposed through `just`.
- Run backend Cargo checks through `just` so SQLx uses the committed `.sqlx/` metadata in offline mode.
- Commit `.sqlx/`. After changing SQLx query macros or SQLite migrations, run `just sqlx-prepare` and include the refreshed metadata.

## Agent documentation

- Current SQLite table, column, trigger, and index definitions: [`docs/database-schema.md`](docs/database-schema.md). When adding a migration, you must update this document in the same change.
