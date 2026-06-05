# @pilotfy/pi-client-plugin

First-party pi extension for reporting pi startup readiness and confirmed turn facts back to pilotfy.

## Install locally

From the pilotfy repository root, run pi with this package as a temporary extension:

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
| `PILOTFY_WORKSPACE` | recommended | pi process cwd |
| `PILOTFY_RUNTIME_DIR` | recommended | pi process cwd |
| `PILOTFY_SESSION_ID` | required for startup ready | none |
| `PILOTFY_RUNTIME_INSTANCE_ID` | required for startup ready | none |
| `PILOTFY_CURRENT_TURN_FILE` | recommended | `$PILOTFY_RUNTIME_DIR/current-turn.json` |
| `PILOTFY_INTERNAL_EVENT_URL` | required for startup ready, required for turns unless present in context file | none |
| `PILOTFY_PI_HOOK_LOG` | recommended | `$PILOTFY_RUNTIME_DIR/pi-hook.log` |

Expected `current-turn.json`:

```json
{
  "session_id": "sess_xxx",
  "turn_id": "turn_xxx",
  "input": "user task",
  "client_type": "pi",
  "runtime_instance_id": "rtinst_xxx",
  "internal_event_url": "http://127.0.0.1:8080/internal/v1/events"
}
```

`session_id`, `turn_id`, `runtime_instance_id`, and `client_type: "pi"` are required. `PILOTFY_INTERNAL_EVENT_URL` and `PILOTFY_RUNTIME_INSTANCE_ID` override file values when present.

## What the extension reports

- On `session_start` with reason `startup`, it posts a one-time `session.ready` signal from `agent_client` with the current `runtime_instance_id`.
- On `agent_start`, it reads the current turn context.
- On assistant message updates/end events, it collects assistant-visible text from pi lifecycle event payloads.
- On `agent_end`, it posts `turn.output` when text was collected, then posts `turn.completed`.
- If pi exposes an explicit agent-end error, it posts `turn.failed`.

The extension does not parse TUI screen contents and does not infer completion from tmux, process state, or runtime exit.

## pilotfy tools

The pi extension registers four agent-visible DAG tools from `clients/tools/pilotfy-tools.v1.json`:

- `getContext`
- `submitPlan`
- `submitResult`
- `raiseSignal`

Each tool handler reads the current turn context from `PILOTFY_CURRENT_TURN_FILE` / environment, builds `{ session_id, turn_id, runtime_instance_id, input }`, and forwards it to `/internal/v1/agent-tools/{tool}`. The extension does not interpret DAG business logic and never accepts task, WorkItem, or run IDs as authority; pilotfy derives authorization server-side.

Backend errors are returned to the agent as clear tool failures and written to `PILOTFY_PI_HOOK_LOG` diagnostics. Environment values such as API tokens are not included in agent-visible tool results.

## Manual validation

When pi is launched by pilotfy `client_type = "pi"` runtime, the Control Plane writes `current-turn.json` under the global runtime directory and exports `PILOTFY_SESSION_ID`, `PILOTFY_RUNTIME_INSTANCE_ID`, `PILOTFY_RUNTIME_DIR`, `PILOTFY_CURRENT_TURN_FILE`, `PILOTFY_INTERNAL_EVENT_URL`, and `PILOTFY_PI_HOOK_LOG` for the hook. The steps below are useful for standalone plugin validation.

1. Start pilotfy so `/internal/v1/events` is reachable.
2. Create the current turn file in a runtime directory:

   ```bash
   export PILOTFY_RUNTIME_DIR="$HOME/.local/share/pilotfy/runtimes/manual-pi"
   mkdir -p "$PILOTFY_RUNTIME_DIR"
   cat > "$PILOTFY_RUNTIME_DIR/current-turn.json" <<'JSON'
   {
     "session_id": "sess_xxx",
     "turn_id": "turn_xxx",
     "input": "hello",
     "client_type": "pi",
     "runtime_instance_id": "rtinst_xxx",
     "internal_event_url": "http://127.0.0.1:8080/internal/v1/events"
   }
   JSON
   ```

3. Export environment for the pi process:

   ```bash
   export PILOTFY_WORKSPACE="$PWD"
   export PILOTFY_SESSION_ID="sess_xxx"
   export PILOTFY_RUNTIME_INSTANCE_ID="rtinst_xxx"
   export PILOTFY_CURRENT_TURN_FILE="$PILOTFY_RUNTIME_DIR/current-turn.json"
   export PILOTFY_INTERNAL_EVENT_URL="http://127.0.0.1:8080/internal/v1/events"
   export PILOTFY_PI_HOOK_LOG="$PILOTFY_RUNTIME_DIR/pi-hook.log"
   ```

4. Run a real pi session:

   ```bash
   pi -e ./clients/pi
   ```

5. Submit a prompt and verify pilotfy received `turn.output` and `turn.completed` through its event list/API or database inspection. In DAG-managed turns, ask pi to call `getContext`, `submitPlan`, `submitResult`, or `raiseSignal` and verify the backend receives `/internal/v1/agent-tools/*` requests.

6. If reporting or tool forwarding fails, inspect diagnostics:

   ```bash
   tail -f "$PILOTFY_RUNTIME_DIR/pi-hook.log"
   ```
