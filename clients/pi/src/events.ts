import { randomUUID } from "node:crypto";
import type { TurnContext } from "./context.js";
import type { SessionContext } from "./session.js";

export type InternalEventType = "session.ready" | "session.message_updated" | "session.context_usage_updated" | "turn.created" | "turn.started" | "turn.output" | "turn.completed" | "turn.failed";

export interface ContextUsagePayload {
  used_tokens: number | null;
  max_tokens: number | null;
  remaining_tokens: number | null;
  usage_ratio: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  cache_tokens: number | null;
  model: string | null;
  confidence: "exact" | "estimated" | "unknown";
}

export type SessionMessageUpdatedReason = "append" | "update" | "final";

interface BaseInternalEvent {
  event_id: string;
  session_id: string;
  client_type: "pi";
  type: InternalEventType;
  time: string;
  seq: null;
  payload: Record<string, unknown>;
}

export type InternalEvent =
  | (BaseInternalEvent & {
      turn_id: null;
      source: "agent_client";
      type: "session.ready";
    })
  | (BaseInternalEvent & {
      turn_id: null;
      source: "agent_client";
      type: "session.message_updated";
    })
  | (BaseInternalEvent & {
      turn_id: string | null;
      source: "agent_client";
      type: "session.context_usage_updated";
    })
  | (BaseInternalEvent & {
      turn_id: string;
      source: "agent_client";
      type: "turn.created";
    })
  | (BaseInternalEvent & {
      turn_id: string;
      source: "agent_adapter";
      type: "turn.started" | "turn.output" | "turn.completed" | "turn.failed";
    });

type AdapterTurnInternalEvent = Extract<InternalEvent, { source: "agent_adapter" }>;

function baseAdapterTurnEvent(context: TurnContext, type: AdapterTurnInternalEvent["type"]): Omit<AdapterTurnInternalEvent, "payload"> {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: context.turnId,
    source: "agent_adapter",
    client_type: "pi",
    type,
    time: new Date().toISOString(),
    seq: null,
  };
}

export function buildTurnCreatedEvent(context: TurnContext): InternalEvent {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: context.turnId,
    source: "agent_client",
    client_type: "pi",
    type: "turn.created",
    time: new Date().toISOString(),
    seq: null,
    payload: {
      runtime_instance_id: context.runtimeInstanceId,
      input: context.input ? { summary: context.input } : {},
      metadata: { source: "pi_tui" },
    },
  };
}

export function buildSessionMessageUpdatedEvent(context: TurnContext, reason: SessionMessageUpdatedReason): InternalEvent {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: null,
    source: "agent_client",
    client_type: "pi",
    type: "session.message_updated",
    time: new Date().toISOString(),
    seq: null,
    payload: { reason },
  };
}

export function buildSessionContextUsageUpdatedEvent(context: TurnContext, usage: ContextUsagePayload): InternalEvent {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: context.turnId ?? null,
    source: "agent_client",
    client_type: "pi",
    type: "session.context_usage_updated",
    time: new Date().toISOString(),
    seq: null,
    payload: { context_usage: usage },
  };
}

function optionalNonNegativeInteger(value: unknown): number | null | undefined {
  if (value == null) return null;
  return typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : undefined;
}

function optionalRatio(value: unknown): number | null | undefined {
  if (value == null) return null;
  return typeof value === "number" && value >= 0 && value <= 1 ? value : undefined;
}

function optionalNullableString(value: unknown): string | null | undefined {
  if (value == null) return null;
  return typeof value === "string" ? value : undefined;
}

function confidence(value: unknown): ContextUsagePayload["confidence"] | undefined {
  if (value == null) return "unknown";
  return value === "exact" || value === "estimated" || value === "unknown" ? value : undefined;
}

function modelNameFromRecord(record: Record<string, unknown>): string | null | undefined {
  const direct = optionalNullableString(record.model);
  if (direct !== undefined) return direct;
  const model = record.model;
  if (!model || typeof model !== "object" || Array.isArray(model)) return null;
  const modelRecord = model as Record<string, unknown>;
  return optionalNullableString(modelRecord.id) ?? optionalNullableString(modelRecord.modelId) ?? optionalNullableString(modelRecord.name);
}

function contextUsageFromPontiaShape(raw: unknown): ContextUsagePayload | undefined {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return undefined;
  const record = raw as Record<string, unknown>;
  const used_tokens = optionalNonNegativeInteger(record.used_tokens);
  const max_tokens = optionalNonNegativeInteger(record.max_tokens);
  const remaining_tokens = optionalNonNegativeInteger(record.remaining_tokens);
  const usage_ratio = optionalRatio(record.usage_ratio);
  const input_tokens = optionalNonNegativeInteger(record.input_tokens);
  const output_tokens = optionalNonNegativeInteger(record.output_tokens);
  const cache_tokens = optionalNonNegativeInteger(record.cache_tokens);
  const model = optionalNullableString(record.model);
  const parsedConfidence = confidence(record.confidence);

  if (
    used_tokens === undefined ||
    max_tokens === undefined ||
    remaining_tokens === undefined ||
    usage_ratio === undefined ||
    input_tokens === undefined ||
    output_tokens === undefined ||
    cache_tokens === undefined ||
    model === undefined ||
    parsedConfidence === undefined
  ) {
    return undefined;
  }

  return {
    used_tokens,
    max_tokens,
    remaining_tokens,
    usage_ratio,
    input_tokens,
    output_tokens,
    cache_tokens,
    model,
    confidence: parsedConfidence,
  };
}

