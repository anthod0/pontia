import { homedir } from "node:os";
import { join } from "node:path";
import { resolvePontiaConnection } from "./discovery.js";
import type { SessionContext } from "./session.js";

export interface TurnContext {
  sessionId: string;
  turnId?: string;
  runtimeInstanceId: string;
  input?: string;
  inboxMessageId?: string;
  clientType: "pi";
  internalEventUrl: string;
}

export type LoadTurnContextResult =
  | { ok: true; context: TurnContext; logFile: string }
  | { ok: false; reason: string; logFile: string; silent?: boolean };

export type EnvLike = Record<string, string | undefined>;

export interface LoadTurnContextOptions {
  fetch?: typeof fetch;
  sessionContext?: SessionContext;
}

function fallbackLogDir(env: EnvLike = process.env): string {
  return join(env.PONTIA_HOME ?? join(env.HOME ?? homedir(), ".pontia"), "state");
}

export function defaultHookLogFile(env: EnvLike = process.env): string {
  return join(fallbackLogDir(env), "pi-hook.log");
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function claimUrl(internalEventUrl: string, sessionId: string): string | undefined {
  try {
    const url = new URL(internalEventUrl);
    url.pathname = url.pathname.replace(/\/events\/?$/, `/sessions/${encodeURIComponent(sessionId)}/current-turn/claim`);
    return url.toString();
  } catch {
    return undefined;
  }
}

function contextFromRecord(record: Record<string, unknown>, logFile: string, discoveredInternalEventUrl?: string): LoadTurnContextResult {
  const errors: string[] = [];
  const sessionId = optionalString(record.session_id);
  const turnId = optionalString(record.turn_id);
  const clientType = optionalString(record.client_type);
  const internalEventUrl = optionalString(record.internal_event_url) ?? discoveredInternalEventUrl;
  const runtimeInstanceId = optionalString(record.runtime_instance_id);
  const input = optionalString(record.input);
  const inboxMessageId = optionalString(record.inbox_message_id);

  if (!sessionId) errors.push("session_id is required");
  if (clientType !== "pi") errors.push("client_type must be pi");
  if (!internalEventUrl) errors.push("internal_event_url or pontia connection from PONTIA_HOME/config.toml is required");
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
      internalEventUrl: internalEventUrl!,
    },
  };
}

async function claimTurnContext(
  env: EnvLike,
  logFile: string,
  fetchImpl: typeof fetch,
  sessionContext?: SessionContext,
): Promise<LoadTurnContextResult | undefined> {
  if (!sessionContext) return undefined;
  const connection = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const internalEventUrl = sessionContext.internalEventUrl ?? connection?.internalEventUrl;
  const url = claimUrl(internalEventUrl, sessionContext.sessionId);
  if (!url) return undefined;

  try {
    const response = await fetchImpl(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ runtime_instance_id: sessionContext.runtimeInstanceId, client_type: "pi" }),
    });
    if (!response.ok) return undefined;
    const body = (await response.json()) as unknown;
    const data = asRecord(body)?.data;
    const currentTurn = asRecord(asRecord(data)?.current_turn);
    if (!currentTurn) return { ok: false, reason: "no pending current turn", logFile, silent: true };
    return contextFromRecord(currentTurn, logFile, internalEventUrl);
  } catch {
    return undefined;
  }
}

export async function loadTurnContext(env: EnvLike = process.env, options: LoadTurnContextOptions = {}): Promise<LoadTurnContextResult> {
  const logFile = defaultHookLogFile(env);
  const claimed = await claimTurnContext(env, logFile, options.fetch ?? fetch, options.sessionContext);
  if (claimed) return claimed;
  return { ok: false, reason: "current turn claim unavailable", logFile, silent: true };
}
