import { randomUUID } from "node:crypto";
import { realpathSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { Options } from "@anthropic-ai/claude-agent-sdk";

export const restrictedTools = ["Read", "Bash"];

export function restrictedOptions(home: string, cwd: string, sessionId: string): Options {
  const settings = {
    disableAllHooks: true,
    permissions: {
      defaultMode: "bypassPermissions" as const,
      deny: ["Skill", "mcp__*"],
    },
    sandbox: { enabled: false },
    // Otherwise the SDK can advertise DeferredToolPlaceholder alongside Read and Bash.
    env: { PONTIA_SESSION_ID: sessionId, ENABLE_TOOL_SEARCH: "false" },
  };

  const root = realpathSync(home);
  const workspace = realpathSync(cwd);
  // Explicitly restricted standalone Sessions can run in a user Workspace.
  const directory = dirname(workspace) === join(root, "bot") ? workspace : join(root, "state");
  // Retrieval and Dreaming share a directory, but their tool environments are per Session.
  const path = join(directory, `.pontia-claude-${encodeURIComponent(sessionId)}.json`);
  const temporary = `${path}.${randomUUID()}.tmp`;
  try {
    writeFileSync(temporary, JSON.stringify(settings, null, 2) + "\n", { flag: "wx", mode: 0o600 });
    renameSync(temporary, path);
  } finally {
    rmSync(temporary, { force: true });
  }

  return {
    settings: path,
    tools: restrictedTools,
    settingSources: [],
    // Managed settings can ignore file-based deny rules or override defaultMode.
    disallowedTools: settings.permissions.deny,
    permissionMode: settings.permissions.defaultMode,
    allowDangerouslySkipPermissions: true,
    strictMcpConfig: true,
    mcpServers: {},
    extraArgs: { "disable-slash-commands": null },
  };
}
