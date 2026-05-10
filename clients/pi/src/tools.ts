import toolsManifest from "../../tools/llmparty-tools.v1.json" with { type: "json" };
import type { EnvLike } from "./context.js";

export interface ToolRuntimeDependencies {
  env?: EnvLike;
  fetch?: typeof fetch;
}

interface SharedToolDefinition {
  name: string;
  title?: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

interface ExternalApiResponse {
  data?: unknown;
  error?: { code?: string; message?: string } | null;
}

interface ToolResult {
  content: Array<{ type: "text"; text: string }>;
  details?: unknown;
}

interface RuntimeConfig {
  baseUrl: string;
  token: string;
}

function textResult(text: string, details?: unknown): ToolResult {
  return {
    content: [{ type: "text", text }],
    details,
  };
}

function successResult(action: string, details: unknown): ToolResult {
  return textResult(`${action}: ${JSON.stringify(details)}`, details);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function runtimeConfig(env: EnvLike): RuntimeConfig | ToolResult {
  const baseUrl = optionalString(env.LLMPARTY_EXTERNAL_API_URL);
  const token = optionalString(env.LLMPARTY_EXTERNAL_API_TOKEN);
  const errors: string[] = [];
  if (!baseUrl) errors.push("LLMPARTY_EXTERNAL_API_URL is required");
  if (!token) errors.push("LLMPARTY_EXTERNAL_API_TOKEN is required");
  if (errors.length > 0) return textResult(`llmparty tool unavailable: ${errors.join("; ")}`, { ok: false, errors });
  return { baseUrl: baseUrl!.replace(/\/+$/, ""), token: token! };
}

function isToolResult(value: RuntimeConfig | ToolResult): value is ToolResult {
  return "content" in value;
}

function manifestTools(): SharedToolDefinition[] {
  return (toolsManifest as { tools: SharedToolDefinition[] }).tools;
}

function idempotencyKey(toolCallId: string): string {
  return `pi-tool-${toolCallId}`;
}

async function postExternalApi(
  fetchImpl: typeof fetch,
  config: RuntimeConfig,
  path: string,
  body: unknown,
  toolCallId: string,
): Promise<unknown> {
  const response = await fetchImpl(`${config.baseUrl}${path}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${config.token}`,
      "content-type": "application/json",
      "idempotency-key": idempotencyKey(toolCallId),
    },
    body: JSON.stringify(body),
  });

  let parsed: ExternalApiResponse;
  try {
    parsed = (await response.json()) as ExternalApiResponse;
  } catch (error) {
    throw new Error(`llmparty external API returned non-JSON response (${response.status}): ${error instanceof Error ? error.message : String(error)}`);
  }

  if (!response.ok || parsed.error) {
    const message = parsed.error?.message ?? `HTTP ${response.status}`;
    throw new Error(`llmparty external API request failed: ${message}`);
  }

  return parsed.data;
}

function dataRecord(data: unknown): Record<string, unknown> {
  return data && typeof data === "object" && !Array.isArray(data) ? (data as Record<string, unknown>) : {};
}

function sessionIdFromData(data: unknown): string | undefined {
  const record = dataRecord(data);
  const direct = optionalString(record.session_id);
  if (direct) return direct;
  const session = dataRecord(record.session);
  return optionalString(session.session_id);
}

function messageIdFromData(data: unknown): string | undefined {
  const record = dataRecord(data);
  const direct = optionalString(record.message_id);
  if (direct) return direct;
  const message = dataRecord(record.inbox_message);
  return optionalString(message.message_id);
}

async function executeSafely(action: string, run: () => Promise<ToolResult>): Promise<ToolResult> {
  try {
    return await run();
  } catch (error) {
    return textResult(`${action} failed: ${error instanceof Error ? error.message : String(error)}`, { ok: false });
  }
}

function createSessionTool(definition: SharedToolDefinition, env: EnvLike, fetchImpl: typeof fetch) {
  return {
    name: definition.name,
    label: definition.title ?? definition.name,
    description: definition.description,
    parameters: definition.inputSchema,
    async execute(toolCallId: string, params: Record<string, unknown>) {
      return executeSafely(definition.name, async () => {
        const config = runtimeConfig(env);
        if (isToolResult(config)) return config;

        const initialMessage = optionalString(params.initial_message);
        const body: Record<string, unknown> = {
          client_type: optionalString(params.client_type) ?? optionalString(env.LLMPARTY_CLIENT_TYPE) ?? "pi",
          workspace: optionalString(params.workspace) ?? optionalString(env.LLMPARTY_WORKSPACE),
        };
        if (initialMessage) body.initial_task = { input: initialMessage, metadata: {} };
        body.metadata = dataRecord(params.metadata);

        const data = await postExternalApi(fetchImpl, config, "/sessions", body, toolCallId);
        const details = { session_id: sessionIdFromData(data), ...(dataRecord(data)) };
        return successResult("llmparty session created", details);
      });
    },
  };
}

function sendMessageTool(definition: SharedToolDefinition, env: EnvLike, fetchImpl: typeof fetch) {
  return {
    name: definition.name,
    label: definition.title ?? definition.name,
    description: definition.description,
    parameters: definition.inputSchema,
    async execute(toolCallId: string, params: Record<string, unknown>) {
      return executeSafely(definition.name, async () => {
        const config = runtimeConfig(env);
        if (isToolResult(config)) return config;

        const sessionId = optionalString(params.session_id);
        const message = optionalString(params.message);
        if (!sessionId) return textResult("llmparty_send_message failed: session_id is required", { ok: false });
        if (!message) return textResult("llmparty_send_message failed: message is required", { ok: false });

        const data = await postExternalApi(
          fetchImpl,
          config,
          `/sessions/${encodeURIComponent(sessionId)}/inbox/messages`,
          {
            input: message,
            delivery_policy: optionalString(params.delivery_policy) ?? "after_idle",
            metadata: dataRecord(params.metadata),
          },
          toolCallId,
        );
        const details = { message_id: messageIdFromData(data), ...(dataRecord(data)) };
        return successResult("llmparty message queued", details);
      });
    },
  };
}

function exitSessionTool(definition: SharedToolDefinition, env: EnvLike, fetchImpl: typeof fetch) {
  return {
    name: definition.name,
    label: definition.title ?? definition.name,
    description: definition.description,
    parameters: definition.inputSchema,
    async execute(toolCallId: string, params: Record<string, unknown>) {
      return executeSafely(definition.name, async () => {
        const config = runtimeConfig(env);
        if (isToolResult(config)) return config;

        const sessionId = optionalString(env.LLMPARTY_SESSION_ID);
        if (!sessionId) return textResult("llmparty_exit_session failed: LLMPARTY_SESSION_ID is required", { ok: false });

        const data = await postExternalApi(
          fetchImpl,
          config,
          `/sessions/${encodeURIComponent(sessionId)}/terminate`,
          { reason: optionalString(params.reason) },
          toolCallId,
        );
        const details = { session_id: sessionIdFromData(data) ?? sessionId, ...(dataRecord(data)) };
        return successResult("llmparty session exit requested", details);
      });
    },
  };
}

export function buildLlmpartyTools(dependencies: ToolRuntimeDependencies = {}) {
  const env = dependencies.env ?? process.env;
  const fetchImpl = dependencies.fetch ?? fetch;
  const builders = [createSessionTool, sendMessageTool, exitSessionTool];
  return manifestTools().map((definition, index) => builders[index](definition, env, fetchImpl));
}
