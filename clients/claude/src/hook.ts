import { readFile } from "node:fs/promises";
import { stdin as processStdin } from "node:process";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";
import { claimTurnContext, defaultHookLogFile, loadCurrentTurnByClientSession, loadSessionContext, type EnvLike, type SessionContext, type TurnContext } from "./context.js";
import { buildSessionExitedEvent, buildSessionReadyEvent, buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnOutputEvent, buildTurnStartedEvent, newPontiaTurnId, type InternalEvent } from "./events.js";
import { optionalString } from "./internal-api.js";
import { EventReporter } from "./reporter.js";
import { bindManualSession, hasExistingAgentBinding, type ClaudeSessionDetails } from "./runtime-binding.js";
import { isActiveRegisteredWorkspace } from "./workspace.js";

export interface ClaudeHookInput {
  hook_event_name?: string;
  session_id?: string;
  transcript_path?: string;
  cwd?: string;
  prompt?: string;
  last_assistant_message?: string;
  error?: string;
  error_details?: string;
  reason?: string;
  [key: string]: unknown;
}

interface ReporterLike {
  report(context: { internalEventUrl: string }, event: InternalEvent): Promise<boolean>;
}

export interface ClaudeHookDependencies {
  env?: EnvLike;
  fetch?: typeof fetch;
  makeReporter?: (logFile: string) => ReporterLike;
  logDiagnostic?: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
}

function sessionDetailsFromHook(input: ClaudeHookInput): ClaudeSessionDetails {
  return {
    clientSessionKey: optionalString(input.session_id),
    transcriptPath: optionalString(input.transcript_path),
    clientCwd: optionalString(input.cwd),
  };
}

function activeTurnContext(context: TurnContext): TurnContext & { turnId: string } {
  return { ...context, turnId: context.turnId ?? newPontiaTurnId() };
}

function failureMessage(input: ClaudeHookInput): string {
  const error = optionalString(input.error) ?? "Claude Code turn failed";
  const details = optionalString(input.error_details);
  return details ? `${error}: ${details}` : error;
}

function assistantTextFromTranscriptEntry(entry: unknown): string | undefined {
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) return undefined;
  const record = entry as Record<string, unknown>;
  if (record.type !== "assistant") return undefined;
  const message = record.message;
  if (!message || typeof message !== "object" || Array.isArray(message)) return undefined;
  const content = (message as Record<string, unknown>).content;
  if (typeof content === "string") return optionalString(content);
  if (!Array.isArray(content)) return undefined;
  const texts = content
    .filter((block): block is Record<string, unknown> => Boolean(block) && typeof block === "object" && !Array.isArray(block))
    .filter((block) => block.type === "text")
    .map((block) => optionalString(block.text))
    .filter((text): text is string => Boolean(text));
  return optionalString(texts.join("\n"));
}

function isUserTranscriptEntry(entry: unknown): boolean {
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) return false;
  const record = entry as Record<string, unknown>;
  if (record.type !== "user") return false;
  const message = record.message;
  return Boolean(message && typeof message === "object" && !Array.isArray(message) && (message as Record<string, unknown>).role === "user");
}

async function latestAssistantTextFromTranscript(path: string): Promise<string | undefined> {
  let text: string;
  try {
    text = await readFile(path, "utf8");
  } catch {
    return undefined;
  }
  let afterLastUser = false;
  let latestText: string | undefined;
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    let entry: unknown;
    try {
      entry = JSON.parse(line);
    } catch {
      continue;
    }
    if (isUserTranscriptEntry(entry)) {
      afterLastUser = true;
      latestText = undefined;
      continue;
    }
    if (!afterLastUser) continue;
    latestText = assistantTextFromTranscriptEntry(entry) ?? latestText;
  }
  return latestText;
}

async function stopOutput(input: ClaudeHookInput): Promise<string | undefined> {
  return optionalString(input.last_assistant_message) ?? await latestAssistantTextFromTranscript(optionalString(input.transcript_path) ?? "");
}

