import type { EnvLike } from "./context.js";
import { CONTROL_VERSION, type PiConnection } from "./control-socket.js";
import { asRecord, optionalString } from "./values.js";
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

function tmuxBindingFromEnv(env: EnvLike): { socket_path: string; pane_id: string } | undefined {
  const tmux = optionalString(env.TMUX);
  const paneId = optionalString(env.TMUX_PANE);
  const socketPath = optionalString(tmux?.split(",", 1)[0]);
  if (!socketPath || !paneId) return undefined;
  return { socket_path: socketPath, pane_id: paneId };
}

export async function bindSession(
  connection: PiConnection,
  env: EnvLike,
  sessionDetails: PiSessionDetails,
  options: { startKind?: "fork"; parentSessionId?: string; runtimeInstanceId?: string } = {},
): Promise<SessionContext | undefined> {
  if (!sessionDetails.clientSessionKey) return undefined;
  const tmux = tmuxBindingFromEnv(env);
  const body = await connection.request("runtime.register", {
    version: CONTROL_VERSION,
    binding: {
      client_type: "pi",
      client_session_key: sessionDetails.clientSessionKey,
      client_session_file: sessionDetails.clientSessionFile,
      client_session_dir: sessionDetails.clientSessionDir,
      client_cwd: sessionDetails.clientCwd,
      launch_cwd: sessionDetails.clientCwd,
      start_command: "pi",
      ...(options.startKind ? { start_kind: options.startKind } : {}),
      ...(options.parentSessionId ? { parent_session_id: options.parentSessionId } : {}),
      ...(options.runtimeInstanceId ? { runtime_instance_id: options.runtimeInstanceId } : {}),
      ...(tmux ? { tmux } : {}),
    },
  });

  const record = asRecord(body);
  const session = asRecord(record?.session);
  const runtime = asRecord(record?.runtime);
  const sessionId = optionalString(session?.session_id);
  const resolvedRuntimeInstanceId = optionalString(runtime?.runtime_instance_id);
  if (!sessionId) throw new Error("runtime binding upsert response missing session.session_id");
  if (!resolvedRuntimeInstanceId) throw new Error("runtime binding upsert response missing runtime.runtime_instance_id");
  return {
    sessionId,
    clientType: "pi",
    runtimeInstanceId: resolvedRuntimeInstanceId,
    ...sessionDetails,
  };
}

export interface ExistingPiSessionContext extends SessionContext {
  sessionState: string;
}

export async function loadExistingSessionContext(
  connection: PiConnection,
  sessionDetails: PiSessionDetails,
): Promise<ExistingPiSessionContext | undefined> {
  if (!sessionDetails.clientSessionKey) return undefined;
  const body = await connection.request("session.context", { client_session_key: sessionDetails.clientSessionKey });
  const record = asRecord(asRecord(body)?.session_context);
  if (!record) return undefined;
  const sessionId = optionalString(record?.session_id);
  const sessionState = optionalString(record?.session_state);
  const clientType = optionalString(record?.client_type);
  const runtimeInstanceId = optionalString(record?.runtime_instance_id);
  if (!sessionId || !sessionState || clientType !== "pi" || !runtimeInstanceId) {
    throw new Error("agent binding session context lookup returned an invalid context");
  }
  return {
    sessionId,
    sessionState,
    clientType: "pi",
    runtimeInstanceId,
    ...sessionDetails,
  };
}
