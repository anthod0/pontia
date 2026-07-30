# Pontia Claude Code integration

This plugin is the shared lifecycle and approval integration for Claude Code
sessions started by Pontia and for manually started Claude Code sessions in a
tmux pane and an active registered workspace. Outside tmux, Pontia hooks are a
silent no-op.

Install and enable the `pontia-claude` plugin at user scope so Claude Code loads
the `SessionStart`, `UserPromptSubmit`, `PermissionRequest`, `Stop`,
`StopFailure`, and `SessionEnd` hooks declared in `hooks/hooks.json`.

Start or restart Pontia before starting Claude Code. Pontia merges only the
required logs-only OpenTelemetry configuration into `~/.claude/settings.json`,
preserving unrelated settings, hooks, permissions, and environment entries. It
also removes hook entries created by older Pontia versions under
`$PONTIA_HOME/integrations/claude`; hooks now load exclusively from the plugin.
An `external_api_token` must be configured in `$PONTIA_HOME/config.toml` (or
`PONTIA_EXTERNAL_API_TOKEN`) so Claude can authenticate to the fixed internal
OTLP logs receiver.

The generated configuration explicitly disables metrics, traces, prompts,
assistant responses, tool details/content, and raw API body capture. It exports
Claude events only, over OTLP HTTP/JSON, to Pontia's loopback receiver.

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
