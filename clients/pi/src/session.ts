import { homedir } from "node:os";
import { join } from "node:path";
import { appendDiagnostic } from "./diagnostics.js";
import type { EnvLike } from "./context.js";

export interface SessionContext {
  sessionId: string;
  clientType: "pi";
  internalEventUrl: string;
  runtimeInstanceId: string;
  clientSessionKey?: string;
  clientSessionFile?: string;
  clientSessionDir?: string;
  clientCwd?: string;
}

export type LoadSessionContextResult =
  | { ok: true; context: SessionContext; logFile: string }
  | { ok: false; reason: string; logFile: string };

function fallbackLogDir(env: EnvLike = process.env): string {
  return join(env.PONTIA_HOME ?? join(env.HOME ?? homedir(), ".pontia"), "state");
}

function defaultHookLogFile(env: EnvLike = process.env): string {
  return join(fallbackLogDir(env), "pi-hook.log");
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function hasPontiaRuntimeIntent(env: EnvLike): boolean {
  return Boolean(
    optionalString(env.PONTIA_SESSION_ID) ||
      optionalString(env.PONTIA_RUNTIME_INSTANCE_ID) ||
      optionalString(env.PONTIA_INTERNAL_EVENT_URL),
  );
}

export async function loadSessionContext(env: EnvLike = process.env): Promise<LoadSessionContextResult> {
  const logFile = defaultHookLogFile(env);
  const sessionId = optionalString(env.PONTIA_SESSION_ID);
  const internalEventUrl = optionalString(env.PONTIA_INTERNAL_EVENT_URL);
  const runtimeInstanceId = optionalString(env.PONTIA_RUNTIME_INSTANCE_ID);
  const errors: string[] = [];

  if (!sessionId) errors.push("PONTIA_SESSION_ID is required");
  if (!internalEventUrl) errors.push("PONTIA_INTERNAL_EVENT_URL is required");
  if (!runtimeInstanceId) errors.push("PONTIA_RUNTIME_INSTANCE_ID is required");

  if (errors.length > 0) {
    const reason = errors.join("; ");
    if (!hasPontiaRuntimeIntent(env)) return { ok: false, reason, logFile };
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
