# llmparty Claude Code plugin

Reports Claude Code startup readiness and confirmed turn facts to llmparty through `/internal/v1/events`.

## Installation

Install the llmparty marketplace from GitHub, then install the Claude Code plugin:

```bash
claude plugin marketplace add anthod0/llmparty --sparse .claude-plugin clients/claude-code
claude plugin install llmparty-claude-code@llmparty
```

After installing or updating the plugin, reload plugins inside Claude Code if needed:

```text
/reload-plugins
```

When the plugin is installed from the marketplace, configure llmparty to launch Claude Code normally:

```bash
LLMPARTY_CLAUDE_TUI_COMMAND=claude
```

## Local development

```bash
pnpm install
pnpm test
pnpm typecheck
```

On `SessionStart` startup, the hook reads `LLMPARTY_SESSION_ID`, `LLMPARTY_RUNTIME_INSTANCE_ID`, and `LLMPARTY_INTERNAL_EVENT_URL` to post a one-time `session.ready` signal from `agent_client`.

For turn completion hooks, it reads `LLMPARTY_CURRENT_TURN_FILE`, posts to `LLMPARTY_INTERNAL_EVENT_URL` or the context file URL, and writes JSONL diagnostics to `LLMPARTY_CLAUDE_HOOK_LOG`.
