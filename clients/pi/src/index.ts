import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { defaultHookLogFile, loadTurnContext, type EnvLike, type LoadTurnContextResult, type TurnContext } from "./context.js";
import { attachRuntimeBinding, type AttachRuntimeBindingResult, type BoundPontiaContext } from "./attach.js";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";
import { buildSessionContextUsageUpdatedEvent, buildSessionMessageUpdatedEvent, buildSessionReadyEvent, buildTurnCompletedEvent, buildTurnCreatedEvent, buildTurnFailedEvent, buildTurnOutputEvent, buildTurnStartedEvent, contextUsageFromPiHook, type InternalEvent, type SessionMessageUpdatedReason } from "./events.js";
import { EventReporter } from "./reporter.js";
import { loadSessionContext } from "./session.js";

interface ReporterLike {
  report(context: { internalEventUrl: string }, event: InternalEvent): Promise<boolean>;
}

export interface PontiaPiExtensionDependencies {
  env?: EnvLike;
  loadContext?: (env: EnvLike) => Promise<LoadTurnContextResult>;
  makeReporter?: (logFile: string) => ReporterLike;
  logDiagnostic?: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
  writeContext?: (contextFile: string, context: TurnContext) => Promise<void>;
  fetch?: typeof fetch;
  attachRuntimeBinding?: (options: { env: EnvLike; session: ReturnType<typeof piSessionDetailsFromHookContext>; fetch: typeof fetch }) => Promise<AttachRuntimeBindingResult>;
}

interface ActiveTurnState {
  context: TurnContext;
  logFile: string;
  reporter: ReporterLike;
  output: string;
  ended: boolean;
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

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value : undefined;
}

function callSessionManagerString(sessionManager: unknown, method: string): string | undefined {
  if (!sessionManager || typeof sessionManager !== "object") return undefined;
  const fn = (sessionManager as Record<string, unknown>)[method];
  if (typeof fn !== "function") return undefined;
  try {
    return optionalString(fn.call(sessionManager));
  } catch {
    return undefined;
  }
}

function piSessionDetailsFromHookContext(ctx: unknown): Pick<import("./session.js").SessionContext, "clientSessionKey" | "clientSessionFile" | "clientSessionDir" | "clientCwd"> {
  const sessionManager = ctx && typeof ctx === "object" ? (ctx as Record<string, unknown>).sessionManager : undefined;
  return {
    clientSessionKey: callSessionManagerString(sessionManager, "getSessionId"),
    clientSessionFile: callSessionManagerString(sessionManager, "getSessionFile"),
    clientSessionDir: callSessionManagerString(sessionManager, "getSessionDir"),
    clientCwd: callSessionManagerString(sessionManager, "getCwd"),
  };
}

function externalApiUrl(env: EnvLike): string | undefined {
  return optionalString(env.PONTIA_EXTERNAL_API_URL)?.replace(/\/+$/, "");
}

function hasManagedRuntimeEnvIntent(env: EnvLike): boolean {
  return Boolean(
    optionalString(env.PONTIA_RUNTIME_DIR) ||
      optionalString(env.PONTIA_CURRENT_TURN_FILE) ||
      optionalString(env.PONTIA_SESSION_ID) ||
      optionalString(env.PONTIA_RUNTIME_INSTANCE_ID) ||
      optionalString(env.PONTIA_INTERNAL_EVENT_URL),
  );
}

function freshTurnId(): string {
  return `turn_${randomUUID()}`;
}

async function writeCurrentTurnContext(contextFile: string, context: TurnContext): Promise<void> {
  await mkdir(dirname(contextFile), { recursive: true });
  await writeFile(
    contextFile,
    `${JSON.stringify({
      client_type: context.clientType,
      input: context.input,
      internal_event_url: context.internalEventUrl,
      runtime_instance_id: context.runtimeInstanceId,
      session_id: context.sessionId,
      turn_id: context.turnId,
    }, null, 2)}\n`,
    "utf8",
  );
}

