import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { loadTurnContext, type EnvLike, type LoadTurnContextResult, type TurnContext } from "./context.js";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";
import { buildSessionReadyEvent, buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnOutputEvent, type InternalEvent } from "./events.js";
import { EventReporter } from "./reporter.js";
import { loadSessionContext } from "./session.js";
import { buildLlmpartyTools } from "./tools.js";

interface ReporterLike {
  report(context: { internalEventUrl: string }, event: InternalEvent): Promise<boolean>;
}

export interface LlmpartyPiExtensionDependencies {
  env?: EnvLike;
  loadContext?: (env: EnvLike) => Promise<LoadTurnContextResult>;
  makeReporter?: (logFile: string) => ReporterLike;
  logDiagnostic?: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
  fetch?: typeof fetch;
}

interface ActiveTurnState {
  context: TurnContext;
  logFile: string;
  reporter: ReporterLike;
  output: string;
  ended: boolean;
}

type LlmpartyAgentKind = "planner" | "executor";

const TOOL_NAMES_BY_AGENT_KIND: Record<LlmpartyAgentKind, Set<string>> = {
  planner: new Set(["getContext", "submitPlan", "applyPlan", "raiseSignal"]),
  executor: new Set(["getContext", "submitResult", "raiseSignal"]),
};

function allowedToolNamesForAgentKind(kind: string | undefined): Set<string> {
  if (kind === "planner" || kind === "executor") return TOOL_NAMES_BY_AGENT_KIND[kind];
  return new Set();
}

function textFromContent(content: unknown): string | undefined {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return undefined;

  const text = content
    .map((part) => {
      if (!part || typeof part !== "object") return "";
      const record = part as Record<string, unknown>;
      return record.type === "text" && typeof record.text === "string" ? record.text : "";
    })
    .join("");

  return text.length > 0 ? text : undefined;
}

function assistantTextFromMessage(message: unknown): string | undefined {
  if (!message || typeof message !== "object") return undefined;
  const record = message as Record<string, unknown>;
  if (record.role !== "assistant") return undefined;
  return textFromContent(record.content);
}

function assistantDeltaFromEvent(event: unknown): string | undefined {
  if (!event || typeof event !== "object") return undefined;
  const record = event as Record<string, unknown>;
  const streamEvent = record.assistantMessageEvent;
  if (streamEvent && typeof streamEvent === "object") {
    const streamRecord = streamEvent as Record<string, unknown>;
    for (const key of ["text_delta", "textDelta", "delta", "text"] as const) {
      const value = streamRecord[key];
      if (typeof value === "string" && value.length > 0) return value;
    }
  }
  return undefined;
}

function errorMessageFromAgentEnd(event: unknown): string | undefined {
  if (!event || typeof event !== "object") return undefined;
  const record = event as Record<string, unknown>;
  const error = record.error;
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error.length > 0) return error;
  if (record.isError === true) return "pi agent reported an error";
  return undefined;
}

function lastAssistantTextFromMessages(messages: unknown): string | undefined {
  if (!Array.isArray(messages)) return undefined;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const text = assistantTextFromMessage(messages[index]);
    if (text) return text;
  }
  return undefined;
}

export function createLlmpartyPiExtension(pi: ExtensionAPI, dependencies: LlmpartyPiExtensionDependencies = {}): void {
  const env = dependencies.env ?? process.env;
  const contextLoader = dependencies.loadContext ?? loadTurnContext;
  const makeReporter = dependencies.makeReporter ?? ((logFile: string) => new EventReporter({ logFile }));
  const logDiagnostic = dependencies.logDiagnostic ?? appendDiagnostic;
  const allowedToolNames = allowedToolNamesForAgentKind(env.LLMPARTY_AGENT_KIND);
  for (const tool of buildLlmpartyTools({
    env,
    loadContext: contextLoader,
    logDiagnostic,
    fetch: dependencies.fetch,
  })) {
    if (!allowedToolNames.has(tool.name)) continue;
    pi.registerTool(tool as any);
  }

  let activeTurn: ActiveTurnState | undefined;
  let readyReported = false;

  pi.on("session_start", async (event) => {
    if (readyReported) return;
    const reason = (event as unknown as Record<string, unknown> | undefined)?.reason;
    if (reason !== "startup") return;

    try {
      const loaded = await loadSessionContext(env);
      if (!loaded.ok) return;
      readyReported = await makeReporter(loaded.logFile).report(loaded.context, buildSessionReadyEvent(loaded.context));
    } catch (error) {
      const logFile = env.LLMPARTY_PI_HOOK_LOG ?? "pi-hook.log";
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to report llmparty ready signal",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("agent_start", async (_event, ctx) => {
    try {
      const loaded = await contextLoader(env);
      if (!loaded.ok) {
        activeTurn = undefined;
        if (!loaded.silent && ctx?.hasUI) ctx.ui.notify(`llmparty: ${loaded.reason}`, "warning");
        return;
      }

      activeTurn = {
        context: loaded.context,
        logFile: loaded.logFile,
        reporter: makeReporter(loaded.logFile),
        output: "",
        ended: false,
      };
    } catch (error) {
      activeTurn = undefined;
      const logFile = env.LLMPARTY_PI_HOOK_LOG ?? "pi-hook.log";
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to initialize llmparty active turn",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("message_update", async (event) => {
    if (!activeTurn || activeTurn.ended) return;

    const fullText = assistantTextFromMessage((event as unknown as Record<string, unknown> | undefined)?.message);
    if (fullText) {
      activeTurn.output = fullText;
      return;
    }

    const delta = assistantDeltaFromEvent(event);
    if (delta) activeTurn.output += delta;
  });

  pi.on("message_end", async (event) => {
    if (!activeTurn || activeTurn.ended) return;
    const fullText = assistantTextFromMessage((event as unknown as Record<string, unknown> | undefined)?.message);
    if (fullText) activeTurn.output = fullText;
  });

  pi.on("agent_end", async (event) => {
    if (!activeTurn || activeTurn.ended) return;
    activeTurn.ended = true;

    const state = activeTurn;
    const failureMessage = errorMessageFromAgentEnd(event);
    if (failureMessage) {
      await state.reporter.report(state.context, buildTurnFailedEvent(state.context, failureMessage));
      return;
    }

    if (!state.output) {
      const finalText = lastAssistantTextFromMessages((event as unknown as Record<string, unknown> | undefined)?.messages);
      if (finalText) state.output = finalText;
    }

    const output = state.output.trim();
    if (output.length > 0) {
      const outputOk = await state.reporter.report(state.context, buildTurnOutputEvent(state.context, output));
      if (!outputOk) return;
    }

    await state.reporter.report(state.context, buildTurnCompletedEvent(state.context));
  });
}

export default function llmpartyPiExtension(pi: ExtensionAPI): void {
  createLlmpartyPiExtension(pi);
}
