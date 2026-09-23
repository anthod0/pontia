import { join } from "node:path";
import type { PiConnection } from "./control-socket.js";
import { pontiaHomeFromEnv } from "./discovery.js";
import type { SessionContext } from "./session.js";

export interface TurnContext {
  sessionId: string;
  turnId?: string;
  runtimeInstanceId: string;
  input?: string;
  inboxMessageId?: string;
  clientType: "pi";
}

export type LoadTurnContextResult =
  | { ok: true; context: TurnContext; logFile: string }
  | { ok: false; reason: string; logFile?: string; silent?: boolean };

export type EnvLike = Record<string, string | undefined>;

export interface LoadTurnContextOptions {
  connection?: Pick<PiConnection, "request">;
  sessionContext?: SessionContext;
}

export function defaultHookLogFile(pontiaHome: string): string {
  return join(pontiaHome, "state", "pi-hook.log");
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function contextFromRecord(record: Record<string, unknown>, logFile: string): LoadTurnContextResult {
  const errors: string[] = [];
  const sessionId = optionalString(record.session_id);
  const turnId = optionalString(record.turn_id);
  const clientType = optionalString(record.client_type);
  const runtimeInstanceId = optionalString(record.runtime_instance_id);
  const input = optionalString(record.input);
  const inboxMessageId = optionalString(record.inbox_message_id);

  if (!sessionId) errors.push("session_id is required");
  if (clientType !== "pi") errors.push("client_type must be pi");
  if (!runtimeInstanceId) errors.push("runtime_instance_id is required");

  if (errors.length > 0) {
    return { ok: false, reason: errors.join("; "), logFile };
  }

  return {
    ok: true,
    logFile,
    context: {
      sessionId: sessionId!,
      turnId,
      runtimeInstanceId: runtimeInstanceId!,
      input,
      inboxMessageId,
      clientType: "pi",
    },
  };
}

async function claimTurnContext(
  logFile: string,
  connection?: Pick<PiConnection, "request">,
  sessionContext?: SessionContext,
): Promise<LoadTurnContextResult | undefined> {
  if (!sessionContext || !connection) return undefined;
  try {
    const body = await connection.request("turn.claim", {
      session_id: sessionContext.sessionId,
      runtime_instance_id: sessionContext.runtimeInstanceId,
      client_type: "pi",
    });
    const currentTurn = asRecord(asRecord(body)?.current_turn);
    if (!currentTurn) return { ok: false, reason: "no pending current turn", logFile, silent: true };
    return contextFromRecord(currentTurn, logFile);
  } catch {
    return undefined;
  }
}

export async function loadTurnContext(env: EnvLike = process.env, options: LoadTurnContextOptions = {}): Promise<LoadTurnContextResult> {
  const pontiaHome = pontiaHomeFromEnv(env);
  if (!pontiaHome) {
    return { ok: false, reason: "Pontia home must resolve to a non-root absolute path", silent: true };
  }
  const logFile = defaultHookLogFile(pontiaHome);
  const claimed = await claimTurnContext(logFile, options.connection, options.sessionContext);
  if (claimed) return claimed;
  return { ok: false, reason: "current turn claim unavailable", logFile, silent: true };
}
