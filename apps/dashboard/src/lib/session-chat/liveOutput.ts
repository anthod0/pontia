import type { ManagedToolUse, TurnView } from "../../api/types";
import {
  managedToolUseContent,
  managedToolUseTitle,
  userPromptForDisplay,
  type SessionChatMessage,
  type SessionChatThoughtStep,
} from "./sessionChat";

export interface LiveToolStatus {
  phase: "running" | "ended";
  outcome?: "succeeded" | "failed" | "rejected" | "interrupted";
}
export interface LiveToolOutput {
  text: string;
  truncated: boolean;
}
export interface LiveToolResult {
  content?: unknown;
  error?: unknown;
  truncated: boolean;
}

export type LiveOutputItem =
  | { kind: "assistant_text"; item_id: string; text: string }
  | {
      kind: "tool_call";
      item_id: string;
      call_id: string;
      tool_name: string;
      arguments: unknown;
      managed_tool_use?: ManagedToolUse;
      status?: LiveToolStatus;
      output?: LiveToolOutput;
      result?: LiveToolResult;
    };

export type LiveOutputUpdate =
  | { type: "tool_status_updated"; call_id: string; status: LiveToolStatus }
  | { type: "tool_result"; call_id: string; result: LiveToolResult }
  | { type: "tool_output_delta"; call_id: string; delta: string; truncated: boolean }
  | { type: "tool_output_updated"; call_id: string; text: string; truncated: boolean }
  | { type: "assistant_text_delta"; item_id: string; delta: string }
  | {
      type: "tool_call";
      item_id: string;
      call_id: string;
      tool_name: string;
      arguments: unknown;
      managed_tool_use?: ManagedToolUse;
    };

interface LiveOutputIdentity {
  session_id: string;
  turn_id: string;
  stream_id: string;
}

export type LiveOutputEvent =
  | ({ type: "snapshot"; sequence: number; items: LiveOutputItem[] } & LiveOutputIdentity)
  | ({ type: "updates"; first_sequence: number; updates: LiveOutputUpdate[] } & LiveOutputIdentity)
  | ({
      type: "closed";
      sequence: number;
      reason: "producer_closed" | "invalidated" | "expired";
    } & LiveOutputIdentity);

export interface LiveOutputOverlay extends LiveOutputIdentity {
  sequence: number;
  items: LiveOutputItem[];
  awaitingSnapshot: boolean;
  closed: boolean;
}

export type LiveOutputOverlays = Record<string, LiveOutputOverlay>;

export function applyLiveOutputEvent(
  overlays: LiveOutputOverlays,
  expectedSessionId: string,
  event: LiveOutputEvent,
): LiveOutputOverlays {
  if (event.session_id !== expectedSessionId) return overlays;

  if (event.type === "snapshot") {
    const current = overlays[event.turn_id];
    if (current?.stream_id === event.stream_id && event.sequence < current.sequence)
      return overlays;
    return {
      ...overlays,
      [event.turn_id]: {
        session_id: event.session_id,
        turn_id: event.turn_id,
        stream_id: event.stream_id,
        sequence: event.sequence,
        items: event.items.map(cloneItem),
        awaitingSnapshot: false,
        closed: false,
      },
    };
  }

  const current = overlays[event.turn_id];
  if (!current || current.stream_id !== event.stream_id) return overlays;
  if (event.type === "closed") {
    if (event.sequence < current.sequence) return overlays;
    if (event.reason === "expired") return removeLiveOutputOverlay(overlays, event.turn_id);
    return { ...overlays, [event.turn_id]: { ...current, closed: true } };
  }
  if (current.awaitingSnapshot || event.first_sequence !== current.sequence + 1) {
    return { ...overlays, [event.turn_id]: { ...current, awaitingSnapshot: true } };
  }

  const items = current.items.map(cloneItem);
  for (const update of event.updates) {
    if (!applyUpdate(items, update))
      return { ...overlays, [event.turn_id]: { ...current, awaitingSnapshot: true } };
  }
  return {
    ...overlays,
    [event.turn_id]: {
      ...current,
      sequence: event.first_sequence + event.updates.length - 1,
      items,
    },
  };
}

export function markLiveOutputDisconnected(overlays: LiveOutputOverlays): LiveOutputOverlays {
  return Object.fromEntries(
    Object.entries(overlays).map(([turnId, overlay]) => [
      turnId,
      { ...overlay, awaitingSnapshot: true },
    ]),
  );
}

export function removeLiveOutputOverlay(
  overlays: LiveOutputOverlays,
  turnId: string,
): LiveOutputOverlays {
  if (!overlays[turnId]) return overlays;
  const next = { ...overlays };
  delete next[turnId];
  return next;
}

