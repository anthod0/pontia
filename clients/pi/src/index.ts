import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { defaultHookLogFile, loadTurnContext, type EnvLike, type LoadTurnContextResult, type TurnContext } from "./context.js";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";
import { buildSessionContextUsageUpdatedEvent, buildSessionExitedEvent, buildSessionMessageUpdatedEvent, buildSessionReadyEvent, buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnOutputEvent, buildTurnStartedEvent, contextUsageFromPiHook, newPontiaTurnId, type InternalEvent, type SessionMessageUpdatedReason } from "./events.js";
import { optionalString } from "./internal-api.js";
import { assistantDeltaFromEvent, assistantTextFromMessage, errorMessageFromAgentEnd, isTranscriptBoundaryMessageUpdate, lastAssistantTextFromMessages } from "./pi-message.js";
import { loadProfileSystemPrompt } from "./profile.js";
import { EventReporter } from "./reporter.js";
import { bindManualSession, hasExistingAgentBinding, piSessionDetailsFromHookContext, type PiSessionDetails } from "./runtime-binding.js";
import { loadSessionContext, type SessionContext } from "./session.js";
import { isActiveRegisteredWorkspace } from "./workspace.js";

interface ReporterLike {
  report(context: { internalEventUrl: string }, event: InternalEvent): Promise<boolean>;
}

export interface PontiaPiExtensionDependencies {
  env?: EnvLike;
  loadContext?: (env: EnvLike) => Promise<LoadTurnContextResult>;
  makeReporter?: (logFile: string) => ReporterLike;
  logDiagnostic?: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
  fetch?: typeof fetch;
}

interface ActiveTurnState {
  context: TurnContext & { turnId: string };
  logFile: string;
  reporter: ReporterLike;
  output: string;
  ended: boolean;
}

