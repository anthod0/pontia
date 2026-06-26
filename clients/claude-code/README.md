# pontia Claude Code plugin

Reports Claude Code startup readiness to pontia through `/internal/v1/events`.

## Installation

Install the pontia marketplace from GitHub, then install the Claude Code plugin:

```bash
claude plugin marketplace add anthod0/pontia --sparse .claude-plugin clients/claude-code
claude plugin install pontia-claude-code@pontia
```

After installing or updating the plugin, reload plugins inside Claude Code if needed:

```text
/reload-plugins
```

When the plugin is installed from the marketplace, pontia launches Claude Code with its default command:

```bash
claude --dangerously-skip-permissions
```

To override the launch command, set `PONTIA_CLAUDE_TUI_COMMAND`.

## Local development

```bash
pnpm install
pnpm test
pnpm typecheck
```

On `SessionStart` startup, the hook reads `PONTIA_SESSION_ID`, `PONTIA_RUNTIME_INSTANCE_ID`, `PONTIA_INTERNAL_EVENT_URL`, and `PONTIA_HOME` to post a one-time `session.ready` signal from `agent_client`; diagnostics are written to `$PONTIA_HOME/state/claude-hook.log`.

Claude Code turn hooks are currently disabled.