export function mergeLiveOutputMessages(
  transcriptMessages: SessionChatMessage[],
  turns: TurnView[],
  overlays: LiveOutputOverlays,
  activeTurnId: string | null = null,
): SessionChatMessage[] {
  const turnsById = new Map(turns.map((turn) => [turn.turn_id, turn]));
  let messages = transcriptMessages.slice();

  for (const overlay of Object.values(overlays)) {
    const turn = turnsById.get(overlay.turn_id);
    if (!turn) continue;

    const transcriptForTurn = messages.filter((message) => message.turnId === overlay.turn_id);
    const activeOnCurrentBranch =
      overlay.turn_id === activeTurnId && (turn.state === "queued" || turn.state === "running");
    if (
      (!activeOnCurrentBranch && !transcriptForTurn.length) ||
      (overlay.awaitingSnapshot && !overlay.closed)
    )
      continue;
    const userMessages = transcriptForTurn.filter((message) => message.role === "user");
    messages = messages.filter(
      (message) => message.turnId !== overlay.turn_id || message.role === "user",
    );
    if (!userMessages.length) {
      const input = userPromptForDisplay(turn.input?.summary ?? "").trim();
      if (input) {
        const insertionIndex = messages.findIndex((message) => message.createdAt > turn.created_at);
        const userMessage: SessionChatMessage = {
          id: `${turn.turn_id}:user`,
          turnId: turn.turn_id,
          role: "user",
          content: input,
          status: "sent",
          createdAt: turn.created_at,
        };
        if (insertionIndex < 0) messages.push(userMessage);
        else messages.splice(insertionIndex, 0, userMessage);
      }
    }

    const liveMessages = liveItemsToMessages(overlay);
    const lastTurnMessage = messages.reduce(
      (last, message, index) => (message.turnId === overlay.turn_id ? index : last),
      -1,
    );
    messages.splice(lastTurnMessage + 1, 0, ...liveMessages);
  }

  return messages;
}

function liveItemsToMessages(overlay: LiveOutputOverlay): SessionChatMessage[] {
  if (!overlay.items.length) return [];

  const content = overlay.items
    .filter(
      (item): item is Extract<LiveOutputItem, { kind: "assistant_text" }> =>
        item.kind === "assistant_text",
    )
    .map((item) => item.text)
    .join("\n\n");
  const thoughtSteps = overlay.items
    .filter(
      (item): item is Extract<LiveOutputItem, { kind: "tool_call" }> => item.kind === "tool_call",
    )
    .map((item): SessionChatThoughtStep => ({
      id: `live:${overlay.stream_id}:${item.item_id}`,
      kind: "tool_call",
      title: item.managed_tool_use ? managedToolUseTitle(item.managed_tool_use) : item.tool_name,
      callId: item.call_id,
      status: item.status?.outcome ?? item.status?.phase ?? "generated",
      executionStatus: item.status?.outcome ?? item.status?.phase ?? "generated",
      ...(item.output ? { processOutput: { ...item.output } } : {}),
      ...(item.result
        ? {
            result: {
              content: formatResult(item.result),
              status: "error" in item.result || item.status?.outcome === "failed" ? "error" : null,
              occurredAt: null,
              truncated: item.result.truncated,
            },
          }
        : {}),
      content: item.managed_tool_use
        ? managedToolUseContent(item.managed_tool_use)
        : formatArguments(item.arguments),
      occurredAt: null,
      ...(item.managed_tool_use ? { managedToolUse: item.managed_tool_use } : {}),
    }));

  return [
    {
      id: `live:${overlay.stream_id}:assistant`,
      turnId: overlay.turn_id,
      role: "assistant",
      content,
      status: "pending",
      createdAt: "",
      ...(thoughtSteps.length ? { thoughtSteps } : {}),
    },
  ];
}

function applyUpdate(items: LiveOutputItem[], update: LiveOutputUpdate): boolean {
  if (update.type === "assistant_text_delta") {
    const existing = items.find((item) => item.item_id === update.item_id);
    if (existing?.kind === "assistant_text") {
      existing.text += update.delta;
    } else if (!existing) {
      items.push({ kind: "assistant_text", item_id: update.item_id, text: update.delta });
    }
    return !existing || existing.kind === "assistant_text";
  }
  if (update.type === "tool_call") {
    if (
      items.some(
        (item) =>
          item.item_id === update.item_id ||
          (item.kind === "tool_call" && item.call_id === update.call_id),
      )
    )
      return false;
    const { type: _type, ...call } = update;
    items.push({ kind: "tool_call", ...call });
    return true;
  }
  const item = items.find((item) => item.kind === "tool_call" && item.call_id === update.call_id);
  if (item?.kind !== "tool_call") return false;
  switch (update.type) {
    case "tool_result":
      item.result = cloneJson(update.result);
      break;
    case "tool_status_updated":
      if (
        !(
          item.status?.phase === "ended" &&
          (update.status.phase === "running" || item.status.outcome)
        )
      )
        item.status = { ...update.status };
      break;
    case "tool_output_updated":
      item.output = { text: update.text, truncated: update.truncated };
      break;
    case "tool_output_delta":
      item.output = {
        text: (item.output?.text ?? "") + update.delta,
        truncated: !!item.output?.truncated || update.truncated,
      };
      break;
  }
  return true;
}

function cloneItem(item: LiveOutputItem): LiveOutputItem {
  return item.kind === "assistant_text"
    ? { ...item }
    : {
        ...item,
        arguments: cloneJson(item.arguments),
        ...(item.status ? { status: { ...item.status } } : {}),
        ...(item.output ? { output: { ...item.output } } : {}),
        ...(item.result ? { result: cloneJson(item.result) } : {}),
        ...(item.managed_tool_use ? { managed_tool_use: cloneJson(item.managed_tool_use) } : {}),
      };
}

function cloneJson<T>(value: T): T {
  if (typeof structuredClone === "function") return structuredClone(value);
  return JSON.parse(JSON.stringify(value)) as T;
}

function formatResult(result: LiveToolResult): string {
  return ["content", "error"]
    .filter((key) => key in result)
    .map((key) => {
      const value = result[key as "content" | "error"];
      return typeof value === "string" ? value : formatArguments(value);
    })
    .join("\n\n");
}

function formatArguments(argumentsValue: unknown): string {
  try {
    return JSON.stringify(argumentsValue, null, 2);
  } catch {
    return String(argumentsValue);
  }
}