async function reportReadyForManagedSession(input: ClaudeHookInput, deps: RequiredDeps): Promise<void> {
  const loaded = await loadSessionContext(deps.env);
  const logFile = loaded.logFile;
  if (!loaded.ok) return;
  const details = sessionDetailsFromHook(input);
  const workspaceActive = await isActiveRegisteredWorkspace(deps.env, deps.fetchImpl, details.clientCwd);
  if (workspaceActive !== true) {
    await deps.logDiagnostic(logFile, {
      level: "info",
      code: workspaceActive === false ? "workspace_not_active" : "workspace_check_unavailable",
      message: workspaceActive === false
        ? "current claude workspace is not an active registered pontia workspace; pontia reporting disabled"
        : "could not verify active registered pontia workspace; pontia reporting disabled",
      details: { client_cwd: details.clientCwd },
    });
    return;
  }
  const context: SessionContext = { ...loaded.context, ...details };
  await deps.makeReporter(logFile).report(context, buildSessionReadyEvent(context));
}

async function reportReadyForExistingManualBinding(input: ClaudeHookInput, deps: RequiredDeps): Promise<void> {
  const details = sessionDetailsFromHook(input);
  const logFile = defaultHookLogFile(deps.env);
  const workspaceActive = await isActiveRegisteredWorkspace(deps.env, deps.fetchImpl, details.clientCwd);
  if (workspaceActive !== true) {
    await deps.logDiagnostic(logFile, {
      level: "info",
      code: workspaceActive === false ? "workspace_not_active" : "workspace_check_unavailable",
      message: workspaceActive === false
        ? "current claude workspace is not an active registered pontia workspace; pontia reporting disabled"
        : "could not verify active registered pontia workspace; pontia reporting disabled",
      details: { client_cwd: details.clientCwd },
    });
    return;
  }
  if (!(await hasExistingAgentBinding(deps.env, deps.fetchImpl, details))) return;
  const context = await bindManualSession(deps.env, deps.fetchImpl, details);
  if (!context) return;
  await deps.makeReporter(logFile).report(context, buildSessionReadyEvent(context));
}

async function handleSessionStart(input: ClaudeHookInput, deps: RequiredDeps): Promise<void> {
  if (optionalString(deps.env.PONTIA_SESSION_ID)) {
    await reportReadyForManagedSession(input, deps);
  } else {
    await reportReadyForExistingManualBinding(input, deps);
  }
}

async function handleSessionEnd(input: ClaudeHookInput, deps: RequiredDeps): Promise<void> {
  const loaded = await loadSessionContext(deps.env);
  if (!loaded.ok) return;
  await deps.makeReporter(loaded.logFile).report(loaded.context, buildSessionExitedEvent(loaded.context, optionalString(input.reason) ?? "exit"));
}

async function manualTurnContext(input: ClaudeHookInput, deps: RequiredDeps, logFile: string): Promise<TurnContext | undefined> {
  const details = sessionDetailsFromHook(input);
  const workspaceActive = await isActiveRegisteredWorkspace(deps.env, deps.fetchImpl, details.clientCwd);
  if (workspaceActive !== true) {
    await deps.logDiagnostic(logFile, {
      level: "info",
      code: workspaceActive === false ? "workspace_not_active" : "workspace_check_unavailable",
      message: workspaceActive === false
        ? "current claude workspace is not an active registered pontia workspace; pontia reporting disabled"
        : "could not verify active registered pontia workspace; pontia reporting disabled",
      details: { client_cwd: details.clientCwd },
    });
    return undefined;
  }
  const session = await bindManualSession(deps.env, deps.fetchImpl, details);
  if (!session) return undefined;
  await deps.makeReporter(logFile).report(session, buildSessionReadyEvent(session));
  return {
    sessionId: session.sessionId,
    runtimeInstanceId: session.runtimeInstanceId,
    clientType: "claude",
    internalEventUrl: session.internalEventUrl,
    input: optionalString(input.prompt),
  };
}

