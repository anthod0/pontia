# @llmparty/pi-client-plugin

First-party pi extension for reporting confirmed pi turn facts back to llmparty.

## Install locally

From the llmparty repository root, run pi with this package as a temporary extension:

```bash
pi -e ./clients/pi
```

Or install it into project-local pi settings:

```bash
pi install -l ./clients/pi
```

## Runtime environment

The extension reads configuration from environment variables:

| Variable | Required | Default |
| --- | --- | --- |
| `LLMPARTY_WORKSPACE` | recommended | pi process cwd |
| `LLMPARTY_RUNTIME_DIR` | recommended | pi process cwd |
| `LLMPARTY_CURRENT_TURN_FILE` | recommended | `$LLMPARTY_RUNTIME_DIR/current-turn.json` |
| `LLMPARTY_INTERNAL_EVENT_URL` | required unless present in context file | none |
| `LLMPARTY_PI_HOOK_LOG` | recommended | `$LLMPARTY_RUNTIME_DIR/pi-hook.log` |

Expected `current-turn.json`:

```json
{
  "session_id": "sess_xxx",
  "turn_id": "turn_xxx",
  "input": "user task",
  "client_type": "pi",
  "internal_event_url": "http://127.0.0.1:8080/internal/v1/events"
}
```

`session_id`, `turn_id`, and `client_type: "pi"` are required. `LLMPARTY_INTERNAL_EVENT_URL` overrides `internal_event_url` when both are present.

## What the extension reports

- On `agent_start`, it reads the current turn context.
- On assistant message updates/end events, it collects assistant-visible text from pi lifecycle event payloads.
- On `agent_end`, it posts `turn.output` when text was collected, then posts `turn.completed`.
- If pi exposes an explicit agent-end error, it posts `turn.failed`.

The extension does not parse TUI screen contents and does not infer completion from tmux, process state, or runtime exit.

## Manual validation

When pi is launched by llmparty `client_type = "pi"` runtime, the Control Plane writes `current-turn.json` under the global runtime directory and exports `LLMPARTY_RUNTIME_DIR`, `LLMPARTY_CURRENT_TURN_FILE`, `LLMPARTY_INTERNAL_EVENT_URL`, and `LLMPARTY_PI_HOOK_LOG` for the hook. The steps below are useful for standalone plugin validation.

1. Start llmparty so `/internal/v1/events` is reachable.
2. Create the current turn file in a runtime directory:

   ```bash
   export LLMPARTY_RUNTIME_DIR="$HOME/.local/share/llmparty/runtimes/manual-pi"
   mkdir -p "$LLMPARTY_RUNTIME_DIR"
   cat > "$LLMPARTY_RUNTIME_DIR/current-turn.json" <<'JSON'
   {
     "session_id": "sess_xxx",
     "turn_id": "turn_xxx",
     "input": "hello",
     "client_type": "pi",
     "internal_event_url": "http://127.0.0.1:8080/internal/v1/events"
   }
   JSON
   ```

3. Export environment for the pi process:

   ```bash
   export LLMPARTY_WORKSPACE="$PWD"
   export LLMPARTY_CURRENT_TURN_FILE="$LLMPARTY_RUNTIME_DIR/current-turn.json"
   export LLMPARTY_INTERNAL_EVENT_URL="http://127.0.0.1:8080/internal/v1/events"
   export LLMPARTY_PI_HOOK_LOG="$LLMPARTY_RUNTIME_DIR/pi-hook.log"
   ```

4. Run a real pi session:

   ```bash
   pi -e ./clients/pi
   ```

5. Submit a prompt and verify llmparty received `turn.output` and `turn.completed` through its event list/API or database inspection.

6. If reporting fails, inspect diagnostics:

   ```bash
   tail -f "$LLMPARTY_RUNTIME_DIR/pi-hook.log"
   ```
