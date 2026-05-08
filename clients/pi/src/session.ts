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
  return join(tmpdir(), "llmparty", "pi-runtime-fallback");
}

function defaultHookLogFile(env: EnvLike = process.env): string {
  const runtimeDir = env.LLMPARTY_RUNTIME_DIR ?? fallbackRuntimeDir();
  return join(runtimeDir, "pi-hook.log");
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

export async function loadSessionContext(env: EnvLike = process.env): Promise<LoadSessionContextResult> {
  const logFile = env.LLMPARTY_PI_HOOK_LOG ?? defaultHookLogFile(env);
  const sessionId = optionalString(env.LLMPARTY_SESSION_ID);
  const internalEventUrl = optionalString(env.LLMPARTY_INTERNAL_EVENT_URL);
  const runtimeInstanceId = optionalString(env.LLMPARTY_RUNTIME_INSTANCE_ID);
  const errors: string[] = [];

  if (!sessionId) errors.push("LLMPARTY_SESSION_ID is required");
  if (!internalEventUrl) errors.push("LLMPARTY_INTERNAL_EVENT_URL is required");
  if (!runtimeInstanceId) errors.push("LLMPARTY_RUNTIME_INSTANCE_ID is required");

  if (errors.length > 0) {
    const reason = errors.join("; ");
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
