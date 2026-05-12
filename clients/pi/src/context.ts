import { readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { appendDiagnostic } from "./diagnostics.js";

export interface TurnContext {
  sessionId: string;
  turnId: string;
  runtimeInstanceId: string;
  input?: string;
  clientType: "pi";
  internalEventUrl: string;
}

export type LoadTurnContextResult =
  | { ok: true; context: TurnContext; contextFile: string; logFile: string }
  | { ok: false; reason: string; contextFile: string; logFile: string; silent?: boolean };

export type EnvLike = Record<string, string | undefined>;

function fallbackRuntimeDir(): string {
  return join(tmpdir(), "llmparty", "pi-runtime-fallback");
}

export function defaultCurrentTurnFile(env: EnvLike = process.env): string {
  const runtimeDir = env.LLMPARTY_RUNTIME_DIR ?? fallbackRuntimeDir();
  return join(runtimeDir, "current-turn.json");
}

export function defaultHookLogFile(env: EnvLike = process.env): string {
  const runtimeDir = env.LLMPARTY_RUNTIME_DIR ?? fallbackRuntimeDir();
  return join(runtimeDir, "pi-hook.log");
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function hasLlmpartyRuntimeIntent(env: EnvLike): boolean {
  return Boolean(
    optionalString(env.LLMPARTY_RUNTIME_DIR) ||
      optionalString(env.LLMPARTY_CURRENT_TURN_FILE) ||
      optionalString(env.LLMPARTY_SESSION_ID) ||
      optionalString(env.LLMPARTY_RUNTIME_INSTANCE_ID) ||
      optionalString(env.LLMPARTY_INTERNAL_EVENT_URL),
  );
}

export async function loadTurnContext(env: EnvLike = process.env): Promise<LoadTurnContextResult> {
  const contextFile = env.LLMPARTY_CURRENT_TURN_FILE ?? defaultCurrentTurnFile(env);
  const logFile = env.LLMPARTY_PI_HOOK_LOG ?? defaultHookLogFile(env);

  let raw: string;
  try {
    raw = await readFile(contextFile, "utf8");
  } catch (error) {
    const reason = `current-turn file is missing or unreadable: ${contextFile}`;
    if (!hasLlmpartyRuntimeIntent(env)) return { ok: false, reason, contextFile, logFile, silent: true };
    await appendDiagnostic(logFile, {
      level: "warn",
      code: "missing_current_turn_file",
      message: reason,
      details: error instanceof Error ? error.message : String(error),
    });
    return { ok: false, reason, contextFile, logFile };
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    const reason = "current-turn file is not valid JSON";
    await appendDiagnostic(logFile, {
      level: "error",
      code: "invalid_current_turn_context",
      message: reason,
      details: error instanceof Error ? error.message : String(error),
    });
    return { ok: false, reason, contextFile, logFile };
  }

  const record = asRecord(parsed);
  const errors: string[] = [];
  if (!record) errors.push("context must be a JSON object");

  const sessionId = optionalString(record?.session_id);
  const turnId = optionalString(record?.turn_id);
  const clientType = optionalString(record?.client_type);
  const internalEventUrl = optionalString(env.LLMPARTY_INTERNAL_EVENT_URL) ?? optionalString(record?.internal_event_url);
  const runtimeInstanceId = optionalString(env.LLMPARTY_RUNTIME_INSTANCE_ID) ?? optionalString(record?.runtime_instance_id);
  const input = optionalString(record?.input);

  if (!sessionId) errors.push("session_id is required");
  if (!turnId) errors.push("turn_id is required");
  if (clientType !== "pi") errors.push("client_type must be pi");
  if (!internalEventUrl) errors.push("internal_event_url or LLMPARTY_INTERNAL_EVENT_URL is required");
  if (!runtimeInstanceId) errors.push("runtime_instance_id or LLMPARTY_RUNTIME_INSTANCE_ID is required");

  if (errors.length > 0) {
    const reason = errors.join("; ");
    await appendDiagnostic(logFile, {
      level: "error",
      code: "invalid_current_turn_context",
      message: reason,
      details: { contextFile },
    });
    return { ok: false, reason, contextFile, logFile };
  }

  return {
    ok: true,
    contextFile,
    logFile,
    context: {
      sessionId: sessionId!,
      turnId: turnId!,
      runtimeInstanceId: runtimeInstanceId!,
      input,
      clientType: "pi",
      internalEventUrl: internalEventUrl!,
    },
  };
}
