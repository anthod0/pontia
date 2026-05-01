# llmparty Web UI

Svelte + Vite + TypeScript dashboard served by the Rust backend at `/dashboard` after a production build.

The app is an External API client only. It stores an External API bearer token locally, lists and creates sessions, submits turns, consumes SSE events, runs lifecycle actions, and browses/discovers artifact metadata and content.

## Development

```bash
pnpm --dir apps/web install
LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run
pnpm --dir apps/web dev
```

Open the Vite dev server URL and use `dev-token` as the External API token. Vite proxies `/external/*` to `http://127.0.0.1:8080`.

In development you can create/select a session, submit turns, observe the SSE timeline, click **Discover artifacts**, and select artifacts to read text content or see a safe binary fallback.

## Built mode

```bash
pnpm --dir apps/web build
LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run
```

Open <http://127.0.0.1:8080/dashboard>.

Before serving built mode after frontend changes, rebuild `apps/web`; generated `apps/web/dist/` output is ignored and should not be committed.
