import { randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { EnvLike } from "./context.js";

const execFileAsync = promisify(execFile);

export interface PiSessionDetails {
  clientSessionKey?: string;
  clientSessionFile?: string;
  clientSessionDir?: string;
  clientCwd?: string;
}

export interface BoundPontiaContext {
  sessionId: string;
  clientType: "pi";
  internalEventUrl: string;
  runtimeInstanceId: string;
  currentTurnFile: string;
}

interface TmuxMetadata {
  socket_path?: string;
  session_id?: string;
  session_name?: string;
  window_id?: string;
  window_index?: number;
  pane_id?: string;
  pane_index?: number;
  pane_current_path?: string;
}

export type AttachRuntimeBindingResult =
  | { ok: true; context: BoundPontiaContext; logFile: string }
  | { ok: false; code: string; reason: string; logFile: string };

export interface AttachRuntimeBindingOptions {
  env?: EnvLike;
  session: PiSessionDetails;
  fetch?: typeof fetch;
  runTmuxDisplayMessage?: (socketPath: string, paneId: string) => Promise<string>;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function defaultHookLogFile(env: EnvLike): string {
  return optionalString(env.PONTIA_PI_HOOK_LOG) ?? "pi-hook.log";
}

function upsertUrl(env: EnvLike): string {
  const explicitInternalApi = optionalString(env.PONTIA_INTERNAL_API_URL)?.replace(/\/+$/, "");
  if (explicitInternalApi) return `${explicitInternalApi}/runtime-bindings/upsert`;

  const eventUrl = optionalString(env.PONTIA_INTERNAL_EVENT_URL);
  if (eventUrl) return eventUrl.replace(/\/events\/?$/, "/runtime-bindings/upsert");

  const baseUrl = optionalString(env.PONTIA_BASE_URL)?.replace(/\/+$/, "") ?? "http://127.0.0.1:8080";
  return `${baseUrl}/internal/v1/runtime-bindings/upsert`;
}

function tmuxSocketPath(env: EnvLike): string | undefined {
  const tmux = optionalString(env.TMUX);
  if (!tmux) return undefined;
  return optionalString(tmux.split(",", 1)[0]);
}

async function defaultRunTmuxDisplayMessage(socketPath: string, paneId: string): Promise<string> {
  const { stdout } = await execFileAsync("tmux", [
    "-S",
    socketPath,
    "display-message",
    "-p",
    "-t",
    paneId,
    "#{session_id} #{session_name} #{window_id} #{window_index} #{pane_id} #{pane_index} #{pane_current_path}",
  ]);
  return stdout;
}

function parseInteger(value: string | undefined): number | undefined {
  if (value == null || value.trim() === "") return undefined;
  const parsed = Number(value);
  return Number.isInteger(parsed) ? parsed : undefined;
}

function parseTmuxDisplayMessage(socketPath: string, fallbackPaneId: string, raw: string): TmuxMetadata {
  const [session_id, session_name, window_id, window_index, pane_id, pane_index, ...pathParts] = raw.trim().split(/\s+/);
  const metadata: TmuxMetadata = {
    socket_path: socketPath,
    session_id: optionalString(session_id),
    session_name: optionalString(session_name),
    window_id: optionalString(window_id),
    window_index: parseInteger(window_index),
    pane_id: optionalString(pane_id) ?? fallbackPaneId,
    pane_index: parseInteger(pane_index),
  };
  const paneCurrentPath = optionalString(pathParts.join(" "));
  if (paneCurrentPath) metadata.pane_current_path = paneCurrentPath;
  return metadata;
}

async function collectTmuxMetadata(env: EnvLike, runTmuxDisplayMessage: (socketPath: string, paneId: string) => Promise<string>): Promise<TmuxMetadata | undefined> {
  const socketPath = tmuxSocketPath(env);
  const paneId = optionalString(env.TMUX_PANE);
  if (!socketPath || !paneId) return undefined;
  try {
    return parseTmuxDisplayMessage(socketPath, paneId, await runTmuxDisplayMessage(socketPath, paneId));
  } catch {
    return { socket_path: socketPath, pane_id: paneId };
  }
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;
}

function contextFromUpsertResponse(body: unknown, fallbackRuntimeInstanceId: string): BoundPontiaContext | undefined {
  const record = asRecord(body);
  const session = asRecord(record?.session);
  const runtime = asRecord(record?.runtime);
  const sessionId = optionalString(session?.session_id);
  const runtimeInstanceId = optionalString(runtime?.runtime_instance_id) ?? fallbackRuntimeInstanceId;
  const internalEventUrl = optionalString(runtime?.internal_event_url);
  const currentTurnFile = optionalString(runtime?.current_turn_file);
  if (!sessionId || !runtimeInstanceId || !internalEventUrl || !currentTurnFile) return undefined;
  return {
    sessionId,
    clientType: "pi",
    runtimeInstanceId,
    internalEventUrl,
    currentTurnFile,
  };
}

export async function attachRuntimeBinding(options: AttachRuntimeBindingOptions): Promise<AttachRuntimeBindingResult> {
  const env = options.env ?? process.env;
  const logFile = defaultHookLogFile(env);
  const fetchImpl = options.fetch ?? fetch;
  const runTmuxDisplayMessage = options.runTmuxDisplayMessage ?? defaultRunTmuxDisplayMessage;
  const clientSessionKey = optionalString(options.session.clientSessionKey);
  const clientCwd = optionalString(options.session.clientCwd) ?? optionalString(env.PONTIA_WORKSPACE) ?? process.cwd();

  if (!clientSessionKey) {
    return { ok: false, code: "missing_client_session_key", reason: "ctx.sessionManager.getSessionId() is required", logFile };
  }

  const runtimeInstanceId = optionalString(env.PONTIA_RUNTIME_INSTANCE_ID) ?? `rtinst_${randomUUID()}`;
  const tmux = await collectTmuxMetadata(env, runTmuxDisplayMessage);
  const body = {
    client_type: "pi",
    client_session_key: clientSessionKey,
    client_session_file: optionalString(options.session.clientSessionFile),
    client_session_dir: optionalString(options.session.clientSessionDir),
    client_cwd: clientCwd,
    launch_cwd: clientCwd,
    runtime_instance_id: runtimeInstanceId,
    start_command: optionalString(env.PONTIA_START_COMMAND),
    tmux,
  };

  try {
    const response = await fetchImpl(upsertUrl(env), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    const text = await response.text().catch(() => "");
    const parsed = text ? JSON.parse(text) : null;
    if (!response.ok) {
      return { ok: false, code: "upsert_failed", reason: `${response.status} ${response.statusText}`, logFile };
    }
    const context = contextFromUpsertResponse(parsed, runtimeInstanceId);
    if (!context) {
      return { ok: false, code: "invalid_upsert_response", reason: "runtime binding upsert response missing session/runtime context", logFile };
    }
    return { ok: true, context, logFile };
  } catch (error) {
    return { ok: false, code: "upsert_failed", reason: error instanceof Error ? error.message : String(error), logFile };
  }
}
