import { homedir } from "node:os";
import { join } from "node:path";
import { resolvePontiaConnection } from "./discovery.js";
import { asRecord, optionalString } from "./internal-api.js";

export type EnvLike = Record<string, string | undefined>;

export interface TurnContext {
  sessionId: string;
  turnId?: string;
  runtimeInstanceId: string;
  input?: string;
  clientType: "claude";
  internalEventUrl: string;
}

export interface SessionContext {
  sessionId: string;
  sessionState?: string;
  clientType: "claude";
  internalEventUrl: string;
  runtimeInstanceId: string;
  clientSessionKey?: string;
  transcriptPath?: string;
  clientCwd?: string;
}

export type LoadTurnContextResult =
  | { ok: true; context: TurnContext; logFile: string }
  | { ok: false; reason: string; logFile: string; silent?: boolean };

export type LoadSessionContextResult =
  | { ok: true; context: SessionContext; logFile: string }
  | { ok: false; reason: string; logFile: string };

function fallbackLogDir(env: EnvLike = process.env): string {
  return join(env.PONTIA_HOME ?? join(env.HOME ?? homedir(), ".pontia"), "state");
}

export function defaultHookLogFile(env: EnvLike = process.env): string {
  return join(fallbackLogDir(env), "claude-hook.log");
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

function agentBindingContextUrl(internalEventUrl: string, path: string, clientSessionKey: string): string | undefined {
  try {
    const url = new URL(internalEventUrl);
    url.pathname = url.pathname.replace(/\/events\/?$/, `/agent-bindings/${path}`);
    url.searchParams.set("client_type", "claude");
    url.searchParams.set("client_session_key", clientSessionKey);
    return url.toString();
  } catch {
    return undefined;
  }
}

function contextFromRecord(record: Record<string, unknown>, env: EnvLike, logFile: string, internalEventUrl?: string): LoadTurnContextResult {
  const sessionId = optionalString(record.session_id);
  const turnId = optionalString(record.turn_id);
  const clientType = optionalString(record.client_type);
  const runtimeInstanceId = optionalString(env.PONTIA_RUNTIME_INSTANCE_ID) ?? optionalString(record.runtime_instance_id);
  const resolvedInternalEventUrl = optionalString(record.internal_event_url) ?? internalEventUrl;
  const errors: string[] = [];
  if (!sessionId) errors.push("session_id is required");
  if (clientType !== "claude") errors.push("client_type must be claude");
  if (!runtimeInstanceId) errors.push("runtime_instance_id or PONTIA_RUNTIME_INSTANCE_ID is required");
  if (!resolvedInternalEventUrl) errors.push("internal_event_url is required");
  if (errors.length > 0) return { ok: false, reason: errors.join("; "), logFile };
  return {
    ok: true,
    logFile,
    context: {
      sessionId: sessionId!,
      turnId,
      runtimeInstanceId: runtimeInstanceId!,
      clientType: "claude",
      internalEventUrl: resolvedInternalEventUrl!,
    },
  };
}

export async function loadSessionContext(env: EnvLike = process.env): Promise<LoadSessionContextResult> {
  const logFile = defaultHookLogFile(env);
  const sessionId = optionalString(env.PONTIA_SESSION_ID);
  const runtimeInstanceId = optionalString(env.PONTIA_RUNTIME_INSTANCE_ID);
  const connection = await resolvePontiaConnection({ env });
  const errors: string[] = [];
  if (!sessionId) errors.push("PONTIA_SESSION_ID is required");
  if (!runtimeInstanceId) errors.push("PONTIA_RUNTIME_INSTANCE_ID is required");
  if (!connection?.internalEventUrl) errors.push("pontia connection from PONTIA_HOME/config.toml is required");
  if (errors.length > 0) return { ok: false, reason: errors.join("; "), logFile };
  return {
    ok: true,
    logFile,
    context: { sessionId: sessionId!, runtimeInstanceId: runtimeInstanceId!, clientType: "claude", internalEventUrl: connection!.internalEventUrl },
  };
}

export async function claimTurnContext(env: EnvLike, fetchImpl: typeof fetch): Promise<LoadTurnContextResult> {
  const logFile = defaultHookLogFile(env);
  const sessionId = optionalString(env.PONTIA_SESSION_ID);
  const runtimeInstanceId = optionalString(env.PONTIA_RUNTIME_INSTANCE_ID);
  if (!sessionId || !runtimeInstanceId) return { ok: false, reason: "managed runtime context unavailable", logFile, silent: true };
  const connection = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const internalEventUrl = connection?.internalEventUrl;
  const url = internalEventUrl ? claimUrl(internalEventUrl, sessionId) : undefined;
  if (!url) return { ok: false, reason: "current turn claim unavailable", logFile, silent: true };
  try {
    const response = await fetchImpl(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ runtime_instance_id: runtimeInstanceId, client_type: "claude" }),
    });
    if (!response.ok) return { ok: false, reason: "current turn claim failed", logFile, silent: true };
    const body = await response.json() as unknown;
    const currentTurn = asRecord(asRecord(asRecord(body)?.data)?.current_turn);
    if (!currentTurn) return { ok: false, reason: "no pending current turn", logFile, silent: true };
    return contextFromRecord(currentTurn, env, logFile, internalEventUrl);
  } catch {
    return { ok: false, reason: "current turn claim exception", logFile, silent: true };
  }
}

