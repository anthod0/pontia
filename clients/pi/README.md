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

On `session_start`, the extension first tries the managed pre-bound runtime environment (`PONTIA_SESSION_ID`, `PONTIA_RUNTIME_INSTANCE_ID`, and `PONTIA_HOME`). It resolves the backend from `$PONTIA_HOME/config.toml` and reports against that existing pontia session.

On startup, the extension first verifies that the pi cwd is an active, explicitly registered pontia workspace through the External workspace API. If the workspace is missing, deleted, or pontia cannot be reached for this check, the extension disables pontia reporting for that pi process.

When `PONTIA_SESSION_ID` is absent and the active workspace check passes, the extension defers binding a manually started pi TUI until the first real `agent_start`. This matches pi's behavior where a startup-only client session can be discarded when the user exits without sending a prompt, so pontia does not persist an empty session just because pi opened. On first turn, the extension discovers a local pontia server from `${PONTIA_HOME:-$HOME/.pontia}/config.toml` and calls the Internal runtime binding upsert API. The backend creates or reuses the pontia `session_id`; the extension only reports the real pi `client_session_key` from `ctx.sessionManager.getSessionId()`. If pontia cannot be discovered or reached, the extension skips pontia reporting.

## Runtime environment

The extension reads runtime context from environment variables and, for manually started sessions, can discover the pontia server from `${PONTIA_HOME:-$HOME/.pontia}/config.toml`:

| Variable | Required | Default |
| --- | --- | --- |
| `PONTIA_WORKSPACE` | recommended | pi process cwd |
| `PONTIA_HOME` | optional | `$HOME/.pontia` |
| `PONTIA_SESSION_ID` | required for reporting | none |
| `PONTIA_RUNTIME_INSTANCE_ID` | required for reporting | none |
| hook log | derived | `$PONTIA_HOME/state/pi-hook.log` |

Backend-delivered input is claimed through the Internal API endpoint discovered from `$PONTIA_HOME/config.toml`: `/internal/v1/sessions/{session_id}/current-turn/claim`. `PONTIA_SESSION_ID`, `PONTIA_RUNTIME_INSTANCE_ID`, and `PONTIA_HOME` are required for this pre-bound claim path. `inbox_message_id`, when present in the claim response, is used to link backend-delivered input to the real turn after `agent_start`. `turn_id` is intentionally omitted for pi: the plugin generates the authoritative pontia turn id when pi reports a real `agent_start`.

The extension is intentionally input-source agnostic at the hook layer. Backend-dispatched input reaches pi through the same real TUI and produces the same `agent_start` lifecycle signal as manually typed TUI input. The claim response is correlation metadata only; it must not determine whether a hook-observed prompt becomes a pontia turn.

## What the extension reports

- On `session_start` with reason `startup`, it posts a one-time `session.ready` signal from `agent_client` for pre-bound managed sessions with the current `runtime_instance_id` plus the real pi session identity from `ctx.sessionManager.getSessionId()` as `client_session_key`. Manual pi TUI sessions defer binding and `session.ready` until the first `agent_start`.
- On `agent_start`, it claims pending input context from the Internal API, generates a fresh pontia `turn_id`, and posts `turn.started` with any `inbox_message_id` metadata.
- On assistant message updates/end events, it collects assistant-visible text from pi lifecycle event payloads.
- When pi exposes context usage through hook events, message usage, or `ctx.getContextUsage()`, it posts `session.context_usage_updated`; it does not parse session files or fabricate usage when pi does not provide it.
- On `agent_end`, it posts `turn.output` when text was collected, then posts `turn.completed` for the plugin-generated turn id.
- If pi exposes an explicit agent-end error, it posts `turn.failed`.
- On `session_shutdown` with reason `quit`, `new`, `resume`, or `fork`, it posts `session.exited` from the pi lifecycle hook. `reload` is ignored because it does not represent the pi session being detached from the active runtime.

The extension does not parse TUI screen contents and does not infer turn completion from tmux, process state, or runtime exit.

## Manual validation

When pi is launched by pontia `client_type = "pi"` runtime, the Control Plane exports `PONTIA_SESSION_ID`, `PONTIA_RUNTIME_INSTANCE_ID`, and `PONTIA_HOME` for the hook. Backend-delivered input is made available through the Internal current-turn claim API discovered from `$PONTIA_HOME/config.toml`. The steps below are useful for standalone plugin validation.

1. Start pontia so `/internal/v1/events` and the current-turn claim API are reachable.
2. Export environment for the pi process:

   ```bash
   export PONTIA_HOME="$HOME/.pontia"
   mkdir -p "$PONTIA_HOME/state"
   export PONTIA_WORKSPACE="$PWD"
   export PONTIA_SESSION_ID="sess_xxx"
   export PONTIA_RUNTIME_INSTANCE_ID="rtinst_xxx"
   ```

3. Run a real pi session:

   ```bash
   pi --approve -e ./clients/pi
   ```

4. Submit a prompt and verify pontia received `turn.output` and `turn.completed` through its event list/API or database inspection.

5. If reporting fails, inspect diagnostics:

   ```bash
   tail -f "$PONTIA_HOME/state/pi-hook.log"
   ```
