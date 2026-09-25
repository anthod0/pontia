# Pontia Dashboard

View and control your coding-agent sessions from the web. For installation and setup, see the [getting started guide](../../README.md#get-started).

## Development

From the repository root:

```bash
bun install --cwd apps/dashboard
just dev
```

If `just` is not installed yet:

```bash
cargo install just
```

Open <http://127.0.0.1:5173/dashboard/>. Changes reload automatically; press Ctrl-C to stop the development servers.

If you prefer separate terminals, run:

```bash
just dev-backend
just dev-dashboard
```

## Build and serve through pontia

```bash
bun run --cwd apps/dashboard build
PONTIA_DASHBOARD_SOURCE=apps/dashboard/dist just dev-backend
```

Open <http://127.0.0.1:8080/dashboard>.