export function createPontiaPiExtension(pi: ExtensionAPI, dependencies: PontiaPiExtensionDependencies = {}): void {
  const env = dependencies.env ?? process.env;
  const contextLoader = dependencies.loadContext ?? loadTurnContext;
  const makeReporter = dependencies.makeReporter ?? ((logFile: string) => new EventReporter({ logFile }));
  const logDiagnostic = dependencies.logDiagnostic ?? appendDiagnostic;
  const fetchImpl = dependencies.fetch ?? fetch;

  let activeTurn: ActiveTurnState | undefined;
  let readyReported = false;
  let boundSessionContext: SessionContext | undefined;
  let deferredManualSessionDetails: PiSessionDetails | undefined;
  let pontiaDisabled = false;
  let lastContextUsageJson: string | undefined;
  let pendingPrompt: string | undefined;

  async function scheduleMessageRefresh(reason: SessionMessageUpdatedReason): Promise<void> {
    if (!activeTurn || activeTurn.ended) return;
    await activeTurn.reporter.report(activeTurn.context, buildSessionMessageUpdatedEvent(activeTurn.context, reason));
  }

  async function reportFinalMessageRefresh(state: ActiveTurnState): Promise<void> {
    await state.reporter.report(state.context, buildSessionMessageUpdatedEvent(state.context, "final"));
  }

  async function reportContextUsageFromHookEvent(event: unknown, ctx?: unknown): Promise<void> {
    if (!activeTurn || activeTurn.ended) return;
    const observation = contextUsageFromPiHook(event, ctx);
    if (!observation) return;
    const usageJson = JSON.stringify(observation);
    if (usageJson === lastContextUsageJson) return;
    lastContextUsageJson = usageJson;
    await activeTurn.reporter.report(activeTurn.context, buildSessionContextUsageUpdatedEvent(activeTurn.context, observation.context_usage, observation.model));
  }

  pi.on("before_agent_start", async (event) => {
    const eventRecord = event as unknown as Record<string, unknown>;
    pendingPrompt = optionalString(eventRecord.prompt);
    if (pontiaDisabled) return { systemPrompt: typeof eventRecord.systemPrompt === "string" ? eventRecord.systemPrompt : "" };
    const currentSystemPrompt = typeof eventRecord.systemPrompt === "string" ? eventRecord.systemPrompt : "";
    try {
      const profilePrompt = await loadProfileSystemPrompt(env, fetchImpl);
      if (!profilePrompt) return { systemPrompt: currentSystemPrompt };
      return { systemPrompt: `${currentSystemPrompt}\n\n${profilePrompt}` };
    } catch (error) {
      await logDiagnostic(defaultHookLogFile(env), {
        level: "warn",
        code: "system_prompt_append_failed",
        message: "failed to append pontia execution profile system prompt",
        details: error instanceof Error ? error.message : String(error),
      });
      return { systemPrompt: currentSystemPrompt };
    }
  });

  pi.on("session_start", async (event, ctx) => {
    const reason = (event as unknown as Record<string, unknown> | undefined)?.reason;
    if (readyReported && reason !== "fork" && reason !== "resume") return;
    if (reason !== "startup" && reason !== "new" && reason !== "resume" && reason !== "fork") return;

    try {
      const sessionDetails = piSessionDetailsFromHookContext(ctx);
      deferredManualSessionDetails = sessionDetails;
      const loaded = await loadSessionContext(env);
      const logFile = loaded.logFile;
      let context: SessionContext | undefined;

      if (env.PONTIA_SESSION_ID && !loaded.ok) return;

      const workspaceActive = await isActiveRegisteredWorkspace(env, fetchImpl, sessionDetails.clientCwd);
      if (workspaceActive !== true) {
        pontiaDisabled = true;
        await logDiagnostic(logFile, {
          level: "info",
          code: workspaceActive === false ? "workspace_not_active" : "workspace_check_unavailable",
          message: workspaceActive === false
            ? "current pi workspace is not an active registered pontia workspace; pontia reporting disabled"
            : "could not verify active registered pontia workspace; pontia reporting disabled",
          details: { client_cwd: sessionDetails.clientCwd },
        });
        return;
      }

      if (reason === "fork") {
        const parentSessionId = boundSessionContext?.sessionId ?? (loaded.ok ? loaded.context.sessionId : undefined);
        if (!parentSessionId) {
          await logDiagnostic(logFile, {
            level: "error",
            code: "missing_fork_parent_session",
            message: "pi fork session_start requires an existing pontia parent session",
          });
          return;
        }
        context = await bindManualSession(env, fetchImpl, sessionDetails, { startKind: "fork", parentSessionId });
        if (!context) return;
        readyReported = false;
      } else if (loaded.ok) {
        if (!sessionDetails.clientSessionKey) {
          await logDiagnostic(logFile, {
            level: "error",
            code: "missing_pi_client_session_key",
            message: "ctx.sessionManager.getSessionId() is required to report pontia ready signal",
          });
          return;
        }
        context = { ...loaded.context, ...sessionDetails };
      } else {
        if (await hasExistingAgentBinding(env, fetchImpl, sessionDetails)) {
          context = await bindManualSession(env, fetchImpl, sessionDetails);
          if (!context) return;
          if (reason === "resume") readyReported = false;
        } else {
          await logDiagnostic(logFile, {
            level: "info",
            code: "manual_pi_binding_deferred",
            message: "manual pi session binding deferred until first agent turn",
          });
          return;
        }
      }

      boundSessionContext = context;
      readyReported = await makeReporter(logFile).report(context, buildSessionReadyEvent(context));
    } catch (error) {
      const logFile = defaultHookLogFile(env);
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to report pontia ready signal",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("session_shutdown", async (event) => {
    if (pontiaDisabled) return;
    const reason = (event as unknown as Record<string, unknown> | undefined)?.reason;
    if (reason !== "quit" && reason !== "new" && reason !== "resume" && reason !== "fork") return;

    try {
      const loaded = await loadSessionContext(env);
      const logFile = loaded.logFile;
      const context = boundSessionContext ?? (loaded.ok ? loaded.context : undefined);
      if (!context) return;
      await makeReporter(logFile).report(context, buildSessionExitedEvent(context, reason));
    } catch (error) {
      const logFile = defaultHookLogFile(env);
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to report pontia exited signal",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("agent_start", async (_event, ctx) => {
    if (pontiaDisabled) return;
    try {
      const loaded = await contextLoader(env);
      let turnContext: TurnContext | undefined;
      let logFile: string;
      if (loaded.ok) {
        turnContext = loaded.context;
        logFile = loaded.logFile;
      } else if (loaded.silent) {
        logFile = loaded.logFile;
        if (!boundSessionContext) {
          const hookSessionDetails = piSessionDetailsFromHookContext(ctx);
          const sessionDetails = hookSessionDetails.clientSessionKey ? hookSessionDetails : deferredManualSessionDetails;
          if (!sessionDetails) {
            activeTurn = undefined;
            pendingPrompt = undefined;
            return;
          }
          const workspaceActive = await isActiveRegisteredWorkspace(env, fetchImpl, sessionDetails.clientCwd);
          if (workspaceActive !== true) {
            pontiaDisabled = true;
            await logDiagnostic(logFile, {
              level: "info",
              code: workspaceActive === false ? "workspace_not_active" : "workspace_check_unavailable",
              message: workspaceActive === false
                ? "current pi workspace is not an active registered pontia workspace; pontia reporting disabled"
                : "could not verify active registered pontia workspace; pontia reporting disabled",
              details: { client_cwd: sessionDetails.clientCwd },
            });
            activeTurn = undefined;
            pendingPrompt = undefined;
            return;
          }
          boundSessionContext = await bindManualSession(env, fetchImpl, sessionDetails);
          if (boundSessionContext && !readyReported) {
            readyReported = await makeReporter(logFile).report(boundSessionContext, buildSessionReadyEvent(boundSessionContext));
          }
        }
        if (boundSessionContext) {
          turnContext = {
            sessionId: boundSessionContext.sessionId,
            runtimeInstanceId: boundSessionContext.runtimeInstanceId,
            clientType: "pi",
            internalEventUrl: boundSessionContext.internalEventUrl,
            input: pendingPrompt,
          };
        } else {
          activeTurn = undefined;
          pendingPrompt = undefined;
          return;
        }
      } else {
        activeTurn = undefined;
        if (!loaded.silent && ctx?.hasUI) ctx.ui.notify(`pontia: ${loaded.reason}`, "warning");
        return;
      }

      pendingPrompt = undefined;
      const reporter = makeReporter(logFile);
      lastContextUsageJson = undefined;
      activeTurn = {
        context: { ...turnContext, turnId: turnContext.turnId ?? newPontiaTurnId() },
        logFile,
        reporter,
        output: "",
        ended: false,
      };
      await reporter.report(activeTurn.context, buildTurnStartedEvent(activeTurn.context));
    } catch (error) {
      pendingPrompt = undefined;
      activeTurn = undefined;
      const logFile = defaultHookLogFile(env);
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to initialize pontia active turn",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("message_update", async (event, ctx) => {
    if (!activeTurn || activeTurn.ended) return;
    await reportContextUsageFromHookEvent(event, ctx);

    const fullText = assistantTextFromMessage((event as unknown as Record<string, unknown> | undefined)?.message);
    if (fullText) {
      activeTurn.output = fullText;
    } else {
      const delta = assistantDeltaFromEvent(event);
      if (delta) activeTurn.output += delta;
    }

    if (isTranscriptBoundaryMessageUpdate(event)) await scheduleMessageRefresh("update");
  });

  pi.on("tool_execution_start", async () => {
    await scheduleMessageRefresh("update");
  });

  pi.on("tool_execution_end", async () => {
    await scheduleMessageRefresh("update");
  });

  pi.on("message_end", async (event, ctx) => {
    if (!activeTurn || activeTurn.ended) return;
    await reportContextUsageFromHookEvent(event, ctx);
    const fullText = assistantTextFromMessage((event as unknown as Record<string, unknown> | undefined)?.message);
    if (fullText) activeTurn.output = fullText;
    await scheduleMessageRefresh("append");
  });

  pi.on("agent_end", async (event, ctx) => {
    if (!activeTurn || activeTurn.ended) return;
    await reportContextUsageFromHookEvent(event, ctx);
    activeTurn.ended = true;

    const state = activeTurn;
    const failureMessage = errorMessageFromAgentEnd(event);
    if (failureMessage) {
      await state.reporter.report(state.context, buildTurnFailedEvent(state.context, failureMessage));
      await reportFinalMessageRefresh(state);
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
    await reportFinalMessageRefresh(state);
  });
}

export default function pontiaPiExtension(pi: ExtensionAPI): void {
  createPontiaPiExtension(pi);
}
