import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
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

export function defaultHookLogFile(env: EnvLike = process.env): string {
  return join(optionalString(env.PONTIA_HOME) ?? join(optionalString(env.HOME) ?? homedir(), ".pontia"), "state", "claude-hook.log");
}

function pontiaHome(env: EnvLike): string {
  return optionalString(env.PONTIA_HOME) ?? join(optionalString(env.HOME) ?? homedir(), ".pontia");
}

async function internalEventUrlFromPontiaHome(env: EnvLike): Promise<string | undefined> {
  let raw: string;
  try {
    raw = await readFile(join(pontiaHome(env), "config.toml"), "utf8");
  } catch {
    return undefined;
  }
  const match = raw.match(/^\s*bind_addr\s*=\s*"([^"]*)"/m);
  const bindAddr = optionalString(match?.[1]);
  if (!bindAddr) return undefined;
  const parts = bindAddr.match(/^([^:]+):(\d+)$/);
  if (!parts) return undefined;
  const host = parts[1] === "0.0.0.0" || parts[1] === "::" ? "127.0.0.1" : parts[1];
  const port = parts[2];
  const baseUrl = port === "80" ? `http://${host}` : `http://${host}:${port}`;
  return `${baseUrl}/internal/v1/events`;
}

function hasPontiaRuntimeIntent(env: EnvLike): boolean {
  return Boolean(
    optionalString(env.PONTIA_SESSION_ID) ||
      optionalString(env.PONTIA_RUNTIME_INSTANCE_ID),
  );
}

export async function loadSessionContext(env: EnvLike = process.env): Promise<LoadSessionContextResult> {
  const logFile = defaultHookLogFile(env);
  const sessionId = optionalString(env.PONTIA_SESSION_ID);
  const internalEventUrl = await internalEventUrlFromPontiaHome(env);
  const runtimeInstanceId = optionalString(env.PONTIA_RUNTIME_INSTANCE_ID);
  const errors: string[] = [];

  if (!sessionId) errors.push("PONTIA_SESSION_ID is required");
  if (!internalEventUrl) errors.push("pontia connection from PONTIA_HOME/config.toml is required");
  if (!runtimeInstanceId) errors.push("PONTIA_RUNTIME_INSTANCE_ID is required");

  if (errors.length > 0) {
    const reason = errors.join("; ");
    if (hasPontiaRuntimeIntent(env)) {
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
    logFile,
    context: {
      sessionId: sessionId!,
      clientType: "claude_code",
      internalEventUrl: internalEventUrl!,
      runtimeInstanceId: runtimeInstanceId!,
    },
  };
}