async function handleUserPromptSubmit(input: ClaudeHookInput, deps: RequiredDeps): Promise<void> {
  const prompt = optionalString(input.prompt);
  const claimed = await claimTurnContext(deps.env, deps.fetchImpl);
  const logFile = claimed.logFile;
  let context: TurnContext | undefined;
  if (claimed.ok) {
    context = { ...claimed.context, input: prompt ?? claimed.context.input };
  } else {
    const loaded = await loadSessionContext(deps.env);
    if (loaded.ok) {
      context = { ...loaded.context, input: prompt };
    } else if (claimed.silent) {
      context = await manualTurnContext(input, deps, logFile);
    }
  }
  if (!context) return;
  const active = activeTurnContext(context);
  await deps.makeReporter(logFile).report(active, buildTurnStartedEvent(active));
}

async function handleStop(input: ClaudeHookInput, deps: RequiredDeps): Promise<void> {
  const clientSessionKey = optionalString(input.session_id);
  if (!clientSessionKey) return;
  const loaded = await loadCurrentTurnByClientSession(deps.env, deps.fetchImpl, clientSessionKey);
  if (!loaded.ok) {
    if (!loaded.silent) await deps.logDiagnostic(loaded.logFile, { level: "warn", code: "current_turn_lookup_failed", message: loaded.reason });
    return;
  }
  const context = activeTurnContext(loaded.context);
  const reporter = deps.makeReporter(loaded.logFile);
  const output = await stopOutput(input);
  if (output) {
    const ok = await reporter.report(context, buildTurnOutputEvent(context, output));
    if (!ok) return;
  }
  await reporter.report(context, buildTurnCompletedEvent(context));
}

async function handleStopFailure(input: ClaudeHookInput, deps: RequiredDeps): Promise<void> {
  const clientSessionKey = optionalString(input.session_id);
  if (!clientSessionKey) return;
  const loaded = await loadCurrentTurnByClientSession(deps.env, deps.fetchImpl, clientSessionKey);
  if (!loaded.ok) return;
  const context = activeTurnContext(loaded.context);
  await deps.makeReporter(loaded.logFile).report(context, buildTurnFailedEvent(context, failureMessage(input)));
}

interface RequiredDeps {
  env: EnvLike;
  fetchImpl: typeof fetch;
  makeReporter: (logFile: string) => ReporterLike;
  logDiagnostic: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
}

function requiredDeps(dependencies: ClaudeHookDependencies): RequiredDeps {
  const env = dependencies.env ?? process.env;
  const fetchImpl = dependencies.fetch ?? fetch;
  return {
    env,
    fetchImpl,
    makeReporter: dependencies.makeReporter ?? ((logFile: string) => new EventReporter({ logFile, fetch: fetchImpl })),
    logDiagnostic: dependencies.logDiagnostic ?? appendDiagnostic,
  };
}

export async function runClaudeHook(input: ClaudeHookInput, dependencies: ClaudeHookDependencies = {}): Promise<void> {
  const deps = requiredDeps(dependencies);
  try {
    switch (input.hook_event_name) {
      case "SessionStart":
        await handleSessionStart(input, deps);
        break;
      case "UserPromptSubmit":
        await handleUserPromptSubmit(input, deps);
        break;
      case "Stop":
        await handleStop(input, deps);
        break;
      case "StopFailure":
        await handleStopFailure(input, deps);
        break;
      case "SessionEnd":
        await handleSessionEnd(input, deps);
        break;
      default:
        break;
    }
  } catch (error) {
    await deps.logDiagnostic(defaultHookLogFile(deps.env), {
      level: "error",
      code: "unexpected_hook_exception",
      message: "failed to process Claude Code pontia hook",
      details: error instanceof Error ? error.message : String(error),
    });
  }
}

async function readStdin(): Promise<string> {
  let data = "";
  for await (const chunk of processStdin) data += chunk;
  return data;
}

export async function main(): Promise<void> {
  const text = await readStdin();
  if (!text.trim()) return;
  try {
    await runClaudeHook(JSON.parse(text) as ClaudeHookInput);
  } catch {
    // Hook must no-op on malformed input.
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  void main();
}
