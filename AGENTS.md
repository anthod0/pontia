## Local instructions

If `AGENTS.local.md` exists, read it before making changes.

## Project snapshot

- `pontia` is a Rust console/control plane for coding agents with a web dashboard and client integrations.
- Backend: Rust 2024, Axum, Tokio, SQLx/SQLite.
- Frontend/dashboard and client plugins use pnpm.
- Use `pnpm dlx` (not `npx`) to run package binaries.

## Dashboard UI rules

- When a new basic UI primitive is needed, first check the shadcn-svelte component catalog and add the component through the shadcn-svelte CLI instead of hand-rolling it.
- Prefer extending or composing existing `ui/` components before writing one-off markup for common primitives.

## Commands

- Run `just --list` to discover project commands and `just check` for the standard verification suite.
- Use pnpm for package-specific scripts not exposed through `just`.
- Run backend Cargo checks through `just` so SQLx uses the generated check database; do not commit `.sqlx/`.

## Coding style

- Do not preserve backward compatibility. Remove obsolete paths instead of adding compatibility layers, fallbacks, or migrations.
- Choose the simplest implementation that fully meets the current requirements. Avoid speculative abstractions, configuration, and indirection.
- Grow the system in layers. Start from the smallest version that works end to end, and add each new capability on top of a product that already works.
- Keep components modular and concerns clearly separated.
- Make architectural decisions for the long term. Do not accept a stopgap that only works for now and is meant to be replaced later.
