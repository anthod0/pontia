import { randomUUID } from "node:crypto";
import type { EnvLike, SessionContext } from "./context.js";
import { resolvePontiaConnection } from "./discovery.js";
import { asRecord, optionalString, parseJsonResponse } from "./internal-api.js";

export interface ClaudeSessionDetails {
  clientSessionKey?: string;
  transcriptPath?: string;
  clientCwd?: string;
}

function newRuntimeInstanceId(): string {
  return `rtinst_${randomUUID()}`;
}

function tmuxBindingFromEnv(env: EnvLike): { socket_path: string; pane_id: string } | undefined {
  const tmux = optionalString(env.TMUX);
  const paneId = optionalString(env.TMUX_PANE);
  const socketPath = optionalString(tmux?.split(",", 1)[0]);
  if (!socketPath || !paneId) return undefined;
  return { socket_path: socketPath, pane_id: paneId };
}

export async function bindManualSession(env: EnvLike, fetchImpl: typeof fetch, details: ClaudeSessionDetails): Promise<SessionContext | undefined> {
  if (optionalString(env.PONTIA_SESSION_ID)) return undefined;
  if (!details.clientSessionKey) return undefined;
  const discovered = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const url = discovered?.bindingUpsertUrl;
  if (!url) return undefined;
  const runtimeInstanceId = optionalString(env.PONTIA_RUNTIME_INSTANCE_ID) ?? newRuntimeInstanceId();
  const tmux = tmuxBindingFromEnv(env);
  const response = await fetchImpl(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      client_type: "claude",
      client_session_key: details.clientSessionKey,
      client_cwd: details.clientCwd,
      launch_cwd: details.clientCwd,
      runtime_instance_id: runtimeInstanceId,
      start_command: "claude",
      metadata: details.transcriptPath ? { transcript_path: details.transcriptPath } : undefined,
      ...(tmux ? { tmux } : {}),
    }),
  });
  const body = await parseJsonResponse(response);
  if (!response.ok) throw new Error(`runtime binding upsert failed: ${response.status} ${response.statusText}`);
  const record = asRecord(body);
  const session = asRecord(record?.session);
  const runtime = asRecord(record?.runtime);
  const sessionId = optionalString(session?.session_id);
  const resolvedRuntimeInstanceId = optionalString(runtime?.runtime_instance_id) ?? runtimeInstanceId;
  const internalEventUrl = optionalString(runtime?.internal_event_url) ?? discovered?.internalEventUrl;
  if (!sessionId || !internalEventUrl) return undefined;
  return { sessionId, clientType: "claude", internalEventUrl, runtimeInstanceId: resolvedRuntimeInstanceId, ...details };
}
