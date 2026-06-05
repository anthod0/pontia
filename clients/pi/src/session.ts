import { tmpdir } from "node:os";
import { join } from "node:path";
import { appendDiagnostic } from "./diagnostics.js";
import type { EnvLike } from "./context.js";

export interface SessionContext {
  sessionId: string;
  clientType: "pi";
  internalEventUrl: string;
  runtimeInstanceId: string;
}

export type LoadSessionContextResult =
  | { ok: true; context: SessionContext; logFile: string }
  | { ok: false; reason: string; logFile: string };

function fallbackRuntimeDir(): string {
  return join(tmpdir(), "pilotfy", "pi-runtime-fallback");
}

function defaultHookLogFile(env: EnvLike = process.env): string {
  const runtimeDir = env.PILOTFY_RUNTIME_DIR ?? fallbackRuntimeDir();
  return join(runtimeDir, "pi-hook.log");
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function hasPilotfyRuntimeIntent(env: EnvLike): boolean {
  return Boolean(
    optionalString(env.PILOTFY_RUNTIME_DIR) ||
      optionalString(env.PILOTFY_CURRENT_TURN_FILE) ||
      optionalString(env.PILOTFY_SESSION_ID) ||
      optionalString(env.PILOTFY_RUNTIME_INSTANCE_ID) ||
      optionalString(env.PILOTFY_INTERNAL_EVENT_URL),
  );
}

export async function loadSessionContext(env: EnvLike = process.env): Promise<LoadSessionContextResult> {
  const logFile = env.PILOTFY_PI_HOOK_LOG ?? defaultHookLogFile(env);
  const sessionId = optionalString(env.PILOTFY_SESSION_ID);
  const internalEventUrl = optionalString(env.PILOTFY_INTERNAL_EVENT_URL);
  const runtimeInstanceId = optionalString(env.PILOTFY_RUNTIME_INSTANCE_ID);
  const errors: string[] = [];

  if (!sessionId) errors.push("PILOTFY_SESSION_ID is required");
  if (!internalEventUrl) errors.push("PILOTFY_INTERNAL_EVENT_URL is required");
  if (!runtimeInstanceId) errors.push("PILOTFY_RUNTIME_INSTANCE_ID is required");

  if (errors.length > 0) {
    const reason = errors.join("; ");
    if (!hasPilotfyRuntimeIntent(env)) return { ok: false, reason, logFile };
    await appendDiagnostic(logFile, {
      level: "error",
      code: "invalid_session_context",
      message: reason,
    });
    return { ok: false, reason, logFile };
  }

  return {
    ok: true,
    logFile,
    context: {
      sessionId: sessionId!,
      clientType: "pi",
      internalEventUrl: internalEventUrl!,
      runtimeInstanceId: runtimeInstanceId!,
    },
  };
}
