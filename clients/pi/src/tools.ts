import toolContract from "../../tools/llmparty-tools.v1.json" with { type: "json" };
import type { EnvLike, LoadTurnContextResult } from "./context.js";
import { loadTurnContext } from "./context.js";
import { appendDiagnostic, type DiagnosticEntry } from "./diagnostics.js";

type FetchLike = typeof fetch;

type ToolContract = typeof toolContract;
type ToolSpec = ToolContract["tools"][number];

type AgentToolResult = {
  content: Array<{ type: "text"; text: string }>;
  details: unknown;
};

export interface LlmpartyAgentToolDependencies {
  env?: EnvLike;
  loadContext?: (env: EnvLike) => Promise<LoadTurnContextResult>;
  logDiagnostic?: (logFile: string, entry: DiagnosticEntry) => Promise<void>;
  fetch?: FetchLike;
}

export interface PiToolDefinitionLike {
  name: string;
  label: string;
  description: string;
  promptSnippet: string;
  promptGuidelines: string[];
  parameters: unknown;
  execute(
    toolCallId: string,
    params: unknown,
    signal?: AbortSignal,
    onUpdate?: unknown,
    ctx?: unknown,
  ): Promise<AgentToolResult>;
}

const TOOL_NAMES = ["getContext", "submitPlan", "applyPlan", "submitResult", "raiseSignal"] as const;
type LlmpartyToolName = (typeof TOOL_NAMES)[number];

function isLlmpartyToolName(name: string): name is LlmpartyToolName {
  return (TOOL_NAMES as readonly string[]).includes(name);
}

function piToolName(toolName: LlmpartyToolName): string {
  return toolName;
}

function responseText(toolName: string, payload: unknown): string {
  if (toolName === "getContext") {
    const text = agentContextText(payload);
    if (text) return text;
  }

  const result = resultRecord(payload);
  const lines = [`${toolName} succeeded.`];

  if (toolName === "submitPlan") {
    const validation = result?.validation;
    const accepted = booleanText(result?.accepted) ?? booleanText(validation && typeof validation === "object" ? (validation as Record<string, unknown>).ok : undefined);
    lines.push(`Accepted: ${accepted ?? "unknown"}`);
    const proposalId = stringField(result, "proposal_id");
    if (proposalId) lines.push(`Proposal ID: ${proposalId}`);
    return lines.join("\n");
  }

  if (toolName === "applyPlan") {
    const proposalId = stringField(result, "proposal_id");
    if (proposalId) lines.push(`Proposal ID: ${proposalId}`);
    return lines.join("\n");
  }

  if (toolName === "submitResult") {
    const status = stringField(result, "status");
    if (status) lines.push(`Status: ${status}`);
    const runId = stringField(result, "run_id");
    if (runId) lines.push(`Run ID: ${runId}`);
    return lines.join("\n");
  }

  if (toolName === "raiseSignal") {
    const signalId = stringField(result, "signal_id");
    if (signalId) lines.push(`Signal ID: ${signalId}`);
    const recorded = booleanText(result?.recorded);
    if (recorded) lines.push(`Recorded: ${recorded}`);
    return lines.join("\n");
  }

  return lines.join("\n");
}

function resultRecord(payload: unknown): Record<string, unknown> | undefined {
  if (!payload || typeof payload !== "object") return undefined;
  const result = (payload as Record<string, unknown>).result;
  return result && typeof result === "object" ? (result as Record<string, unknown>) : undefined;
}

function stringField(record: Record<string, unknown> | undefined, key: string): string | undefined {
  const value = record?.[key];
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function booleanText(value: unknown): string | undefined {
  return typeof value === "boolean" ? (value ? "yes" : "no") : undefined;
}

function errorBodyText(body: unknown): string | undefined {
  if (typeof body === "string") return body;
  if (!body || typeof body !== "object") return undefined;
  const record = body as Record<string, unknown>;
  const error = record.error;
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const message = (error as Record<string, unknown>).message;
    if (typeof message === "string") return message;
  }
  const message = record.message;
  return typeof message === "string" ? message : undefined;
}

