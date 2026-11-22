import { AsyncLocalStorage } from "node:async_hooks";
import { randomUUID } from "node:crypto";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import type { HistoryContext } from "./history.js";
import { PiCompaction } from "./compaction.js";
import { connectPi, type ControlInput, type PiControlBinding } from "./control-socket.js";
import { RpcError, type RuntimeConnection } from "../shared/control-socket.js";
import { defaultHookLogFile, type EnvLike, type TurnContext } from "./context.js";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";
import { pontiaHomeFromEnv } from "./discovery.js";
import {
  buildSessionContextUsageUpdatedEvent,
  buildSessionExitedEvent,
  buildSessionReadyEvent,
  buildTurnCompletedEvent,
  buildTurnFailedEvent,
  buildTurnInterruptedEvent,
  buildTurnOutputEvent,
  buildTurnStartedEvent,
  contextUsageFromPiHook,
  nextReportKey,
  type InternalEvent,
  type PiTopologyContext,
  type PiTopologyEntryKind,
} from "./events.js";
import { asRecord, optionalString } from "./values.js";
import {
  completeToolCallFromMessageUpdate,
  LiveOutputPublisher,
  type LiveOutputPublisherLike,
} from "./live-output.js";
import {
  agentEndWasInterrupted,
  assistantDeltaFromEvent,
  assistantTextFromMessage,
  errorMessageFromAgentEnd,
  lastAssistantTextFromMessages,
  thinkingDeltaFromEvent,
  userTextFromMessage,
} from "./pi-message.js";
import { loadSessionSystemPrompt } from "./instructions.js";
import { EventReporter } from "./reporter.js";
import {
  bindSession,
  loadExistingSessionContext,
  piSessionDetailsFromHookContext,
  type PiSessionDetails,
} from "./runtime-binding.js";
import type { SessionContext } from "./session.js";
import { isActiveRegisteredWorkspace } from "./workspace.js";

const extensionKey = Symbol.for("pontia.pi.extension");
const registry = globalThis as typeof globalThis & {
  [extensionKey]?: {
    owner?: symbol;
    forkParentSessionId?: string;
    reload?: {
      home: string;
      context: SessionContext;
      control: PiControlBinding;
      reporter?: EventReporter;
    };
  };
};
const extensionState = (registry[extensionKey] ??= {});

type ReporterLike = Pick<EventReporter, "report">;

function isPersistentSessionContext(ctx: unknown): boolean {
  if (!ctx || typeof ctx !== "object") return false;
  const mode = (ctx as Record<string, unknown>).mode;
  if (mode !== "tui" && mode !== "rpc") return false;
  return piSessionDetailsFromHookContext(ctx).clientSessionFile !== undefined;
}

function terminateRpcProcess(): void {
  process.kill(process.pid, "SIGTERM");
}

export interface PontiaPiExtensionDependencies {
  env?: EnvLike;
  makeReporter?: (logFile: string) => ReporterLike;
  logDiagnostic?: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
  makeLiveOutputPublisher?: (context: TurnContext & { turnId: string }) => LiveOutputPublisherLike;
  connectPi?: typeof connectPi;
}

interface ActiveTurnState {
  context: TurnContext;
  start: ReturnType<ReporterLike["report"]>;
  logFile: string;
  reporter: ReporterLike;
  liveOutput?: LiveOutputPublisherLike;
  output: string;
  ended: boolean;
  // A prompted run's first user message is the Turn input; later ones were consumed mid-run.
  awaitingPromptMessage: boolean;
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
      ? (entry.type as PiTopologyEntryKind)
      : "other";
  }
  const message = entry.message;
  const role =
    message && typeof message === "object" ? (message as Record<string, unknown>).role : undefined;
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

