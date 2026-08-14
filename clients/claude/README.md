# Pontia Claude Code integration

This plugin is the shared lifecycle and approval integration for Claude Code
sessions started by Pontia and for manually started Claude Code sessions in a
tmux pane and an active registered workspace. Outside tmux, Pontia hooks are a
silent no-op. Hooks identify the Pontia Session through Claude's native
`session_id` (`client_session_key`), never through tmux pane markers. A second
TUI for a native key already bound to a non-exited Session is ignored.

Install and enable the `pontia-claude` plugin at user scope so Claude Code loads
the `SessionStart`, `UserPromptSubmit`, `PermissionRequest`, `Stop`,
`StopFailure`, and `SessionEnd` hooks declared in `hooks/hooks.json`.

Pontia does not read or modify Claude user settings. Hooks load exclusively
from the separately installed plugin. An `external_api_token` must be configured
in `$PONTIA_HOME/config.toml` (or `PONTIA_EXTERNAL_API_TOKEN`) for authenticated
Pontia requests.

## Local approval verification

Use both a Pontia-started Claude TUI and a manually started Claude TUI in tmux
whose working directory is an active registered Pontia workspace. Verify:

1. `SessionStart` binds each native Claude session to a Pontia Session.
2. Trigger a permission request and exercise Dashboard **Accept once**,
   **Always allow**, and **Reject**.
3. Trigger separate requests and exercise native TUI accept, reject, and abort.
4. After a Dashboard command succeeds, confirm the Session remains
   `interaction.state = "awaiting"` until Claude emits the final
   `claude_code.tool_decision` observation.
5. Confirm the final `approval.accepted`, `approval.rejected`, or
   `approval.cancelled` event, accepted scope, and cleared Session interaction
   match Claude's decision.
6. From an unregistered workspace, confirm the hook and OTLP records are no-op.

Pontia does not use tmux screen contents, terminal keys, transcripts, runtime
logs, or process state to infer an approval result.
