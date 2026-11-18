## Local instructions

Read `../pontia-docs/AGENTS.local.md` if it exists and treat it as part of this file's project instructions.

## Rules

- Delete unused code and replaced implementations in the same change; no fallbacks or compatibility layers. If a change breaks a released external contract (CLI, config, DB schema, APIs), ask the user whether to keep compatibility.
- When a new basic UI primitive is needed, first check the shadcn-svelte component catalog and add the component through the shadcn-svelte CLI instead of hand-rolling it.
- Do not modify any README file unless the user explicitly requests it.
- Do not restrict agent client release versions.
- When performing a pure file split of Rust code without changing behavior, refer to [Rust Module Refactoring](docs/agents/rust-module-refactoring.md) and [Rust Test Refactoring](docs/agents/rust-test-refactoring.md).

## Documentation

- Undocumented means unsupported. Do not write "X is not supported"; mention a limitation only when it is especially important.

## End-to-end validation

Run [Docker E2E](docs/docker-e2e.md) after relevant automated tests pass and before delivery when changes require full-chain validation; use frontend tests and browser validation for changes limited to presentation and local UI interactions.

## Runtime diagnostics

- For runtime diagnostics, use the global log files in the Pontia state directory (by default, `${PONTIA_HOME:-$HOME/.pontia}/state/`), including `runtime.log` and client hook logs such as `pi-hook.log`.

## Database definitions

- Database documents are final SQL schema snapshots, not descriptions of code behavior, workflows, or architecture. When adding a migration, update the corresponding database document in the same change.
- `docs/database/control-plane.md` for the local control-plane SQLite database; migrations live in `crates/pontia-storage-sqlite/migrations/`.
- `docs/database/cloud.md` for the cloud database; migrations live in `apps/cloud/migrations/`.
