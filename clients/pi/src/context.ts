import { join } from "node:path";

export interface TurnContext {
  sessionId: string;
  turnId?: string;
  runtimeInstanceId: string;
  input?: string;
  inboxMessageId?: string;
  clientType: "pi";
}

export type EnvLike = Record<string, string | undefined>;

export function defaultHookLogFile(pontiaHome: string): string {
  return join(pontiaHome, "state", "pi-hook.log");
}
