import { type EnvLike } from "./context.js";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";
import { buildSessionReadyEvent, type InternalEvent } from "./events.js";
import { defaultHookLogFile, loadSessionContext } from "./session.js";
import { EventReporter } from "./reporter.js";

export type ClaudeHookCommand = "prompt-submit" | "session-start" | "stop" | "stop-failure";

interface ReporterLike {
  report(context: { internalEventUrl: string }, event: InternalEvent): Promise<boolean>;
}

export interface ClaudeHookDependencies {
  env?: EnvLike;
  loadContext?: (env: EnvLike) => Promise<unknown>;
  makeReporter?: (logFile: string) => ReporterLike;
  logDiagnostic?: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
}

export async function handleClaudeHook(
  command: ClaudeHookCommand,
  payload: unknown,
  dependencies: ClaudeHookDependencies = {},
): Promise<number> {
  const env = dependencies.env ?? process.env;
  const makeReporter = dependencies.makeReporter ?? ((logFile: string) => new EventReporter({ logFile }));
  const logDiagnostic = dependencies.logDiagnostic ?? appendDiagnostic;

  try {
    if (command === "session-start") {
      const loaded = await loadSessionContext(env);
      if (!loaded.ok) return 0;
      await makeReporter(loaded.logFile).report(loaded.context, buildSessionReadyEvent(loaded.context));
      return 0;
    }

    return 0;
  } catch (error) {
    const logFile = defaultHookLogFile(env);
    await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_hook_exception",
        message: "Claude Code hook handling failed",
        details: error instanceof Error ? error.message : String(error),
    });
    return 0;
  }
}
