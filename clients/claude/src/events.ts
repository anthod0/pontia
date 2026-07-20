import type { SessionContext, TurnContext } from "./context.js";

const MAX_TURN_OUTPUT_CHARS = 200;

export type InternalEventType = "session.ready" | "session.exited" | "turn.started" | "turn.output" | "turn.completed" | "turn.failed";

export interface InternalEvent {
  session_id: string;
  turn_id?: string;
  type: InternalEventType;
  data: Record<string, unknown>;
}

type ActiveTurnContext = TurnContext & { turnId: string };

function turnFact(context: ActiveTurnContext, type: InternalEventType, data: Record<string, unknown>): InternalEvent {
  return {
    session_id: context.sessionId,
    turn_id: context.turnId,
    type,
    data,
  };
}

export function buildSessionReadyEvent(context: SessionContext): InternalEvent {
  const data: Record<string, unknown> = { runtime_instance_id: context.runtimeInstanceId };
  if (context.clientSessionKey) data.client_session_key = context.clientSessionKey;
  if (context.transcriptPath) data.transcript_path = context.transcriptPath;
  if (context.clientCwd) data.client_cwd = context.clientCwd;
  return {
    session_id: context.sessionId,
    type: "session.ready",
    data,
  };
}

export function buildSessionExitedEvent(context: SessionContext, reason: string): InternalEvent {
  return {
    session_id: context.sessionId,
    type: "session.exited",
    data: { reason, runtime_instance_id: context.runtimeInstanceId },
  };
}

export function buildTurnStartedEvent(context: TurnContext): InternalEvent {
  return {
    session_id: context.sessionId,
    type: "turn.started",
    data: {
      runtime_instance_id: context.runtimeInstanceId,
      input: context.input ? { summary: context.input } : {},
    },
  };
}

export function buildTurnOutputEvent(context: ActiveTurnContext, output: string): InternalEvent {
  return turnFact(context, "turn.output", {
    output: { summary: Array.from(output).slice(0, MAX_TURN_OUTPUT_CHARS).join("") },
  });
}

export function buildTurnCompletedEvent(context: ActiveTurnContext): InternalEvent {
  return turnFact(context, "turn.completed", {});
}

export function buildTurnFailedEvent(context: ActiveTurnContext, message: string): InternalEvent {
  return turnFact(context, "turn.failed", { failure: { message } });
}
