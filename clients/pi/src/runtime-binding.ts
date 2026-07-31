import type { EnvLike } from "./context.js";
import { resolvePontiaConnection } from "./discovery.js";
import { asRecord, optionalString, parseJsonResponse } from "./internal-api.js";
import type { SessionContext } from "./session.js";

export type PiSessionDetails = Pick<SessionContext, "clientSessionKey" | "clientSessionFile" | "clientSessionDir" | "clientCwd">;

function callSessionManagerString(sessionManager: unknown, method: string): string | undefined {
  if (!sessionManager || typeof sessionManager !== "object") return undefined;
  const fn = (sessionManager as Record<string, unknown>)[method];
  if (typeof fn !== "function") return undefined;
  try {
    return optionalString(fn.call(sessionManager));
  } catch {
    return undefined;
  }
}

export function piSessionDetailsFromHookContext(ctx: unknown): PiSessionDetails {
  const sessionManager = ctx && typeof ctx === "object" ? (ctx as Record<string, unknown>).sessionManager : undefined;
  return {
    clientSessionKey: callSessionManagerString(sessionManager, "getSessionId"),
    clientSessionFile: callSessionManagerString(sessionManager, "getSessionFile"),
    clientSessionDir: callSessionManagerString(sessionManager, "getSessionDir"),
    clientCwd: callSessionManagerString(sessionManager, "getCwd"),
  };
}

function agentBindingSessionContextUrl(discoveredBindingUpsertUrl?: string): string | undefined {
  return discoveredBindingUpsertUrl?.replace(
    /\/runtime-bindings\/upsert\/?$/,
    "/agent-bindings/session-context",
  );
}

function tmuxBindingFromEnv(env: EnvLike): { socket_path: string; pane_id: string } | undefined {
  const tmux = optionalString(env.TMUX);
  const paneId = optionalString(env.TMUX_PANE);
  const socketPath = optionalString(tmux?.split(",", 1)[0]);
  if (!socketPath || !paneId) return undefined;
  return { socket_path: socketPath, pane_id: paneId };
}

export async function bindSession(
  env: EnvLike,
  fetchImpl: typeof fetch,
  sessionDetails: PiSessionDetails,
  options: { startKind?: "fork"; parentSessionId?: string } = {},
): Promise<SessionContext | undefined> {
  if (!sessionDetails.clientSessionKey) return undefined;
  const discovered = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const url = discovered?.bindingUpsertUrl;
  if (!url) return undefined;

  const tmux = tmuxBindingFromEnv(env);
  const response = await fetchImpl(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      client_type: "pi",
      client_session_key: sessionDetails.clientSessionKey,
      client_session_file: sessionDetails.clientSessionFile,
      client_session_dir: sessionDetails.clientSessionDir,
      client_cwd: sessionDetails.clientCwd,
      launch_cwd: sessionDetails.clientCwd,
      start_command: "pi",
      ...(options.startKind ? { start_kind: options.startKind } : {}),
      ...(options.parentSessionId ? { parent_session_id: options.parentSessionId } : {}),
      ...(tmux ? { tmux } : {}),
    }),
  });
  const body = await parseJsonResponse(response);
  if (!response.ok) throw new Error(`runtime binding upsert failed: ${response.status} ${response.statusText}`);

  const record = asRecord(body);
  const session = asRecord(record?.session);
  const runtime = asRecord(record?.runtime);
  const sessionId = optionalString(session?.session_id);
  const resolvedRuntimeInstanceId = optionalString(runtime?.runtime_instance_id);
  const internalEventUrl = optionalString(runtime?.internal_event_url) ?? discovered?.internalEventUrl;
  if (!sessionId) throw new Error("runtime binding upsert response missing session.session_id");
  if (!resolvedRuntimeInstanceId) throw new Error("runtime binding upsert response missing runtime.runtime_instance_id");
  if (!internalEventUrl) throw new Error("runtime binding upsert response missing runtime.internal_event_url");
  return {
    sessionId,
    clientType: "pi",
    internalEventUrl,
    runtimeInstanceId: resolvedRuntimeInstanceId,
    ...sessionDetails,
  };
}

export interface ExistingPiSessionContext extends SessionContext {
  sessionState: string;
}

export async function loadExistingSessionContext(
  env: EnvLike,
  fetchImpl: typeof fetch,
  sessionDetails: PiSessionDetails,
): Promise<ExistingPiSessionContext | undefined> {
  if (!sessionDetails.clientSessionKey) return undefined;
  const discovered = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const baseUrl = agentBindingSessionContextUrl(discovered?.bindingUpsertUrl);
  if (!baseUrl) return undefined;
  const url = new URL(baseUrl);
  url.searchParams.set("client_type", "pi");
  url.searchParams.set("client_session_key", sessionDetails.clientSessionKey);
  const response = await fetchImpl(url.toString());
  if (response.status === 404) return undefined;
  const body = await parseJsonResponse(response);
  if (!response.ok) throw new Error(`agent binding session context lookup failed: ${response.status} ${response.statusText}`);

  const record = asRecord(asRecord(asRecord(body)?.data)?.session_context);
  const sessionId = optionalString(record?.session_id);
  const sessionState = optionalString(record?.session_state);
  const clientType = optionalString(record?.client_type);
  const runtimeInstanceId = optionalString(record?.runtime_instance_id);
  const internalEventUrl = optionalString(record?.internal_event_url) ?? discovered?.internalEventUrl;
  if (!sessionId || !sessionState || clientType !== "pi" || !runtimeInstanceId || !internalEventUrl) {
    throw new Error("agent binding session context lookup returned an invalid context");
  }
  return {
    sessionId,
    sessionState,
    clientType: "pi",
    runtimeInstanceId,
    internalEventUrl,
    ...sessionDetails,
  };
}
