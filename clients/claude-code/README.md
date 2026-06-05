# pilotfy Claude Code plugin

Reports Claude Code startup readiness and confirmed turn facts to pilotfy through `/internal/v1/events`.

## Installation

Install the pilotfy marketplace from GitHub, then install the Claude Code plugin:

```bash
claude plugin marketplace add anthod0/pilotfy --sparse .claude-plugin clients/claude-code
claude plugin install pilotfy-claude-code@pilotfy
```

After installing or updating the plugin, reload plugins inside Claude Code if needed:

```text
/reload-plugins
```

When the plugin is installed from the marketplace, pilotfy launches Claude Code with its default command:

```bash
claude --dangerously-skip-permissions
```

To override the launch command, set `PILOTFY_CLAUDE_TUI_COMMAND`.

## Local development

```bash
pnpm install
pnpm test
pnpm typecheck
```

On `SessionStart` startup, the hook reads `PILOTFY_SESSION_ID`, `PILOTFY_RUNTIME_INSTANCE_ID`, and `PILOTFY_INTERNAL_EVENT_URL` to post a one-time `session.ready` signal from `agent_client`.

For turn completion hooks, it reads `PILOTFY_CURRENT_TURN_FILE`, posts to `PILOTFY_INTERNAL_EVENT_URL` or the context file URL, and writes JSONL diagnostics to `PILOTFY_CLAUDE_HOOK_LOG`.
