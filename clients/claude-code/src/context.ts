import { defaultHookLogFile } from "./session.js";

export interface TurnContext {
  sessionId: string;
  turnId: string;
  input?: string;
  clientType: "claude_code";
  internalEventUrl: string;
}

export type LoadTurnContextResult = { ok: false; reason: string; logFile?: string; silent: true };

export type EnvLike = Record<string, string | undefined>;

export async function loadTurnContext(env: EnvLike = process.env): Promise<LoadTurnContextResult> {
  return {
    ok: false,
    reason: "Claude Code turn hooks are disabled",
    logFile: defaultHookLogFile(env),
    silent: true,
  };
}
