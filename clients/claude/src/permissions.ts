import { createHash } from "node:crypto";
import type { CanUseTool, PermissionResult } from "@anthropic-ai/claude-agent-sdk";
import type { Emit } from "./facts.js";

type Pending = {
  digest: string;
  answer: Promise<PermissionResult>;
  finish: (option?: "allow" | "deny") => void;
};

// Object member order is immaterial. Array order, tool arguments and SDK permission
// context are part of the authorization. The signal and correlation ID are transport state.
function digest(tool: string, input: Record<string, unknown>, context: object): string {
  const content = JSON.stringify({ tool, input, context }, (_key, value: unknown) => {
    if (typeof value !== "object" || value === null || Array.isArray(value)) return value;
    return Object.fromEntries(
      Object.entries(value).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
    );
  });
  return createHash("sha256").update(content).digest("hex");
}

export class Permissions {
  private readonly pending = new Map<string, Pending>();
  constructor(private readonly emit: Emit) {}

  request: CanUseTool = (tool, input, { signal, requestId: id, ...context }) => {
    if (tool === "AskUserQuestion" || signal.aborted) {
      return Promise.resolve({
        behavior: "deny",
        message: "This interaction is unsupported or expired.",
      });
    }
    const authorization_digest = digest(tool, input, context);
    const requested = {
      kind: "permission_requested" as const,
      permission_id: id,
      tool: tool.slice(0, 256),
      blocking: !context.agentID,
      authorization_digest,
    };
    const existing = this.pending.get(id);
    if (existing) {
      if (existing.digest !== authorization_digest) {
        this.emit(requested);
        existing.finish();
      }
      return existing.answer;
    }
    let settle!: (result: PermissionResult) => void;
    const answer = new Promise<PermissionResult>((resolve) => {
      settle = resolve;
    });
    const abort = () => finish();
    const finish = (option?: "allow" | "deny") => {
      if (!this.pending.delete(id)) return;
      signal.removeEventListener("abort", abort);
      this.emit({
        kind: "permission_settled",
        permission_id: id,
        outcome: option ? "resolved" : "expired",
      });
      settle(
        option === "allow"
          ? { behavior: "allow" }
          : {
              behavior: "deny",
              message: option === "deny" ? "User denied this tool use." : "Tool approval expired.",
            },
      );
    };
    this.pending.set(id, { digest: authorization_digest, answer, finish });
    signal.addEventListener("abort", abort, { once: true });
    this.emit(requested);
    return answer;
  };

  resolve(id: string, option: string): boolean {
    const pending = this.pending.get(id);
    if (!pending || (option !== "allow" && option !== "deny")) return false;
    pending.finish(option);
    return true;
  }

  has(id: string): boolean {
    return this.pending.has(id);
  }
  cancel(id: string): void {
    this.pending.get(id)?.finish();
  }
  expire(): void {
    for (const pending of this.pending.values()) pending.finish();
  }
}