export function createPontiaPiExtension(
  pi: ExtensionAPI,
  dependencies: PontiaPiExtensionDependencies = {},
): void {
  const sourceEnv = dependencies.env ?? process.env;
  delete sourceEnv.PONTIA_SESSION_ID;
  delete sourceEnv.PONTIA_RUNTIME_INSTANCE_ID;
  if (!pontiaHomeFromEnv(sourceEnv)) return;
  // Environment extensions may load PONTIA_HOME after extension registration.
  const currentPontiaHome = () => pontiaHomeFromEnv(sourceEnv)!;
  const currentHookLogFile = () => defaultHookLogFile(currentPontiaHome());

  const rpcConnection: Pick<RuntimeConnection, "request"> = {
    request(method, params) {
      if (!controlSocket) return Promise.reject(new Error("Pi connection is unavailable"));
      return controlSocket.request(method, params);
    },
  };
  const reporters = new WeakMap<RuntimeConnection, EventReporter>();
  const makeReporter =
    dependencies.makeReporter ??
    ((logFile: string) => {
      const connection = controlSocket!;
      let reporter = reporters.get(connection);
      if (!reporter) {
        reporter = new EventReporter({ logFile, connection });
        reporters.set(connection, reporter);
      }
      return reporter;
    });
  const logDiagnostic = dependencies.logDiagnostic ?? appendDiagnostic;
  const makeLiveOutputPublisher =
    dependencies.makeLiveOutputPublisher ??
    ((context: TurnContext & { turnId: string }) =>
      new LiveOutputPublisher(context, { connection: rpcConnection }));

  let activeTurn: ActiveTurnState | undefined;
  let readyReported = false;
  let boundSessionContext: SessionContext | undefined;
  let deferredManualSessionDetails: PiSessionDetails | undefined;
  let reportingDisabled = false;
  let lastContextUsageJson: string | undefined;
  let pendingPrompt: string | undefined;
  let controlSocket: RuntimeConnection | undefined;
  let binding: PiControlBinding = {};
  let controlGeneration = 0;
  let piContext: ExtensionContext | undefined;
  const compaction = () => {
    const context = boundSessionContext;
    if (!context || reportingDisabled || !readyReported) return undefined;
    return new PiCompaction(currentPontiaHome(), context, (event) =>
      makeReporter(currentHookLogFile()).report(context, event),
    );
  };
  const directInput = new AsyncLocalStorage<{
    submission: ControlInput;
    sessionContext: SessionContext;
    consumed: boolean;
  }>();
  let queuedControlInput: { id: string; submission: ControlInput } | undefined;

  const modelId = (model: { provider: string; id: string }) => `${model.provider}/${model.id}`;

  let lastModelReport: { event: InternalEvent; conflict?: boolean } | undefined;

  async function reportModel(reconnecting = false): Promise<void> {
    const context = boundSessionContext;
    const model = piContext?.model;
    if (!context || !model || !readyReported || reportingDisabled) return;
    const name = modelId(model);
    if (
      !reconnecting ||
      lastModelReport?.event.session_id !== context.sessionId ||
      lastModelReport.event.data.runtime_id !== context.runtimeId ||
      lastModelReport.event.data.model !== name
    ) {
      lastModelReport = {
        event: {
          session_id: context.sessionId,
          report_key: nextReportKey(),
          type: "session.model_updated",
          data: { model: name, runtime_id: context.runtimeId },
        },
      };
    }
    const report = lastModelReport;
    if (report.conflict) return;
    const result = await makeReporter(currentHookLogFile()).report(context, report.event);
    if (result.conflict) report.conflict = true;
    if (!result.accepted) throw new Error("Pi model observation was not accepted");
  }

  function reportReady(context: SessionContext, logFile: string): void {
    readyReported = true;
    void makeReporter(logFile)
      .report(context, buildSessionReadyEvent(context))
      .then((result) => {
        if (!result.accepted && boundSessionContext === context) readyReported = false;
      })
      .catch(controlError);
    void reportModel().catch(controlError);
  }

  function sendControlInput(submission: ControlInput): void {
    if (!boundSessionContext) throw new Error("Pi session is not bound");
    try {
      directInput.run({ submission, sessionContext: boundSessionContext, consumed: false }, () =>
        pi.sendUserMessage(submission.input),
      );
    } catch (error) {
      throw new RpcError(
        -32007,
        `Pi input delivery uncertain: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  function dispatchQueuedInput(): void {
    if (!queuedControlInput) return;
    // Pi may defer commands sent during agent_settled. Bind the input identity
    // inside the command, after Pi has taken it out of that native queue.
    pi.sendUserMessage(`/pontia-input ${queuedControlInput.id}`, { expandPromptTemplates: true });
  }

  async function controlError(error: unknown): Promise<void> {
    await logDiagnostic(currentHookLogFile(), {
      level: "warn",
      code: "pi_control_unavailable",
      message: "Pi control channel failed",
      details: error instanceof Error ? error.message : String(error),
    });
  }

  async function closeControlSocket(): Promise<void> {
    controlGeneration += 1;
    queuedControlInput = undefined;
    const socket = controlSocket;
    binding = {};
    controlSocket = undefined;
    if (!socket) return;
    const reporter = reporters.get(socket);
    if (reporter)
      void reporter
        .drained()
        .then(() => socket.close())
        .catch(controlError);
    else await socket.close().catch(controlError);
  }

  async function registrationConnection(): Promise<RuntimeConnection> {
    if (controlSocket) return controlSocket;
    const generation = controlGeneration;
    const lifecycleContext = () => {
      if (
        generation !== controlGeneration ||
        reportingDisabled ||
        !boundSessionContext ||
        !piContext ||
        !readyReported
      ) {
        throw new Error("Pi lifecycle control session is no longer current");
      }
      return piContext;
    };
    const socket = await (dependencies.connectPi ?? connectPi)(
      currentPontiaHome(),
      (error) => {
        void controlError(error).catch(() => {});
      },
      (submission) => {
        if (generation !== controlGeneration || reportingDisabled || !boundSessionContext) {
          throw new Error("Pi control session is no longer current");
        }
        if (submission.steerTargetTurnId) {
          if (
            !activeTurn ||
            activeTurn.ended ||
            activeTurn.context.turnId !== submission.steerTargetTurnId ||
            piContext?.isIdle()
          ) {
            throw new RpcError(
              -32011,
              "Pi steer target is no longer active; input was not submitted",
            );
          }
          try {
            pi.sendUserMessage(submission.input, { deliverAs: "steer" });
          } catch (error) {
            throw new RpcError(
              -32007,
              `Pi steer delivery uncertain: ${error instanceof Error ? error.message : String(error)}`,
            );
          }
          return;
        }
        if (queuedControlInput)
          throw new RpcError(-32010, "Pi already has pending input; input was not submitted");
        const idle = piContext?.isIdle();
        if (!idle && !activeTurn?.ended)
          throw new RpcError(-32010, "Pi is busy; input was not submitted");
        const id = randomUUID();
        queuedControlInput = { id, submission };
        if (idle) dispatchQueuedInput();
      },
      async () => {
        if (generation !== controlGeneration) return;
        await reportModel(true);
      },
      (inboxMessageId) => {
        if (generation !== controlGeneration || reportingDisabled || !boundSessionContext) {
          throw new Error("Pi control session is no longer current");
        }
        // Command dispatch supplies the native context required by navigateTree.
        pi.sendUserMessage(`/pontia-edit ${inboxMessageId}`, { expandPromptTemplates: true });
      },
      {
        interrupt() {
          lifecycleContext().abort();
        },
        shutdownIfIdle() {
          const ctx = lifecycleContext();
          if (!ctx.isIdle() || ctx.hasPendingMessages()) return false;
          if (ctx.mode === "rpc") return terminateRpcProcess;
          ctx.shutdown();
          return true;
        },
        shutdown() {
          const ctx = lifecycleContext();
          if (ctx.mode === "rpc" && ctx.isIdle()) return terminateRpcProcess;
          // Ordinary extension context: abort first, then exit after Pi settles.
          // Unlike double Ctrl+C, this allows turn cleanup before session_shutdown.
          ctx.abort();
          ctx.shutdown();
        },
      },
      () => {
        lifecycleContext();
        const manager = piContext!.sessionManager;
        if (manager.getSessionId() !== boundSessionContext?.clientSessionKey)
          throw new Error("Pi history native Session identity changed");
        return { generation, sessionManager: manager } satisfies HistoryContext;
      },
      {
        compact(operationId) {
          const ctx = lifecycleContext();
          if (ctx.sessionManager.getSessionId() !== boundSessionContext?.clientSessionKey)
            throw new RpcError(-32006, "Pi compaction native Session identity changed");
          const observation = compaction()!;
          if (!ctx.isIdle() || ctx.hasPendingMessages() || queuedControlInput)
            throw new RpcError(-32006, "Pi is busy; compaction was not requested");
          observation.request(operationId, () =>
            ctx.compact({
              onComplete: (result) => {
                void observation.callbackComplete(operationId, result).catch(controlError);
              },
              onError: (error) => {
                void observation.callbackError(operationId, error).catch(controlError);
              },
            }),
          );
        },
      },
      binding,
    );
    if (generation !== controlGeneration) {
      await socket.close();
      throw new Error("Pi session changed during connection initialization");
    }
    controlSocket = socket;
    return socket;
  }

  pi.registerCommand("pontia-input", {
    description: "Deliver a pending Pontia input",
    handler: async (args) => {
      if (!queuedControlInput || args.trim() !== queuedControlInput.id) return;
      const { submission } = queuedControlInput;
      queuedControlInput = undefined;
      sendControlInput(submission);
    },
  });

  pi.registerCommand("pontia-edit", {
    description: "Replay a Pontia Inbox message from a historical Pi entry",
    handler: async (args, ctx) => {
      if (!isPersistentSessionContext(ctx)) return;
      const inboxMessageId = args.trim();
      const logFile = currentHookLogFile();
      if (!/^msg_[^\s]+$/.test(inboxMessageId)) {
        await logDiagnostic(logFile, {
          level: "error",
          code: "branch_replay_invalid_command",
          message: "pontia-edit requires exactly one Inbox Message identifier",
        });
        return;
      }

      const commandContext = boundSessionContext;
      if (!commandContext) {
        await logDiagnostic(logFile, {
          level: "error",
          code: "branch_replay_stale_context",
          message: "pontia-edit requires a bound Pontia Session and Runtime",
        });
        return;
      }
      const loaded = { logFile };

      let replacementInput: string;
      let targetEntryId: string;
      try {
        const connection = await registrationConnection();
        const body = await connection.request("branch.resolve", {
          inbox_message_id: inboxMessageId,
          session_id: commandContext.sessionId,
          runtime_id: commandContext.runtimeId,
          client_type: "pi",
        });
        const replay = asRecord(asRecord(body)?.branch_replay);
        replacementInput = optionalString(replay?.replacement_input) ?? "";
        targetEntryId = optionalString(replay?.target_entry_id) ?? "";
        if (
          optionalString(replay?.inbox_message_id) !== inboxMessageId ||
          optionalString(replay?.session_id) !== commandContext.sessionId ||
          optionalString(replay?.runtime_id) !== commandContext.runtimeId ||
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
        const current = boundSessionContext;
        if (
          current?.sessionId !== commandContext.sessionId ||
          current?.runtimeId !== commandContext.runtimeId
        ) {
          throw new Error("branch replay Runtime is no longer current");
        }
        await directInput.run(
          {
            submission: { input: replacementInput, inboxMessageId },
            sessionContext: current,
            consumed: false,
          },
          () => pi.sendUserMessage(replacementInput),
        );
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

  async function reportContextUsageFromHookEvent(event: unknown, ctx?: unknown): Promise<void> {
    if (!activeTurn || activeTurn.ended) return;
    const observation = contextUsageFromPiHook(event, ctx);
    if (!observation) return;
    const usageJson = JSON.stringify(observation);
    if (usageJson === lastContextUsageJson) return;
    lastContextUsageJson = usageJson;
    void activeTurn.reporter.report(
      activeTurn.context,
      buildSessionContextUsageUpdatedEvent(activeTurn.context, observation.context_usage),
      activeTurn.start,
    );
  }

  pi.on("before_agent_start", async (event, ctx) => {
    pendingPrompt = event.messageType === "user" ? event.prompt : undefined;
    if (reportingDisabled) return;
    const logFile = currentHookLogFile();
    try {
      if (!boundSessionContext) {
        if (ctx.mode === "rpc") return;
        const hookSessionDetails = piSessionDetailsFromHookContext(ctx);
        const sessionDetails = hookSessionDetails.clientSessionKey
          ? hookSessionDetails
          : deferredManualSessionDetails;
        if (!sessionDetails) return;
        const workspaceActive = await isActiveRegisteredWorkspace(
          await registrationConnection(),
          sessionDetails.clientCwd,
        );
        if (!workspaceActive) {
          await closeControlSocket();
          reportingDisabled = true;
          await logDiagnostic(logFile, {
            level: "info",
            code: "workspace_not_active",
            message:
              "current pi workspace is not an active registered pontia workspace; pontia reporting disabled",
            details: { client_cwd: sessionDetails.clientCwd },
          });
          return;
        }
        boundSessionContext = await bindSession(await registrationConnection(), sessionDetails);
        if (boundSessionContext) {
          sourceEnv.PONTIA_SESSION_ID = boundSessionContext.sessionId;
          sourceEnv.PONTIA_RUNTIME_INSTANCE_ID = boundSessionContext.runtimeId;
        }
        if (boundSessionContext && !readyReported) {
          piContext = ctx;
          controlSocket!.registered(boundSessionContext);
          reportReady(boundSessionContext, logFile);
        }
      }
      if (!boundSessionContext) return;
      const sessionPrompt = await loadSessionSystemPrompt(
        rpcConnection,
        boundSessionContext.sessionId,
      );
      if (sessionPrompt) {
        event.systemPromptOptions.sections.pontia_session_instructions = sessionPrompt;
      }
    } catch (error) {
      if (!boundSessionContext) await closeControlSocket();
      await logDiagnostic(currentHookLogFile(), {
        level: "warn",
        code: "system_prompt_append_failed",
        message: "failed to append pontia session system prompt",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("model_select", async (_event, ctx) => {
    piContext = ctx;
    void reportModel().catch(controlError);
  });

  pi.on("session_before_compact", async (event) => {
    void compaction()?.before(event).catch(controlError);
  });
  pi.on("session_compact", async (event) => {
    void compaction()?.completed(event).catch(controlError);
  });
  pi.on("session_compact_failed", async (event, ctx) => {
    void compaction()
      ?.failed(event, leafIdFromHookContext(ctx ?? piContext))
      .catch(controlError);
  });

  pi.on("session_start", async (event, ctx) => {
    if (reportingDisabled) return;
    const reason = (event as unknown as Record<string, unknown> | undefined)?.reason;
    if (readyReported && reason !== "fork" && reason !== "resume" && reason !== "new") return;
    if (
      reason !== "startup" &&
      reason !== "reload" &&
      reason !== "new" &&
      reason !== "resume" &&
      reason !== "fork"
    )
      return;
    delete sourceEnv.PONTIA_SESSION_ID;
    delete sourceEnv.PONTIA_RUNTIME_INSTANCE_ID;
    await closeControlSocket();
    const parentSessionId = boundSessionContext?.sessionId ?? extensionState.forkParentSessionId;
    delete extensionState.forkParentSessionId;
    boundSessionContext = undefined;
    readyReported = false;
    if (!isPersistentSessionContext(ctx)) {
      reportingDisabled = true;
      return;
    }

    try {
      const sessionDetails = piSessionDetailsFromHookContext(ctx);
      deferredManualSessionDetails = sessionDetails;
      const logFile = currentHookLogFile();
      let context: SessionContext | undefined;

      const reload = extensionState.reload;
      delete extensionState.reload;
      if (
        reload &&
        reason === "reload" &&
        reload.home === currentPontiaHome() &&
        reload.context.clientSessionKey === sessionDetails.clientSessionKey
      ) {
        binding = reload.control;
        boundSessionContext = reload.context;
        const socket = await registrationConnection();
        if (reload.reporter) reporters.set(socket, reload.reporter);
        piContext = ctx;
        sourceEnv.PONTIA_SESSION_ID = reload.context.sessionId;
        sourceEnv.PONTIA_RUNTIME_INSTANCE_ID = reload.context.runtimeId;
        readyReported = true;
        void reportModel().catch(controlError);
        return;
      }
      if (reload?.control.socket) {
        const previous = reload.control.socket;
        void Promise.resolve(reload.reporter?.drained())
          .then(() => previous.close())
          .catch(controlError);
      }
      const existingSession =
        reason === "fork"
          ? undefined
          : await loadExistingSessionContext(await registrationConnection(), sessionDetails);
      if (ctx.mode === "rpc" && !existingSession && !(reason === "fork" && parentSessionId)) {
        await closeControlSocket();
        reportingDisabled = true;
        return;
      }
      const workspaceActive =
        existingSession !== undefined ||
        (await isActiveRegisteredWorkspace(
          await registrationConnection(),
          sessionDetails.clientCwd,
        ));
      if (!workspaceActive) {
        await closeControlSocket();
        reportingDisabled = true;
        await logDiagnostic(logFile, {
          level: "info",
          code: "workspace_not_active",
          message:
            "current pi workspace is not an active registered pontia workspace; pontia reporting disabled",
          details: { client_cwd: sessionDetails.clientCwd },
        });
        return;
      }

      if (reason === "fork") {
        if (!parentSessionId) {
          await closeControlSocket();
          await logDiagnostic(logFile, {
            level: "error",
            code: "missing_fork_parent_session",
            message: "pi fork session_start requires an existing pontia parent session",
          });
          return;
        }
        context = await bindSession(await registrationConnection(), sessionDetails, {
          startKind: "fork",
          parentSessionId,
        });
        readyReported = false;
      } else {
        if (reason === "reload" && !existingSession) {
          reportingDisabled = true;
          deferredManualSessionDetails = undefined;
          await closeControlSocket();
          await logDiagnostic(logFile, {
            level: "error",
            code: "reload_session_context_missing",
            message: "pi reload could not recover the existing pontia runtime binding",
            details: { client_session_key: sessionDetails.clientSessionKey },
          });
          return;
        }
        if (
          reason !== "reload" &&
          existingSession &&
          ["idle", "busy", "interrupted"].includes(existingSession.sessionState)
        ) {
          await closeControlSocket();
          reportingDisabled = true;
          boundSessionContext = undefined;
          readyReported = false;
          await logDiagnostic(logFile, {
            level: "info",
            code: "duplicate_active_client_session",
            message:
              "native pi session is already bound to an active pontia session; duplicate TUI reporting disabled",
            details: {
              client_session_key: sessionDetails.clientSessionKey,
              session_id: existingSession.sessionId,
              session_state: existingSession.sessionState,
            },
          });
          return;
        }
        if (!existingSession) {
          await closeControlSocket();
          boundSessionContext = undefined;
          readyReported = false;
          return;
        }

        context = await bindSession(await registrationConnection(), sessionDetails, {
          runtimeId:
            reason === "reload" || existingSession?.sessionState === "starting"
              ? existingSession?.runtimeId
              : undefined,
        });
        if (
          reason === "reload" &&
          (context?.sessionId !== existingSession!.sessionId ||
            context.runtimeId !== existingSession!.runtimeId)
        ) {
          reportingDisabled = true;
          deferredManualSessionDetails = undefined;
          throw new Error("pi reload runtime binding changed identity");
        }
        if (reason === "reload" || reason === "resume" || reason === "new") readyReported = false;
      }
      if (!context) {
        if (reason === "reload") {
          reportingDisabled = true;
          deferredManualSessionDetails = undefined;
        }
        await closeControlSocket();
        return;
      }

      boundSessionContext = context;
      sourceEnv.PONTIA_SESSION_ID = context.sessionId;
      sourceEnv.PONTIA_RUNTIME_INSTANCE_ID = context.runtimeId;
      piContext = ctx;
      controlSocket!.registered(context);
      reportReady(context, logFile);
    } catch (error) {
      await closeControlSocket();
      const logFile = currentHookLogFile();
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to report pontia ready signal",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("session_shutdown", async (event) => {
    if (event.reason === "fork") {
      extensionState.forkParentSessionId = boundSessionContext?.sessionId;
    } else {
      delete extensionState.forkParentSessionId;
    }
    delete sourceEnv.PONTIA_SESSION_ID;
    delete sourceEnv.PONTIA_RUNTIME_INSTANCE_ID;
    piContext = undefined;
    readyReported = false;
    if (event.reason === "reload" && boundSessionContext && controlSocket) {
      extensionState.reload = {
        home: currentPontiaHome(),
        context: boundSessionContext,
        control: binding,
        reporter: reporters.get(controlSocket),
      };
      controlGeneration++;
      controlSocket = undefined;
      binding = {};
      return;
    }
    try {
      if (reportingDisabled || !boundSessionContext) return;
      const reason = (event as unknown as Record<string, unknown> | undefined)?.reason;
      if (reason !== "quit" && reason !== "new" && reason !== "resume" && reason !== "fork") return;
      const logFile = currentHookLogFile();
      void makeReporter(logFile)
        .report(boundSessionContext, buildSessionExitedEvent(boundSessionContext, reason))
        .catch(controlError);
    } catch (error) {
      const logFile = currentHookLogFile();
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to report pontia exited signal",
        details: error instanceof Error ? error.message : String(error),
      });
    } finally {
      await closeControlSocket();
    }
  });

  pi.on("agent_start", async (_event, ctx) => {
    const topologyContext = topologyContextFromHookContext(ctx);
    const previousLeafId = leafIdFromHookContext(ctx);
    if (reportingDisabled) return;
    try {
      const dispatch = directInput.getStore();
      const submission =
        dispatch && !dispatch.consumed && dispatch.sessionContext === boundSessionContext
          ? dispatch.submission
          : undefined;
      if (dispatch) dispatch.consumed = true;
      const logFile = currentHookLogFile();
      if (!previousLeafId) {
        activeTurn = undefined;
        pendingPrompt = undefined;
        await logDiagnostic(logFile, {
          level: "error",
          code: "turn_start_entry_id_missing",
          message: "pi agent_start has no native session entry id; turn.started was not reported",
        });
        return;
      }
      if (!boundSessionContext) {
        activeTurn = undefined;
        pendingPrompt = undefined;
        return;
      }
      const turnContext: TurnContext = {
        sessionId: boundSessionContext.sessionId,
        runtimeId: boundSessionContext.runtimeId,
        clientType: "pi",
        ...(submission ?? { input: pendingPrompt }),
      };
      const prompted = turnContext.input !== undefined;

      pendingPrompt = undefined;
      const reporter = makeReporter(logFile);
      lastContextUsageJson = undefined;
      // Editing and replaying input can start another run from the same Pi leaf.
      const start = reporter.report(
        turnContext,
        buildTurnStartedEvent(turnContext, nextReportKey(), previousLeafId, topologyContext),
      );
      const state: ActiveTurnState = {
        context: turnContext,
        start,
        logFile,
        reporter,
        output: "",
        ended: false,
        awaitingPromptMessage: prompted,
      };
      activeTurn = state;
      void start
        .then(async (receipt) => {
          if (!receipt.accepted || !receipt.turnId) {
            if (activeTurn === state) activeTurn = undefined;
            await logDiagnostic(logFile, {
              level: "error",
              code: "turn_start_not_normalized",
              message: "pontia did not return a canonical turn_id for turn.started",
            });
            return;
          }
          state.context.turnId = receipt.turnId;
          if (!state.ended && activeTurn === state) {
            state.liveOutput = makeLiveOutputPublisher({ ...turnContext, turnId: receipt.turnId });
            if (state.output) state.liveOutput.appendText(state.output);
          }
        })
        .catch(controlError);
    } catch (error) {
      if (!boundSessionContext) await closeControlSocket();
      pendingPrompt = undefined;
      activeTurn = undefined;
      const logFile = currentHookLogFile();
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to initialize pontia active turn",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("message_start", (event) => {
    if (!activeTurn || activeTurn.ended) return;
    const message = asRecord((event as unknown as Record<string, unknown>)?.message);
    if (message?.role !== "user") return;
    if (activeTurn.awaitingPromptMessage) {
      activeTurn.awaitingPromptMessage = false;
      return;
    }
    const text = userTextFromMessage(message);
    if (text) activeTurn.liveOutput?.appendUserInput(text);
  });

  pi.on("message_update", async (event, ctx) => {
    if (!activeTurn || activeTurn.ended) return;
    await reportContextUsageFromHookEvent(event, ctx);

    const thinking = thinkingDeltaFromEvent(event);
    if (thinking) activeTurn.liveOutput?.appendReasoning(thinking);
    const delta = assistantDeltaFromEvent(event);
    if (delta) activeTurn.liveOutput?.appendText(delta);
    const toolCall = completeToolCallFromMessageUpdate(event);
    if (toolCall) activeTurn.liveOutput?.appendToolCall(toolCall);

    const fullText = assistantTextFromMessage(
      (event as unknown as Record<string, unknown> | undefined)?.message,
    );
    if (fullText) {
      activeTurn.output = fullText;
    } else if (delta) {
      activeTurn.output += delta;
    }
  });

  const publishToolExecution = (event: unknown) => {
    if (activeTurn && !activeTurn.ended) activeTurn.liveOutput?.toolExecution(event);
  };
  pi.on("tool_execution_start", publishToolExecution);
  pi.on("tool_execution_update", publishToolExecution);
  pi.on("tool_execution_end", publishToolExecution);

  pi.on("message_end", async (event, ctx) => {
    if (!activeTurn || activeTurn.ended) return;
    await reportContextUsageFromHookEvent(event, ctx);
    const fullText = assistantTextFromMessage(
      (event as unknown as Record<string, unknown> | undefined)?.message,
    );
    if (fullText) activeTurn.output = fullText;
  });

  pi.on("agent_settled", (_event, ctx) => {
    if (!queuedControlInput || !ctx.isIdle()) return;
    dispatchQueuedInput();
  });

  pi.on("agent_end", async (event, ctx) => {
    if (!activeTurn || activeTurn.ended) return;
    await reportContextUsageFromHookEvent(event, ctx);
    activeTurn.ended = true;

    const state = activeTurn;
    const terminalLeafId = leafIdFromHookContext(ctx);
    const report = (event: InternalEvent) => {
      void state.reporter.report(state.context, event, state.start).catch(controlError);
    };
    if (agentEndWasInterrupted(event, ctx?.signal)) {
      report(buildTurnInterruptedEvent(state.context, terminalLeafId));
    } else {
      const failure = errorMessageFromAgentEnd(event);
      if (failure) {
        report(buildTurnFailedEvent(state.context, failure, terminalLeafId));
      } else {
        const output = (
          state.output ||
          lastAssistantTextFromMessages(
            (event as unknown as Record<string, unknown> | undefined)?.messages,
          ) ||
          ""
        ).trim();
        if (output) report(buildTurnOutputEvent(state.context, output));
        report(buildTurnCompletedEvent(state.context, terminalLeafId));
      }
    }
    void state.liveOutput?.close().catch(controlError);
  });
}

export default function pontiaPiExtension(pi: ExtensionAPI): void {
  if (extensionState.owner) {
    const home = pontiaHomeFromEnv();
    if (home) {
      void appendDiagnostic(defaultHookLogFile(home), {
        level: "warn",
        code: "duplicate_extension",
        message: "Pontia Pi extension is already loaded in this process; skipping duplicate",
      });
    }
    return;
  }

  const owner = Symbol();
  extensionState.owner = owner;
  const release = () => {
    if (extensionState.owner === owner) delete extensionState.owner;
  };
  try {
    createPontiaPiExtension(pi);
    pi.on("session_shutdown", release);
  } catch (error) {
    release();
    throw error;
  }
}
