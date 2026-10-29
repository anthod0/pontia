import { randomUUID } from "node:crypto";
import type { CanUseTool, PermissionResult } from "@anthropic-ai/claude-agent-sdk";
import type { Emit } from "./facts.js";

// Native callbacks stay in memory and are never reconstructed from persisted Interactions.
export class Permissions {
  private readonly pending = new Map<string, (option?: "allow" | "deny") => void>();

  constructor(private readonly emit: Emit) {}

  request: CanUseTool = (tool, _input, options) => {
    if (tool === "AskUserQuestion" || options.signal.aborted) {
      return Promise.resolve({
        behavior: "deny",
        message: "This interaction is unsupported or expired.",
      });
    }
    const id = randomUUID();
    return new Promise<PermissionResult>((resolve) => {
      const abort = () => finish();
      const finish = (option?: "allow" | "deny") => {
        if (!this.pending.delete(id)) return;
        options.signal.removeEventListener("abort", abort);
        this.emit({
          kind: "permission_settled",
          permission_id: id,
          outcome: option ? "resolved" : "expired",
        });
        resolve(
          option === "allow"
            ? { behavior: "allow" }
            : {
                behavior: "deny",
                message:
                  option === "deny" ? "User denied this tool use." : "Tool approval expired.",
              },
        );
      };
      this.pending.set(id, finish);
      options.signal.addEventListener("abort", abort, { once: true });
      // No arguments, paths, SDK payloads or permission-rule suggestions enter storage.
      // A sub-agent callback is still an ordinary Session Interaction, but does not
      // establish that the foreground Agent is blocked.
      this.emit({
        kind: "permission_requested",
        permission_id: id,
        tool: tool.slice(0, 256),
        blocking: !options.agentID,
      });
    });
  };

  resolve(id: string, option: string): boolean {
    const finish = this.pending.get(id);
    if (!finish || (option !== "allow" && option !== "deny")) return false;
    finish(option);
    return true;
  }

  expire(): void {
    for (const finish of this.pending.values()) finish();
  }
}
