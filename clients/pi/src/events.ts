import { randomUUID } from "node:crypto";
import type { TurnContext } from "./context.js";
import type { SessionContext } from "./session.js";

export type InternalEventType = "session.ready" | "turn.output" | "turn.completed" | "turn.failed";

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
      turn_id: string;
      source: "agent_adapter";
      type: "turn.output" | "turn.completed" | "turn.failed";
    });

type TurnInternalEvent = Extract<InternalEvent, { source: "agent_adapter" }>;

function baseTurnEvent(context: TurnContext, type: TurnInternalEvent["type"]): Omit<TurnInternalEvent, "payload"> {
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

export function buildTurnOutputEvent(context: TurnContext, output: string): InternalEvent {
  return {
    ...baseTurnEvent(context, "turn.output"),
    payload: { output: { summary: output } },
  };
}

export function buildTurnCompletedEvent(context: TurnContext): InternalEvent {
  return {
    ...baseTurnEvent(context, "turn.completed"),
    payload: {},
  };
}

export function buildTurnFailedEvent(context: TurnContext, message: string): InternalEvent {
  return {
    ...baseTurnEvent(context, "turn.failed"),
    payload: { failure: { message } },
  };
}

export function buildSessionReadyEvent(context: SessionContext): InternalEvent {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: null,
    source: "agent_client",
    client_type: "pi",
    type: "session.ready",
    time: new Date().toISOString(),
    seq: null,
    payload: { runtime_instance_id: context.runtimeInstanceId },
  };
}
