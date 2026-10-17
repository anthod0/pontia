## Local instructions

Read `../pontia-docs/AGENTS.local.md` if it exists and treat it as part of this file's project instructions.

## Rules

- Major changes are allowed if necessary. Obsolete branches or approaches should be deprecated and removed promptly; backward compatibility is not required unless explicitly requested.
- When a new basic UI primitive is needed, first check the shadcn-svelte component catalog and add the component through the shadcn-svelte CLI instead of hand-rolling it.
- Prefer extending or composing existing `ui/` components before writing one-off markup for common primitives.
- In `apps/dashboard`, shared modules must never import directly from `src/modes/local/` or `src/modes/public/`. Always import mode-specific dependencies through `$dashboard-mode` so local-only code cannot enter the Public build.
- Do not modify any README file unless the user explicitly requests it.
- Do not restrict agent client release versions.
- When performing a pure file split of Rust code without changing behavior, refer to [Rust Module Refactoring](docs/agents/rust-module-refactoring.md) and [Rust Test Refactoring](docs/agents/rust-test-refactoring.md).

## Runtime diagnostics

- For runtime diagnostics, use the global log files in the Pontia state directory (by default, `${PONTIA_HOME:-$HOME/.pontia}/state/`), including `runtime.log` and client hook logs such as `pi-hook.log`.

## Database definitions

- Database documents are final SQL schema snapshots, not descriptions of code behavior, workflows, or architecture. When adding a migration, update the corresponding database document in the same change.
- `docs/database/control-plane.md` for the local control-plane SQLite database; migrations live in `crates/pontia-storage-sqlite/migrations/`.
- `docs/database/cloud.md` for the cloud database; migrations live in `apps/cloud/migrations/`.