async function parseJsonResponse(response: Response): Promise<unknown> {
  const text = await response.text().catch(() => "");
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

function responseDataRecord(body: unknown): Record<string, unknown> | undefined {
  if (!body || typeof body !== "object" || Array.isArray(body)) return undefined;
  const data = (body as Record<string, unknown>).data;
  if (!data || typeof data !== "object" || Array.isArray(data)) return undefined;
  return data as Record<string, unknown>;
}

async function fetchJson(fetchImpl: typeof fetch, url: string, token: string): Promise<unknown> {
  const response = await fetchImpl(url, { headers: { Authorization: `Bearer ${token}` } });
  const body = await parseJsonResponse(response);
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return body;
}

async function loadProfileSystemPrompt(env: EnvLike, fetchImpl: typeof fetch): Promise<string | undefined> {
  const baseUrl = externalApiUrl(env);
  const token = optionalString(env.PONTIA_EXTERNAL_API_TOKEN);
  const sessionId = optionalString(env.PONTIA_SESSION_ID);
  if (!baseUrl || !token || !sessionId) return undefined;

  const sessionBody = await fetchJson(fetchImpl, `${baseUrl}/sessions/${encodeURIComponent(sessionId)}`, token);
  const session = responseDataRecord(sessionBody)?.session;
  if (!session || typeof session !== "object" || Array.isArray(session)) return undefined;
  const sessionRecord = session as Record<string, unknown>;
  const profileId = optionalString(sessionRecord.execution_profile_id);
  const profileVersion = optionalString(sessionRecord.execution_profile_version);
  if (!profileId) return undefined;

  const profileUrl = profileVersion
    ? `${baseUrl}/agent-profiles/${encodeURIComponent(profileId)}/versions/${encodeURIComponent(profileVersion)}`
    : `${baseUrl}/agent-profiles/${encodeURIComponent(profileId)}`;
  const profileBody = await fetchJson(fetchImpl, profileUrl, token);
  const profile = responseDataRecord(profileBody)?.agent_profile;
  if (!profile || typeof profile !== "object" || Array.isArray(profile)) return undefined;
  return optionalString((profile as Record<string, unknown>).system_prompt_template);
}

export function createPontiaPiExtension(pi: ExtensionAPI, dependencies: PontiaPiExtensionDependencies = {}): void {
  const env = dependencies.env ?? process.env;
  const contextLoader = dependencies.loadContext ?? loadTurnContext;
  const makeReporter = dependencies.makeReporter ?? ((logFile: string) => new EventReporter({ logFile }));
  const logDiagnostic = dependencies.logDiagnostic ?? appendDiagnostic;
  const writeContext = dependencies.writeContext ?? writeCurrentTurnContext;
  const fetchImpl = dependencies.fetch ?? fetch;
  const attachRuntime = dependencies.attachRuntimeBinding ?? attachRuntimeBinding;

  let activeTurn: ActiveTurnState | undefined;
  let readyReported = false;
  let autoBoundContext: BoundPontiaContext | undefined;
  let unboundForSession = false;
  let pendingPrompt: string | undefined;
  let pendingRefreshTimer: ReturnType<typeof setTimeout> | undefined;
  let lastContextUsageJson: string | undefined;
  const endedTurnIds = new Set<string>();

  function clearPendingRefresh(): void {
    if (!pendingRefreshTimer) return;
    clearTimeout(pendingRefreshTimer);
    pendingRefreshTimer = undefined;
  }

  async function scheduleMessageRefresh(reason: SessionMessageUpdatedReason): Promise<void> {
    if (!activeTurn || activeTurn.ended) return;
    if (reason !== "update") {
      clearPendingRefresh();
      await activeTurn.reporter.report(activeTurn.context, buildSessionMessageUpdatedEvent(activeTurn.context, reason));
      return;
    }
    if (pendingRefreshTimer) return;
    const state = activeTurn;
    pendingRefreshTimer = setTimeout(() => {
      pendingRefreshTimer = undefined;
      if (activeTurn !== state || state.ended) return;
      void state.reporter.report(state.context, buildSessionMessageUpdatedEvent(state.context, "update"));
    }, 100);
  }

  async function reportFinalMessageRefresh(state: ActiveTurnState): Promise<void> {
    clearPendingRefresh();
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
    const currentSystemPrompt = typeof eventRecord.systemPrompt === "string" ? eventRecord.systemPrompt : "";
    try {
      const profilePrompt = await loadProfileSystemPrompt(env, fetchImpl);
      if (!profilePrompt) return { systemPrompt: currentSystemPrompt };
      return { systemPrompt: `${currentSystemPrompt}\n\n${profilePrompt}` };
    } catch (error) {
      await logDiagnostic(env.PONTIA_PI_HOOK_LOG ?? defaultHookLogFile(env), {
        level: "warn",
        code: "system_prompt_append_failed",
        message: "failed to append pontia execution profile system prompt",
        details: error instanceof Error ? error.message : String(error),
      });
      return { systemPrompt: currentSystemPrompt };
    }
  });

  pi.on("session_start", async (event, ctx) => {
    if (readyReported || unboundForSession) return;
    const reason = (event as unknown as Record<string, unknown> | undefined)?.reason;
    if (reason !== "startup") return;

    try {
      const sessionDetails = piSessionDetailsFromHookContext(ctx);
      const loaded = await loadSessionContext(env);
      if (loaded.ok) {
        if (!sessionDetails.clientSessionKey) {
          await logDiagnostic(loaded.logFile, {
            level: "error",
            code: "missing_pi_client_session_key",
            message: "ctx.sessionManager.getSessionId() is required to report pontia ready signal",
          });
          return;
        }
        const context = { ...loaded.context, ...sessionDetails };
        readyReported = await makeReporter(loaded.logFile).report(context, buildSessionReadyEvent(context));
        return;
      }

      if (hasManagedRuntimeEnvIntent(env)) return;

      const bindingResult = await attachRuntime({ env, session: sessionDetails, fetch: fetchImpl });
      if (!bindingResult.ok) {
        unboundForSession = true;
        await logDiagnostic(bindingResult.logFile, {
          level: "warn",
          code: "pontia_auto_attach_unbound",
          message: "pontia server unavailable or runtime binding upsert failed; this pi session will not report pontia events",
          details: { code: bindingResult.code, reason: bindingResult.reason },
        });
        return;
      }

      autoBoundContext = bindingResult.context;
      const context = { ...bindingResult.context, ...sessionDetails };
      readyReported = await makeReporter(bindingResult.logFile).report(context, buildSessionReadyEvent(context));
    } catch (error) {
      const logFile = env.PONTIA_PI_HOOK_LOG ?? "pi-hook.log";
      await logDiagnostic(logFile, {
        level: "error",
        code: "unexpected_extension_exception",
        message: "failed to report pontia ready signal",
        details: error instanceof Error ? error.message : String(error),
      });
    }
  });

  pi.on("agent_start", async (_event, ctx) => {
    if (unboundForSession) return;
    try {
      const contextEnv = autoBoundContext
        ? {
            ...env,
            PONTIA_CURRENT_TURN_FILE: autoBoundContext.currentTurnFile,
            PONTIA_INTERNAL_EVENT_URL: autoBoundContext.internalEventUrl,
            PONTIA_RUNTIME_INSTANCE_ID: autoBoundContext.runtimeInstanceId,
          }
        : env;
      const loaded = await contextLoader(contextEnv);
      let context: TurnContext;
      let contextFile: string;
      let logFile: string;
      let createdFromTuiPrompt = false;

      if (!loaded.ok) {
        if (!autoBoundContext) {
          activeTurn = undefined;
          if (!loaded.silent && ctx?.hasUI) ctx.ui.notify(`pontia: ${loaded.reason}`, "warning");
          return;
        }
        context = {
          sessionId: autoBoundContext.sessionId,
          turnId: freshTurnId(),
          runtimeInstanceId: autoBoundContext.runtimeInstanceId,
          input: pendingPrompt,
          clientType: "pi",
          internalEventUrl: autoBoundContext.internalEventUrl,
        };
        contextFile = autoBoundContext.currentTurnFile;
        logFile = loaded.logFile;
        createdFromTuiPrompt = true;
        await writeContext(contextFile, context);
      } else {
        context = loaded.context;
        contextFile = loaded.contextFile;
        logFile = loaded.logFile;
        if (autoBoundContext && (context.sessionId !== autoBoundContext.sessionId || context.runtimeInstanceId !== autoBoundContext.runtimeInstanceId || context.clientType !== "pi")) {
          context = {
            sessionId: autoBoundContext.sessionId,
            turnId: freshTurnId(),
            runtimeInstanceId: autoBoundContext.runtimeInstanceId,
            input: pendingPrompt,
            clientType: "pi",
            internalEventUrl: autoBoundContext.internalEventUrl,
          };
          contextFile = autoBoundContext.currentTurnFile;
          createdFromTuiPrompt = true;
          await writeContext(contextFile, context);
        } else if (endedTurnIds.has(context.turnId)) {
          context = {
            ...context,
            turnId: freshTurnId(),
            input: pendingPrompt ?? context.input,
          };
          createdFromTuiPrompt = true;
          await writeContext(contextFile, context);
        }
      }

      const reporter = makeReporter(logFile);
      lastContextUsageJson = undefined;
      activeTurn = {
        context,
        logFile: loaded.logFile,
        reporter,
        output: "",
        ended: false,
      };
      if (createdFromTuiPrompt) {
        const createdOk = await reporter.report(context, buildTurnCreatedEvent(context));
        if (!createdOk) {
          activeTurn = undefined;
          return;
        }
      }
      await reporter.report(context, buildTurnStartedEvent(context));
    } catch (error) {
      activeTurn = undefined;
      const logFile = env.PONTIA_PI_HOOK_LOG ?? "pi-hook.log";
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
      void scheduleMessageRefresh("update");
      return;
    }

    const delta = assistantDeltaFromEvent(event);
    if (delta) {
      activeTurn.output += delta;
      void scheduleMessageRefresh("update");
    }
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
    endedTurnIds.add(state.context.turnId);
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
