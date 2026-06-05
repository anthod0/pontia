import { loadTurnContext, type EnvLike, type LoadTurnContextResult, type TurnContext } from "./context.js";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";
import { buildSessionReadyEvent, buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnOutputEvent, type InternalEvent } from "./events.js";
import { loadSessionContext } from "./session.js";
import { EventReporter } from "./reporter.js";

export type ClaudeHookCommand = "prompt-submit" | "session-start" | "stop" | "stop-failure";

interface ReporterLike {
  report(context: { internalEventUrl: string }, event: InternalEvent): Promise<boolean>;
}

export interface ClaudeHookDependencies {
  env?: EnvLike;
  loadContext?: (env: EnvLike) => Promise<LoadTurnContextResult>;
  makeReporter?: (logFile: string) => ReporterLike;
  logDiagnostic?: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function failureMessage(payload: unknown): string {
  const record = payload && typeof payload === "object" ? (payload as Record<string, unknown>) : {};
  const error = optionalString(record.error) ?? "unknown_error";
  const details = optionalString(record.error_details) ?? optionalString(record.last_assistant_message);
  return details ? `Claude Code failed: ${error} - ${details}` : `Claude Code failed: ${error}`;
}

export async function handleClaudeHook(
  command: ClaudeHookCommand,
  payload: unknown,
  dependencies: ClaudeHookDependencies = {},
): Promise<number> {
  const env = dependencies.env ?? process.env;
  const contextLoader = dependencies.loadContext ?? loadTurnContext;
  const makeReporter = dependencies.makeReporter ?? ((logFile: string) => new EventReporter({ logFile }));
  const logDiagnostic = dependencies.logDiagnostic ?? appendDiagnostic;

  try {
    if (command === "session-start") {
      const loaded = await loadSessionContext(env);
      if (!loaded.ok) return 0;
      await makeReporter(loaded.logFile).report(loaded.context, buildSessionReadyEvent(loaded.context));
      return 0;
    }

    const loaded = await contextLoader(env);
    if (!loaded.ok) return 0;

    if (command === "prompt-submit") return 0;

    const reporter = makeReporter(loaded.logFile);
    if (command === "stop") {
      const record = payload && typeof payload === "object" ? (payload as Record<string, unknown>) : {};
      const output = optionalString(record.last_assistant_message);
      if (output) {
        const outputOk = await reporter.report(loaded.context, buildTurnOutputEvent(loaded.context, output));
        if (!outputOk) return 0;
      }
      await reporter.report(loaded.context, buildTurnCompletedEvent(loaded.context));
      return 0;
    }

    await reporter.report(loaded.context, buildTurnFailedEvent(loaded.context, failureMessage(payload)));
    return 0;
  } catch (error) {
    const logFile = env.PILOTFY_CLAUDE_HOOK_LOG ?? "claude-hook.log";
    await logDiagnostic(logFile, {
      level: "error",
      code: "unexpected_hook_exception",
      message: "Claude Code hook handling failed",
      details: error instanceof Error ? error.message : String(error),
    });
    return 0;
  }
}
