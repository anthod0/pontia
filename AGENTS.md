> [!IMPORTANT]
> DEVELOPMENT MODE
> This project is still in active development. When choosing an approach, prefer long-term planning over short-term gains.
> Major changes are allowed if necessary. Obsolete branches or approaches should be deprecated and removed promptly; backward compatibility is not required unless explicitly requested.

## Local instructions

If `../pontia-docs/AGENTS.local.md` exists, read it before making changes.

## Project snapshot

`pontia` is a Rust console/control plane for coding agents with a web dashboard and client integrations.

## Rules

- When a new basic UI primitive is needed, first check the shadcn-svelte component catalog and add the component through the shadcn-svelte CLI instead of hand-rolling it.
- Prefer extending or composing existing `ui/` components before writing one-off markup for common primitives.
- Do not modify any README file unless the user explicitly requests it.
- Do not restrict agent client release versions.
- When performing a pure file split of Rust code without changing behavior, refer to [Rust Module Refactoring](docs/agents/rust-module-refactoring.md) and [Rust Test Refactoring](docs/agents/rust-test-refactoring.md).

## Database definitions

- Database documents are final SQL schema snapshots, not descriptions of code behavior, workflows, or architecture. When adding a migration, update the corresponding database document in the same change.
- `docs/database/control-plane.md` for `pontiad`
- `docs/database/edge.md` for `pontia-edge`

## Agent skills

### Issue tracker

Issues and specs are tracked as local Markdown files under `../pontia-docs/scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Triage uses the five canonical status strings. See `docs/agents/triage-labels.md`.

### Domain docs

Domain documentation uses the single-context layout. See `docs/agents/domain.md`.

### Workspaces

When asked for a worktree or isolated workspace, run `scripts/create-workspace <name> [base-revision]`; the base defaults to the calling checkout's HEAD. New worktree destination is `$HOME/worktrees/pontia/<name>`.
