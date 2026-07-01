import { randomUUID } from "node:crypto";
import type { SessionContext, TurnContext } from "./context.js";

export type InternalEventType = "session.ready" | "session.exited" | "turn.started" | "turn.output" | "turn.completed" | "turn.failed";

interface BaseInternalEvent {
  event_id: string;
  session_id: string;
  client_type: "claude";
  type: InternalEventType;
  time: string;
  seq: null;
  payload: Record<string, unknown>;
}

export type InternalEvent =
  | (BaseInternalEvent & { turn_id: null; source: "agent_client"; type: "session.ready" | "session.exited" })
  | (BaseInternalEvent & { turn_id: string; source: "agent_adapter"; type: "turn.started" | "turn.output" | "turn.completed" | "turn.failed" });

type ActiveTurnContext = TurnContext & { turnId: string };

export function newPontiaTurnId(): string {
  return `turn_${randomUUID()}`;
}

function baseAdapterTurnEvent(context: ActiveTurnContext, type: Extract<InternalEvent, { source: "agent_adapter" }>["type"]): Omit<Extract<InternalEvent, { source: "agent_adapter" }>, "payload"> {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: context.turnId,
    source: "agent_adapter",
    client_type: "claude",
    type,
    time: new Date().toISOString(),
    seq: null,
  };
}

export function buildSessionReadyEvent(context: SessionContext): InternalEvent {
  const payload: Record<string, unknown> = { runtime_instance_id: context.runtimeInstanceId };
  if (context.clientSessionKey) payload.client_session_key = context.clientSessionKey;
  if (context.transcriptPath) payload.transcript_path = context.transcriptPath;
  if (context.clientCwd) payload.client_cwd = context.clientCwd;
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: null,
    source: "agent_client",
    client_type: "claude",
    type: "session.ready",
    time: new Date().toISOString(),
    seq: null,
    payload,
  };
}

export function buildSessionExitedEvent(context: SessionContext, reason: string): InternalEvent {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: null,
    source: "agent_client",
    client_type: "claude",
    type: "session.exited",
    time: new Date().toISOString(),
    seq: null,
    payload: { reason, runtime_instance_id: context.runtimeInstanceId },
  };
}

export function buildTurnStartedEvent(context: ActiveTurnContext): InternalEvent {
  return {
    ...baseAdapterTurnEvent(context, "turn.started"),
    payload: {
      runtime_instance_id: context.runtimeInstanceId,
      input: context.input ? { summary: context.input } : {},
    },
  };
}

export function buildTurnOutputEvent(context: ActiveTurnContext, output: string): InternalEvent {
  return { ...baseAdapterTurnEvent(context, "turn.output"), payload: { output: { summary: output } } };
}

export function buildTurnCompletedEvent(context: ActiveTurnContext): InternalEvent {
  return { ...baseAdapterTurnEvent(context, "turn.completed"), payload: {} };
}

export function buildTurnFailedEvent(context: ActiveTurnContext, message: string): InternalEvent {
  return { ...baseAdapterTurnEvent(context, "turn.failed"), payload: { failure: { message } } };
}
