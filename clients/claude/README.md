# Pontia Claude Code integration

This plugin is the shared lifecycle and approval integration for Claude Code
sessions started by Pontia and for manually started Claude Code sessions in an
active registered workspace.

Start or restart Pontia before starting Claude Code. Pontia merges the required
logs-only OpenTelemetry configuration into `~/.claude/settings.json`, preserving
unrelated settings, hooks, permissions, and environment entries. An
`external_api_token` must be configured in `$PONTIA_HOME/config.toml` (or
`PONTIA_EXTERNAL_API_TOKEN`) so Claude can authenticate to the fixed internal
OTLP logs receiver.

Pontia also installs the same hook implementation shipped in this directory
under `$PONTIA_HOME/integrations/claude` and merges its `SessionStart`,
`UserPromptSubmit`, `PermissionRequest`, `Stop`, `StopFailure`, and `SessionEnd`
entries into the user settings. Both managed and manual Claude TUI sessions
therefore load the same hook without a project-specific installation step.

The generated configuration explicitly disables metrics, traces, prompts,
assistant responses, tool details/content, and raw API body capture. It exports
Claude events only, over OTLP HTTP/JSON, to Pontia's loopback receiver.

## Local approval verification

Use both a Pontia-started Claude TUI and a manually started Claude TUI whose
working directory is an active registered Pontia workspace. Verify:

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
