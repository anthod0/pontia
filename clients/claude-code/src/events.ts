import { randomUUID } from "node:crypto";
import type { TurnContext } from "./context.js";

export type InternalEventType = "turn.output" | "turn.completed" | "turn.failed";

export interface InternalEvent {
  event_id: string;
  session_id: string;
  turn_id: string;
  source: "agent_adapter";
  client_type: "claude_code";
  type: InternalEventType;
  time: string;
  seq: null;
  payload: Record<string, unknown>;
}

function baseEvent(context: TurnContext, type: InternalEventType): Omit<InternalEvent, "payload"> {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: context.turnId,
    source: "agent_adapter",
    client_type: "claude_code",
    type,
    time: new Date().toISOString(),
    seq: null,
  };
}

export function buildTurnOutputEvent(context: TurnContext, output: string): InternalEvent {
  return { ...baseEvent(context, "turn.output"), payload: { output: { summary: output } } };
}

export function buildTurnCompletedEvent(context: TurnContext): InternalEvent {
  return { ...baseEvent(context, "turn.completed"), payload: {} };
}

export function buildTurnFailedEvent(context: TurnContext, message: string): InternalEvent {
  return { ...baseEvent(context, "turn.failed"), payload: { failure: { message } } };
}
