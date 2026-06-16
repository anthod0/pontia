# @pontia/pi-client-plugin

First-party pi extension for reporting startup readiness plus confirmed turn facts from an already-bound pi runtime back to pontia.

## Install locally

From the pontia repository root, run pi with this package as a temporary extension:

```bash
pi --approve -e ./clients/pi
```

Or install it into project-local pi settings:

```bash
pi install -l ./clients/pi
```

## Runtime binding

The extension does not create or auto-attach pontia sessions after pi has started. WebUI-launched and TUI-launched sessions must both be bound before the `pi` process starts, and the launcher must inject the same managed runtime environment into pi.

On `session_start`, the extension reads `PONTIA_SESSION_ID`, `PONTIA_RUNTIME_INSTANCE_ID`, and `PONTIA_INTERNAL_EVENT_URL`. If that managed runtime environment is complete, it reports against that existing pontia session. If it is absent or incomplete, the extension skips pontia reporting for that pi process instead of calling pontia APIs or guessing a default server address.

## Runtime environment

The extension reads configuration from environment variables:

| Variable | Required | Default |
| --- | --- | --- |
| `PONTIA_WORKSPACE` | recommended | pi process cwd |
| `PONTIA_RUNTIME_DIR` | recommended | pi process cwd |
| `PONTIA_SESSION_ID` | required for reporting | none |
| `PONTIA_RUNTIME_INSTANCE_ID` | required for reporting | none |
| `PONTIA_CURRENT_TURN_FILE` | recommended | `$PONTIA_RUNTIME_DIR/current-turn.json` |
| `PONTIA_INTERNAL_EVENT_URL` | required for reporting | none |
| `PONTIA_PI_HOOK_LOG` | recommended | `$PONTIA_RUNTIME_DIR/pi-hook.log` |

Expected `current-turn.json` for backend-delivered input:

```json
{
  "session_id": "sess_xxx",
  "input": "user task",
  "client_type": "pi",
  "runtime_instance_id": "rtinst_xxx",
  "internal_event_url": "http://127.0.0.1:8080/internal/v1/events"
}
```

`session_id`, `runtime_instance_id`, and `client_type: "pi"` are required. `turn_id` is intentionally omitted for pi: the plugin generates the authoritative pontia turn id when pi reports a real `agent_start`. `PONTIA_INTERNAL_EVENT_URL` and `PONTIA_RUNTIME_INSTANCE_ID` override file values when present.

## What the extension reports

- On `session_start` with reason `startup`, it posts a one-time `session.ready` signal from `agent_client` for the pre-bound session with the current `runtime_instance_id` plus the real pi session identity from `ctx.sessionManager.getSessionId()` as `client_session_key`.
- On `agent_start`, it reads the pending input context, generates a fresh pontia `turn_id`, and posts `turn.started`.
- On assistant message updates/end events, it collects assistant-visible text from pi lifecycle event payloads.
- When pi exposes context usage through hook events, message usage, or `ctx.getContextUsage()`, it posts `session.context_usage_updated`; it does not parse session files or fabricate usage when pi does not provide it.
- On `agent_end`, it posts `turn.output` when text was collected, then posts `turn.completed` for the plugin-generated turn id.
- If pi exposes an explicit agent-end error, it posts `turn.failed`.

The extension does not parse TUI screen contents and does not infer completion from tmux, process state, or runtime exit.

## pontia tools

DAG task development is currently frozen while pontia focuses on session-first Web UI and bidirectional session control. The pi extension no longer registers agent-visible DAG tools. The shared contract at `clients/tools/pontia-tools.v1.json` is retained for backend compatibility and future revival.

## Manual validation

When pi is launched by pontia `client_type = "pi"` runtime, the Control Plane writes `current-turn.json` under the global runtime directory and exports `PONTIA_SESSION_ID`, `PONTIA_RUNTIME_INSTANCE_ID`, `PONTIA_RUNTIME_DIR`, `PONTIA_CURRENT_TURN_FILE`, `PONTIA_INTERNAL_EVENT_URL`, and `PONTIA_PI_HOOK_LOG` for the hook. A manually opened pi TUI must be launched through an equivalent pre-binding launcher that exports the same environment before pi starts. The steps below are useful for standalone plugin validation.

1. Start pontia so `/internal/v1/events` is reachable.
2. Create the current turn file in a runtime directory:

   ```bash
   export PONTIA_RUNTIME_DIR="$HOME/.local/share/pontia/runtimes/manual-pi"
   mkdir -p "$PONTIA_RUNTIME_DIR"
   cat > "$PONTIA_RUNTIME_DIR/current-turn.json" <<'JSON'
   {
     "session_id": "sess_xxx",
     "input": "hello",
     "client_type": "pi",
     "runtime_instance_id": "rtinst_xxx",
     "internal_event_url": "http://127.0.0.1:8080/internal/v1/events"
   }
   JSON
   ```

3. Export environment for the pi process:

   ```bash
   export PONTIA_WORKSPACE="$PWD"
   export PONTIA_SESSION_ID="sess_xxx"
   export PONTIA_RUNTIME_INSTANCE_ID="rtinst_xxx"
   export PONTIA_CURRENT_TURN_FILE="$PONTIA_RUNTIME_DIR/current-turn.json"
   export PONTIA_INTERNAL_EVENT_URL="http://127.0.0.1:8080/internal/v1/events"
   export PONTIA_PI_HOOK_LOG="$PONTIA_RUNTIME_DIR/pi-hook.log"
   ```

4. Run a real pi session:

   ```bash
   pi --approve -e ./clients/pi
   ```

5. Submit a prompt and verify pontia received `turn.output` and `turn.completed` through its event list/API or database inspection.

6. If reporting fails, inspect diagnostics:

   ```bash
   tail -f "$PONTIA_RUNTIME_DIR/pi-hook.log"
   ```
