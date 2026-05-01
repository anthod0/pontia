# llmparty Web UI

Svelte + Vite + TypeScript dashboard served by the Rust backend at `/dashboard` after a production build.

## Development

```bash
pnpm --dir apps/web install
LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run
pnpm --dir apps/web dev
```

Open the Vite dev server URL and use `dev-token` as the External API token. Vite proxies `/external/*` to `http://127.0.0.1:8080`.

## Built mode

```bash
pnpm --dir apps/web build
LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run
```

Open <http://127.0.0.1:8080/dashboard>.
