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

### Workspaces

When asked for a worktree or isolated workspace, run `scripts/create-workspace <name> [base-revision]`; the base defaults to the calling checkout's HEAD.
Destination: `$HOME/worktrees/pontia/<name>`.

## Project snapshot

`pontia` is a Rust console/control plane for coding agents with a web dashboard and client integrations.

## Dashboard UI rules

- When a new basic UI primitive is needed, first check the shadcn-svelte component catalog and add the component through the shadcn-svelte CLI instead of hand-rolling it.
- Prefer extending or composing existing `ui/` components before writing one-off markup for common primitives.

## Agent documentation

- Do not modify any README file unless the user explicitly requests it.
- Database definitions
  - `docs/database/control-plane.md` for `crates/pontia-storage-sqlite/migrations/`
  - `docs/database/edge.md` for `crates/pontia-edge/migrations/`
  - When adding a migration, update the corresponding database document in the same change. 
  - Database documents are final SQL schema snapshots, not descriptions of code behavior, workflows, or architecture.
