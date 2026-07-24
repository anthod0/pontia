import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { defaultHookLogFile, loadTurnContext, type EnvLike, type LoadTurnContextResult, type TurnContext } from "./context.js";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";
import { buildSessionContextUsageUpdatedEvent, buildSessionExitedEvent, buildSessionMessageUpdatedEvent, buildSessionReadyEvent, buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnInterruptedEvent, buildTurnOutputEvent, buildTurnStartedEvent, contextUsageFromPiHook, type InternalEvent, type PiTopologyContext, type PiTopologyEntryKind, type SessionMessageUpdatedReason } from "./events.js";
import { asRecord, optionalString, parseJsonResponse } from "./internal-api.js";
import { agentEndWasInterrupted, assistantDeltaFromEvent, assistantTextFromMessage, errorMessageFromAgentEnd, isTranscriptBoundaryMessageUpdate, lastAssistantTextFromMessages } from "./pi-message.js";
import { loadProfileSystemPrompt } from "./profile.js";
import { EventReporter, type EventReportResult } from "./reporter.js";
import { bindManualSession, hasExistingAgentBinding, piSessionDetailsFromHookContext, type PiSessionDetails } from "./runtime-binding.js";
import { loadSessionContext, type SessionContext } from "./session.js";
import { isActiveRegisteredWorkspace } from "./workspace.js";

interface ReporterLike {
  report(context: { internalEventUrl: string }, event: InternalEvent): Promise<EventReportResult | boolean>;
}

