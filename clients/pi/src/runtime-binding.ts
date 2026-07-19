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

function agentBindingLookupUrl(discoveredBindingUpsertUrl?: string): string | undefined {
  return discoveredBindingUpsertUrl?.replace(/\/runtime-bindings\/upsert\/?$/, "/agent-bindings");
}

function tmuxBindingFromEnv(env: EnvLike): { socket_path: string; pane_id: string } | undefined {
  const tmux = optionalString(env.TMUX);
  const paneId = optionalString(env.TMUX_PANE);
  const socketPath = optionalString(tmux?.split(",", 1)[0]);
  if (!socketPath || !paneId) return undefined;
  return { socket_path: socketPath, pane_id: paneId };
}

export async function bindManualSession(
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
      ...(options.startKind !== "fork" && optionalString(env.PONTIA_SESSION_ID) ? { session_id: optionalString(env.PONTIA_SESSION_ID) } : {}),
      ...(options.startKind !== "fork" && optionalString(env.PONTIA_SESSION_ID) && optionalString(env.PONTIA_RUNTIME_INSTANCE_ID)
        ? { runtime_instance_id: optionalString(env.PONTIA_RUNTIME_INSTANCE_ID) }
        : {}),
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

export async function hasExistingAgentBinding(
  env: EnvLike,
  fetchImpl: typeof fetch,
  sessionDetails: PiSessionDetails,
): Promise<boolean> {
  if (!sessionDetails.clientSessionKey) return false;
  const discovered = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const baseUrl = agentBindingLookupUrl(discovered?.bindingUpsertUrl);
  if (!baseUrl) return false;
  const url = new URL(baseUrl);
  url.searchParams.set("client_type", "pi");
  url.searchParams.set("client_session_key", sessionDetails.clientSessionKey);
  const response = await fetchImpl(url.toString());
  if (response.status === 404) return false;
  if (!response.ok) throw new Error(`agent binding lookup failed: ${response.status} ${response.statusText}`);
  return true;
}
