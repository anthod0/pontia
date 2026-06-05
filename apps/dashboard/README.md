# pilotfy Dashboard

Svelte + Vite + shadcn-svelte dashboard.

## Development

From the repository root:

```bash
pnpm --dir=apps/dashboard install
just dev
```

If `just` is not installed yet:

```bash
cargo install just
```

You can also run the script directly:

```bash
./scripts/dev-dashboard.sh
```

This starts `cargo run` for the backend and the Vite dev server for the dashboard. Open <http://127.0.0.1:5173/dashboard/> during development for Vite HMR updates.

The Vite dev server proxies `/external/*` to `http://127.0.0.1:8080`.

If you prefer separate terminals, run:

```bash
PILOTFY_EXTERNAL_API_TOKEN=dev-token cargo run
pnpm --dir=apps/dashboard run dev
```

## Build and serve through pilotfy

```bash
pnpm --dir=apps/dashboard run build
PILOTFY_DASHBOARD_SOURCE=apps/dashboard/dist PILOTFY_EXTERNAL_API_TOKEN=dev-token cargo run
```

Open <http://127.0.0.1:8080/dashboard>.

Equivalent TOML config:

```toml
[dashboard]
source = "apps/dashboard/dist"
```

## Implementation plan

See `../../specs/plans/2026-05-13-dashboard-v2.md`.
