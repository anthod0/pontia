# Web UI

Milestone 4 ships the minimal Web Dashboard as a zero-build page served by the Rust backend at `/dashboard`.

The dashboard uses vanilla HTML/CSS/JavaScript and consumes the External HTTP API only. It does not read the SQLite database, runtime internals, client logs, or workspace files directly.

Run the backend with an External API token:

```bash
LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run
```

Open <http://127.0.0.1:8080/dashboard>, enter the token, and use the page to create sessions, submit turns, inspect events, browse artifacts, and trigger lifecycle actions.