function contextUsageFromPiMessage(message: unknown): ContextUsagePayload | undefined {
  if (!message || typeof message !== "object" || Array.isArray(message)) return undefined;
  const messageRecord = message as Record<string, unknown>;
  if (messageRecord.role !== "assistant") return undefined;
  const usage = messageRecord.usage;
  if (!usage || typeof usage !== "object" || Array.isArray(usage)) return undefined;
  const usageRecord = usage as Record<string, unknown>;
  const input_tokens = optionalNonNegativeInteger(usageRecord.input);
  const output_tokens = optionalNonNegativeInteger(usageRecord.output);
  const cache_read_tokens = optionalNonNegativeInteger(usageRecord.cacheRead);
  const cache_write_tokens = optionalNonNegativeInteger(usageRecord.cacheWrite);
  const used_tokens = optionalNonNegativeInteger(usageRecord.totalTokens);
  const model = modelNameFromRecord(messageRecord);

  if (
    input_tokens === undefined ||
    output_tokens === undefined ||
    cache_read_tokens === undefined ||
    cache_write_tokens === undefined ||
    used_tokens === undefined ||
    model === undefined
  ) {
    return undefined;
  }

  return {
    used_tokens,
    max_tokens: null,
    remaining_tokens: null,
    usage_ratio: null,
    input_tokens,
    output_tokens,
    cache_tokens: cache_read_tokens === null && cache_write_tokens === null ? null : (cache_read_tokens ?? 0) + (cache_write_tokens ?? 0),
    model,
    confidence: "estimated",
  };
}

export function contextUsageFromPiContext(ctx: unknown): ContextUsagePayload | undefined {
  if (!ctx || typeof ctx !== "object" || Array.isArray(ctx)) return undefined;
  const record = ctx as Record<string, unknown>;
  const getContextUsage = record.getContextUsage;
  if (typeof getContextUsage !== "function") return undefined;

  let rawUsage: unknown;
  try {
    rawUsage = getContextUsage.call(ctx);
  } catch {
    return undefined;
  }
  if (!rawUsage || typeof rawUsage !== "object" || Array.isArray(rawUsage)) return undefined;
  const usage = rawUsage as Record<string, unknown>;
  const used_tokens = optionalNonNegativeInteger(usage.tokens);
  const max_tokens = optionalNonNegativeInteger(usage.contextWindow);
  const percent = usage.percent == null ? null : typeof usage.percent === "number" && usage.percent >= 0 && usage.percent <= 100 ? usage.percent : undefined;
  const model = modelNameFromRecord(record);

  if (used_tokens === undefined || max_tokens === undefined || percent === undefined || model === undefined) {
    return undefined;
  }

  return {
    used_tokens,
    max_tokens,
    remaining_tokens: used_tokens !== null && max_tokens !== null ? Math.max(0, max_tokens - used_tokens) : null,
    usage_ratio: percent === null ? null : percent / 100,
    input_tokens: null,
    output_tokens: null,
    cache_tokens: null,
    model,
    confidence: "estimated",
  };
}

export function contextUsageFromPiEvent(event: unknown): ContextUsagePayload | undefined {
  if (!event || typeof event !== "object" || Array.isArray(event)) return undefined;
  const record = event as Record<string, unknown>;
  return contextUsageFromPontiaShape(record.context_usage) ?? contextUsageFromPiMessage(record.message);
}

export function buildTurnStartedEvent(context: TurnContext): InternalEvent {
  return {
    ...baseAdapterTurnEvent(context, "turn.started"),
    payload: {
      runtime_instance_id: context.runtimeInstanceId,
      input: context.input ? { summary: context.input } : {},
    },
  };
}

export function buildTurnOutputEvent(context: TurnContext, output: string): InternalEvent {
  return {
    ...baseAdapterTurnEvent(context, "turn.output"),
    payload: { output: { summary: output } },
  };
}

export function buildTurnCompletedEvent(context: TurnContext): InternalEvent {
  return {
    ...baseAdapterTurnEvent(context, "turn.completed"),
    payload: {},
  };
}

export function buildTurnFailedEvent(context: TurnContext, message: string): InternalEvent {
  return {
    ...baseAdapterTurnEvent(context, "turn.failed"),
    payload: { failure: { message } },
  };
}

export function buildSessionReadyEvent(context: SessionContext): InternalEvent {
  const payload: Record<string, unknown> = {
    runtime_instance_id: context.runtimeInstanceId,
  };
  if (context.clientSessionKey) payload.client_session_key = context.clientSessionKey;
  if (context.clientSessionFile) payload.client_session_file = context.clientSessionFile;
  if (context.clientSessionDir) payload.client_session_dir = context.clientSessionDir;
  if (context.clientCwd) payload.client_cwd = context.clientCwd;

  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: null,
    source: "agent_client",
    client_type: "pi",
    type: "session.ready",
    time: new Date().toISOString(),
    seq: null,
    payload,
  };
}