function agentContextText(payload: unknown): string | undefined {
  if (typeof payload === "string") return payload;
  if (!payload || typeof payload !== "object") return undefined;
  const record = payload as Record<string, unknown>;
  const result = record.result;
  if (typeof result === "string") return result;
  if (result && typeof result === "object") {
    const text = (result as Record<string, unknown>).text;
    if (typeof text === "string") return text;
  }
  const text = record.text;
  return typeof text === "string" ? text : undefined;
}

function failureResult(message: string, details: unknown): AgentToolResult {
  return {
    content: [{ type: "text", text: message }],
    details,
  };
}

export function internalAgentToolUrl(internalEventUrl: string, toolName: string): string {
  const trimmed = internalEventUrl.replace(/\/+$/, "");
  if (trimmed.endsWith("/internal/v1/events")) {
    return `${trimmed.slice(0, -"/events".length)}/agent-tools/${encodeURIComponent(toolName)}`;
  }
  return `${trimmed}/agent-tools/${encodeURIComponent(toolName)}`;
}

async function parseResponseBody(response: Response): Promise<unknown> {
  const text = await response.text().catch(() => "");
  if (text.length === 0) return null;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

function buildTool(spec: ToolSpec, dependencies: Required<LlmpartyAgentToolDependencies>): PiToolDefinitionLike | undefined {
  if (!isLlmpartyToolName(spec.name)) return undefined;

  const piName = piToolName(spec.name);
  return {
    name: piName,
    label: spec.title,
    description: spec.description,
    promptSnippet: spec.title,
    promptGuidelines: [],
    parameters: spec.inputSchema,
    async execute(_toolCallId, params, signal) {
      const loaded = await dependencies.loadContext(dependencies.env);
      const logFile = loaded.logFile;
      if (!loaded.ok) {
        await dependencies.logDiagnostic(logFile, {
          level: "warn",
          code: "agent_tool_context_unavailable",
          message: `${piName} unavailable: ${loaded.reason}`,
          details: { contextFile: loaded.contextFile },
        });
        return failureResult(`${piName} unavailable: ${loaded.reason}`, { ok: false, reason: loaded.reason });
      }

      const request = {
        session_id: loaded.context.sessionId,
        turn_id: loaded.context.turnId,
        runtime_instance_id: loaded.context.runtimeInstanceId,
        input: params && typeof params === "object" ? params : {},
      };
      const url = internalAgentToolUrl(loaded.context.internalEventUrl, spec.name);

      try {
        const response = await dependencies.fetch(url, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(request),
          signal,
        });
        const body = await parseResponseBody(response);

        if (!response.ok) {
          await dependencies.logDiagnostic(logFile, {
            level: "error",
            code: "agent_tool_post_failed",
            message: `${piName} failed: ${response.status} ${response.statusText}`,
            details: { tool: spec.name, status: response.status, body },
          });
          const bodyText = errorBodyText(body);
          return failureResult(`${piName} failed: ${response.status} ${response.statusText}${bodyText ? `\n${bodyText}` : ""}`, {
            ok: false,
            status: response.status,
            body,
          });
        }

        return {
          content: [{ type: "text", text: responseText(piName, body) }],
          details: body,
        };
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        await dependencies.logDiagnostic(logFile, {
          level: "error",
          code: "agent_tool_post_exception",
          message: `${piName} Internal Agent Tool API POST failed`,
          details: { tool: spec.name, error: message },
        });
        return failureResult(`${piName} failed: ${message}`, { ok: false, error: message });
      }
    },
  };
}

export function buildLlmpartyTools(dependencies: LlmpartyAgentToolDependencies = {}): PiToolDefinitionLike[] {
  const resolved: Required<LlmpartyAgentToolDependencies> = {
    env: dependencies.env ?? process.env,
    loadContext: dependencies.loadContext ?? loadTurnContext,
    logDiagnostic: dependencies.logDiagnostic ?? appendDiagnostic,
    fetch: dependencies.fetch ?? fetch,
  };

  return toolContract.tools
    .map((spec) => buildTool(spec, resolved))
    .filter((tool): tool is PiToolDefinitionLike => Boolean(tool));
}
