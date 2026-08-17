# pontia Dashboard

SvelteKit SPA + adapter-static + Tailwind CSS + shadcn-svelte dashboard.

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

You can also run the combined development script directly:

```bash
./scripts/dev.sh
```

This starts `cargo run -p pontiad` for the backend and the SvelteKit development server for the dashboard. Open <http://127.0.0.1:5173/dashboard/> during development for HMR updates.

The development server proxies `/external/*` to `http://127.0.0.1:8080`.

If you prefer separate terminals, run:

```bash
just dev-backend
just dev-dashboard
```

## Build and serve through pontia

```bash
pnpm --dir=apps/dashboard run build
PONTIA_DASHBOARD_SOURCE=apps/dashboard/dist PONTIA_EXTERNAL_API_TOKEN=dev-token cargo run -p pontiad
```

Open <http://127.0.0.1:8080/dashboard>.

Equivalent TOML config:

```toml
[dashboard]
source = "apps/dashboard/dist"
```