export async function loadSessionByClientSession(env: EnvLike, fetchImpl: typeof fetch, clientSessionKey: string): Promise<LoadSessionContextResult> {
  const logFile = defaultHookLogFile(env);
  const connection = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const internalEventUrl = connection?.internalEventUrl;
  const url = internalEventUrl ? agentBindingContextUrl(internalEventUrl, "session-context", clientSessionKey) : undefined;
  if (!url) return { ok: false, reason: "session context lookup unavailable", logFile };
  try {
    const response = await fetchImpl(url);
    if (response.status === 404) return { ok: false, reason: "session context not found", logFile };
    if (!response.ok) return { ok: false, reason: "session context lookup failed", logFile };
    const body = await response.json() as unknown;
    const record = asRecord(asRecord(asRecord(body)?.data)?.session_context);
    const sessionId = optionalString(record?.session_id);
    const sessionState = optionalString(record?.session_state);
    const runtimeInstanceId = optionalString(record?.runtime_instance_id);
    const clientType = optionalString(record?.client_type);
    const resolvedInternalEventUrl = optionalString(record?.internal_event_url) ?? internalEventUrl;
    if (!sessionId || !runtimeInstanceId || clientType !== "claude" || !resolvedInternalEventUrl) {
      return { ok: false, reason: "invalid session context lookup response", logFile };
    }
    return {
      ok: true,
      logFile,
      context: {
        sessionId,
        sessionState,
        runtimeInstanceId,
        clientType: "claude",
        internalEventUrl: resolvedInternalEventUrl,
        clientSessionKey,
      },
    };
  } catch {
    return { ok: false, reason: "session context lookup exception", logFile };
  }
}

export async function loadCurrentTurnByClientSession(env: EnvLike, fetchImpl: typeof fetch, clientSessionKey: string): Promise<LoadTurnContextResult> {
  const logFile = defaultHookLogFile(env);
  const connection = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const internalEventUrl = connection?.internalEventUrl;
  const url = internalEventUrl ? agentBindingContextUrl(internalEventUrl, "current-turn", clientSessionKey) : undefined;
  if (!url) return { ok: false, reason: "current turn lookup unavailable", logFile, silent: true };
  try {
    const response = await fetchImpl(url);
    if (!response.ok) return { ok: false, reason: "current turn lookup failed", logFile, silent: true };
    const body = await response.json() as unknown;
    const currentTurn = asRecord(asRecord(asRecord(body)?.data)?.current_turn);
    if (!currentTurn) return { ok: false, reason: "no active current turn", logFile, silent: true };
    return contextFromRecord(currentTurn, env, logFile, internalEventUrl);
  } catch {
    return { ok: false, reason: "current turn lookup exception", logFile, silent: true };
  }
}
