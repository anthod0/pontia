import { appendDiagnostic } from "./diagnostics.js";
import type { EnvLike } from "./context.js";

export interface SessionContext {
  sessionId: string;
  clientType: "claude_code";
  internalEventUrl: string;
  runtimeInstanceId: string;
}

export type LoadSessionContextResult =
  | { ok: true; context: SessionContext; logFile: string }
  | { ok: false; reason: string; logFile?: string };

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function hasPontiaRuntimeIntent(env: EnvLike): boolean {
  return Boolean(
    optionalString(env.PONTIA_SESSION_ID) ||
      optionalString(env.PONTIA_RUNTIME_INSTANCE_ID) ||
      optionalString(env.PONTIA_INTERNAL_EVENT_URL) ||
      optionalString(env.PONTIA_CLAUDE_HOOK_LOG),
  );
}

export async function loadSessionContext(env: EnvLike = process.env): Promise<LoadSessionContextResult> {
  const logFile = optionalString(env.PONTIA_CLAUDE_HOOK_LOG);
  const sessionId = optionalString(env.PONTIA_SESSION_ID);
  const internalEventUrl = optionalString(env.PONTIA_INTERNAL_EVENT_URL);
  const runtimeInstanceId = optionalString(env.PONTIA_RUNTIME_INSTANCE_ID);
  const errors: string[] = [];

  if (!sessionId) errors.push("PONTIA_SESSION_ID is required");
  if (!internalEventUrl) errors.push("PONTIA_INTERNAL_EVENT_URL is required");
  if (!runtimeInstanceId) errors.push("PONTIA_RUNTIME_INSTANCE_ID is required");
  if (!logFile) errors.push("PONTIA_CLAUDE_HOOK_LOG is required");

  if (errors.length > 0) {
    const reason = errors.join("; ");
    if (logFile && hasPontiaRuntimeIntent(env)) {
      await appendDiagnostic(logFile, {
        level: "error",
        code: "invalid_session_context",
        message: reason,
      });
    }
    return { ok: false, reason, logFile };
  }

  return {
    ok: true,
    logFile: logFile!,
    context: {
      sessionId: sessionId!,
      clientType: "claude_code",
      internalEventUrl: internalEventUrl!,
      runtimeInstanceId: runtimeInstanceId!,
    },
  };
}