function reportAccepted(result: EventReportResult | boolean): boolean {
  return typeof result === "boolean" ? result : result.accepted;
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

function leafIdFromHookContext(ctx: unknown): string | null {
  if (!ctx || typeof ctx !== "object") return null;
  const sessionManager = (ctx as Record<string, unknown>).sessionManager;
  if (!sessionManager || typeof sessionManager !== "object") return null;
  const getLeafId = (sessionManager as Record<string, unknown>).getLeafId;
  if (typeof getLeafId !== "function") return null;
  const leafId = getLeafId.call(sessionManager);
  return typeof leafId === "string" && leafId.length > 0 ? leafId : null;
}

const NON_MESSAGE_ENTRY_KINDS = new Set([
  "thinking_level_change",
  "model_change",
  "compaction",
  "branch_summary",
  "custom",
  "custom_message",
  "label",
  "session_info",
]);

function topologyEntryKind(entry: Record<string, unknown>): PiTopologyEntryKind {
  if (entry.type !== "message") {
    return typeof entry.type === "string" && NON_MESSAGE_ENTRY_KINDS.has(entry.type)
      ? entry.type as PiTopologyEntryKind
      : "other";
  }
  const message = entry.message;
  const role = message && typeof message === "object"
    ? (message as Record<string, unknown>).role
    : undefined;
  if (role === "user") return "user_message";
  if (role === "assistant") return "assistant_message";
  if (role === "toolResult") return "tool_result_message";
  return "other_message";
}

function topologyContextFromHookContext(ctx: unknown): PiTopologyContext | undefined {
  if (!ctx || typeof ctx !== "object") return undefined;
  const sessionManager = (ctx as Record<string, unknown>).sessionManager;
  if (!sessionManager || typeof sessionManager !== "object") return undefined;
  const getBranch = (sessionManager as Record<string, unknown>).getBranch;
  if (typeof getBranch !== "function") return undefined;
  try {
    const branch = getBranch.call(sessionManager);
    if (!Array.isArray(branch)) return undefined;
    const entries = branch.map((value) => {
      if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
      const entry = value as Record<string, unknown>;
      if (typeof entry.id !== "string" || entry.id.length === 0) return undefined;
      return { id: entry.id, kind: topologyEntryKind(entry) };
    });
    if (entries.some((entry) => entry === undefined)) return undefined;
    return { entries: entries as PiTopologyContext["entries"] };
  } catch {
    return undefined;
  }
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

  pi.registerCommand("pontia-edit", {
    description: "Replay a Pontia Inbox message from a historical Pi entry",
    handler: async (args, ctx) => {
      const inboxMessageId = args.trim();
      const logFile = defaultHookLogFile(env);
      if (!/^msg_[^\s]+$/.test(inboxMessageId)) {
        await logDiagnostic(logFile, {
          level: "error",
          code: "branch_replay_invalid_command",
          message: "pontia-edit requires exactly one Inbox Message identifier",
        });
        return;
      }

      const loaded = boundSessionContext
        ? { ok: true as const, context: boundSessionContext, logFile }
        : await loadSessionContext(env);
      if (!loaded.ok) {
        await logDiagnostic(loaded.logFile, {
          level: "error",
          code: "branch_replay_stale_context",
          message: "pontia-edit requires a bound Pontia Session and Runtime",
          details: loaded.reason,
        });
        return;
      }
      const commandContext = loaded.context;
      const resolveUrl = commandContext.internalEventUrl.replace(
        /\/events\/?$/,
        "/inbox/branch-replay/resolve",
      );

      let replacementInput: string;
      let targetEntryId: string;
      try {
        const response = await fetchImpl(resolveUrl, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            inbox_message_id: inboxMessageId,
            session_id: commandContext.sessionId,
            runtime_instance_id: commandContext.runtimeInstanceId,
            client_type: "pi",
          }),
        });
        const body = await parseJsonResponse(response);
        if (!response.ok) {
          throw new Error(`${response.status} ${response.statusText}`);
        }
        const replay = asRecord(asRecord(asRecord(body)?.data)?.branch_replay);
        replacementInput = optionalString(replay?.replacement_input) ?? "";
        targetEntryId = optionalString(replay?.target_entry_id) ?? "";
        if (
          optionalString(replay?.inbox_message_id) !== inboxMessageId ||
          optionalString(replay?.session_id) !== commandContext.sessionId ||
          optionalString(replay?.runtime_instance_id) !== commandContext.runtimeInstanceId ||
          optionalString(replay?.client_type) !== "pi" ||
          !replacementInput ||
          !targetEntryId
        ) {
          throw new Error("branch replay response does not match the bound command context");
        }
      } catch (error) {
        await logDiagnostic(loaded.logFile, {
          level: "error",
          code: "branch_replay_resolve_failed",
          message: "failed to resolve pontia-edit command",
          details: error instanceof Error ? error.message : String(error),
        });
        return;
      }

      try {
        await ctx.waitForIdle();
        const navigation = await ctx.navigateTree(targetEntryId, { summarize: false });
        if (navigation.cancelled) {
          await logDiagnostic(loaded.logFile, {
            level: "warn",
            code: "branch_replay_navigation_cancelled",
            message: "pontia-edit tree navigation was cancelled",
            details: { inbox_message_id: inboxMessageId },
          });
          return;
        }
      } catch (error) {
        await logDiagnostic(loaded.logFile, {
          level: "error",
          code: "branch_replay_navigation_failed",
          message: "pontia-edit tree navigation failed",
          details: error instanceof Error ? error.message : String(error),
        });
        return;
      }

      try {
        ctx.ui.setEditorText("");
        await pi.sendUserMessage(replacementInput);
      } catch (error) {
        await logDiagnostic(loaded.logFile, {
          level: "error",
          code: "branch_replay_submission_failed",
          message: "pontia-edit replacement submission failed",
          details: error instanceof Error ? error.message : String(error),
        });
      }
    },
  });

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
      const logFile = defaultHookLogFile(env);
      let context: SessionContext | undefined;

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
        const parentSessionId = boundSessionContext?.sessionId ?? optionalString(env.PONTIA_SESSION_ID);
        if (!parentSessionId) {
          await logDiagnostic(logFile, {
            level: "error",
            code: "missing_fork_parent_session",
            message: "pi fork session_start requires an existing pontia parent session",
          });
          return;
        }
        context = await bindManualSession(env, fetchImpl, sessionDetails, { startKind: "fork", parentSessionId });
        readyReported = false;
      } else {
        const managedSession = optionalString(env.PONTIA_SESSION_ID) !== undefined;
        const existingManualSession = managedSession
          ? false
          : await hasExistingAgentBinding(env, fetchImpl, sessionDetails);
        if (!managedSession && !existingManualSession) {
          boundSessionContext = undefined;
          readyReported = false;
          return;
        }

        context = await bindManualSession(env, fetchImpl, sessionDetails);
        if (reason === "resume" || reason === "new") readyReported = false;
      }
      if (!context) return;

      boundSessionContext = context;
      readyReported = reportAccepted(await makeReporter(logFile).report(context, buildSessionReadyEvent(context)));
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
    const topologyContext = topologyContextFromHookContext(ctx);
    const previousLeafId = leafIdFromHookContext(ctx);
    if (pontiaDisabled) return;
    try {
      const loaded = await contextLoader(boundSessionContext ? {
        ...env,
        PONTIA_SESSION_ID: boundSessionContext.sessionId,
        PONTIA_RUNTIME_INSTANCE_ID: boundSessionContext.runtimeInstanceId,
      } : env);
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
            readyReported = reportAccepted(await makeReporter(logFile).report(boundSessionContext, buildSessionReadyEvent(boundSessionContext)));
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
      const started = await reporter.report(turnContext, buildTurnStartedEvent(turnContext, previousLeafId, topologyContext));
      const canonicalTurnId = typeof started === "boolean" ? turnContext.turnId : started.turnId;
      if (!reportAccepted(started) || !canonicalTurnId) {
        activeTurn = undefined;
        await logDiagnostic(logFile, {
          level: "error",
          code: "turn_start_not_normalized",
          message: "pontia did not return a canonical turn_id for turn.started",
        });
        return;
      }
      activeTurn = {
        context: { ...turnContext, turnId: canonicalTurnId },
        logFile,
        reporter,
        output: "",
        ended: false,
      };
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
    const terminalLeafId = leafIdFromHookContext(ctx);
    if (agentEndWasInterrupted(event)) {
      await state.reporter.report(state.context, buildTurnInterruptedEvent(state.context, terminalLeafId));
      await reportFinalMessageRefresh(state);
      return;
    }

    const failureMessage = errorMessageFromAgentEnd(event);
    if (failureMessage) {
      await state.reporter.report(state.context, buildTurnFailedEvent(state.context, failureMessage, terminalLeafId));
      await reportFinalMessageRefresh(state);
      return;
    }

    if (!state.output) {
      const finalText = lastAssistantTextFromMessages((event as unknown as Record<string, unknown> | undefined)?.messages);
      if (finalText) state.output = finalText;
    }

    const output = state.output.trim();
    if (output.length > 0) {
      const outputResult = await state.reporter.report(state.context, buildTurnOutputEvent(state.context, output));
      if (!reportAccepted(outputResult)) return;
    }

    await state.reporter.report(state.context, buildTurnCompletedEvent(state.context, terminalLeafId));
    await reportFinalMessageRefresh(state);
  });
}

export default function pontiaPiExtension(pi: ExtensionAPI): void {
  createPontiaPiExtension(pi);
}
