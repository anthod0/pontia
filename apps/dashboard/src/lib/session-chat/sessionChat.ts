import { translate as defaultTranslate, type Translator } from "$lib/i18n";
import type { ManagedToolUse, SessionView, TimelineItem, TurnView } from "../../api/types";
import { untitledSessionLabel } from "../sessionTitle";
import { parseUploadMessage } from "../uploads/message";

export type ChatSessionFilter = "active" | "all";
export type ChatMessageRole = "user" | "assistant";
export type ChatMessageStatus = "sent" | "pending" | "failed";

export interface SessionChatToolResult {
  truncated?: boolean;
  content: string;
  status: string | null;
  occurredAt: string | null;
}

export type SessionChatExecutionStatus =
  | "generated"
  | "running"
  | "ended"
  | "succeeded"
  | "failed"
  | "rejected"
  | "interrupted";

export interface SessionChatThoughtStep {
  id: string;
  kind: "thinking" | "tool_call" | "tool_result";
  title: string;
  status: string | null;
  content: string;
  occurredAt: string | null;
  callId?: string;
  result?: SessionChatToolResult;
  managedToolUse?: ManagedToolUse;
  executionStatus?: SessionChatExecutionStatus;
  processOutput?: { text: string; truncated: boolean };
  truncated?: boolean;
}

export interface SessionChatMessage {
  id: string;
  turnId: string;
  role: ChatMessageRole;
  content: string;
  status: ChatMessageStatus;
  createdAt: string;
  thoughtSteps?: SessionChatThoughtStep[];
}

const terminalStates = new Set(["exited", "error"]);
const activePendingTurnStates = new Set(["queued", "running"]);

export function isTerminalChatSession(session: Pick<SessionView, "state">): boolean {
  return terminalStates.has(session.state);
}

export function sessionChatTitle(
  session: Pick<SessionView, "client_type" | "title" | "handle" | "role" | "description">,
  translate: Translator = defaultTranslate,
): string {
  const title = session.title?.trim();
  const handle = session.handle?.trim();
  const role = session.role?.trim();
  const description = session.description?.trim();
  if (title) return title;
  if (handle && role) return `${handle} · ${role}`;
  if (handle) return handle;
  if (description) return description;
  if (role) return role;
  return untitledSessionLabel(session.client_type, translate);
}

export function userPromptForDisplay(prompt: string): string {
  const skillBlock = prompt.match(
    /^<skill name="([^"]+)" location="[^"]+">\r?\n[\s\S]*?\r?\n<\/skill>(?:\r?\n\r?\n([\s\S]*))?$/,
  );
  if (!skillBlock) return prompt;
  return skillBlock[2]?.trim() || `/skill:${skillBlock[1]}`;
}

export function titleFromInitialPrompt(prompt: string, maxLength = 60): string | null {
  const { text, files, secrets } = parseUploadMessage(userPromptForDisplay(prompt));
  const normalized = (
    text.trim() ||
    [
      ...files.map((file) => file.name),
      ...secrets.flatMap((secret) => (secret.label ? [secret.label] : [])),
    ].join(", ") ||
    (secrets.length ? defaultTranslate("secrets.secret") : "")
  )
    .replace(/^```[\w-]*\s*/i, "")
    .replace(/```$/i, "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length > 0)
    ?.replace(/\s+/g, " ")
    .trim();
  if (!normalized) return null;
  return normalized.length > maxLength
    ? `${normalized.slice(0, maxLength - 1).trimEnd()}…`
    : normalized;
}

export function turnsToChatMessages(
  turns: TurnView[],
  translate: Translator = defaultTranslate,
): SessionChatMessage[] {
  return turns
    .slice()
    .sort((a, b) => turnTimestamp(a).localeCompare(turnTimestamp(b)))
    .flatMap((turn) => {
      const input =
        userPromptForDisplay(textFromUnknown(turn.input?.summary ?? turn.input)) ||
        translate("chat.noInputSummary");
      return [
        {
          id: `${turn.turn_id}:user`,
          turnId: turn.turn_id,
          role: "user",
          content: input,
          status: "sent",
          createdAt: turn.created_at,
        },
        assistantMessageForTurn(turn, translate),
      ];
    });
}

/**
 * Projects native timeline items into chat messages in native order. Within a Turn,
 * each assistant message is one segment: the activity steps that preceded a piece of
 * assistant text, followed by that text. User input consumed mid-Turn splits segments.
 */
export function timelineItemsToChatMessages(
  items: TimelineItem[],
  preserveTurnOrder = false,
  translate: Translator = defaultTranslate,
): SessionChatMessage[] {
  const messages: SessionChatMessage[] = [];
  let segmentTurnId: string | null = null;
  let pendingSteps: SessionChatThoughtStep[] = [];

  const flushSteps = () => {
    if (!pendingSteps.length) return;
    messages.push({
      id: `${pendingSteps[0].id}:working`,
      turnId: segmentTurnId ?? pendingSteps[0].id,
      role: "assistant",
      content: "",
      status: pendingSteps.some((step) => step.status === "error") ? "failed" : "pending",
      createdAt: pendingSteps[0].occurredAt ?? "",
      thoughtSteps: pendingSteps,
    });
    pendingSteps = [];
  };

  const orderedItems = preserveTurnOrder
    ? items
    : items.slice().sort((a, b) => timelineTimestamp(a).localeCompare(timelineTimestamp(b)));
  for (const item of orderedItems) {
    if (item.kind === "context_compaction") {
      flushSteps();
      continue;
    }
    if (segmentTurnId && item.turn_id && item.turn_id !== segmentTurnId) flushSteps();

    if (item.kind === "user") {
      flushSteps();
      const content = userPromptForDisplay(item.content_preview ?? "");
      messages.push({
        id: item.item_id,
        turnId: item.turn_id ?? item.item_id,
        role: item.kind,
        content: content.trim() ? content : "No input was reported.",
        status: item.status === "error" ? "failed" : "sent",
        createdAt: item.occurred_at ?? "",
      });
      segmentTurnId = null;
      continue;
    }

    if (item.kind !== "assistant" && !isThoughtStepKind(item.kind)) continue;
    if (!pendingSteps.length) segmentTurnId = item.turn_id ?? segmentTurnId ?? item.item_id;

    if (item.kind === "assistant") {
      messages.push({
        id: item.item_id,
        turnId: segmentTurnId ?? item.item_id,
        role: "assistant",
        content: item.content_preview?.trim() || translate("chat.noAssistantOutput"),
        status: item.status === "error" ? "failed" : "sent",
        createdAt: item.occurred_at ?? "",
        ...(pendingSteps.length ? { thoughtSteps: pendingSteps } : {}),
      });
      pendingSteps = [];
      continue;
    }

    const content = item.content_preview?.trim() || translate("chat.noDetails");
    if (item.kind === "tool_result" && item.call_id) {
      const toolCall = pendingSteps.findLast(
        (step) => step.kind === "tool_call" && step.callId === item.call_id && !step.result,
      );
      if (toolCall) {
        toolCall.result = {
          content,
          status: item.status,
          occurredAt: item.occurred_at,
        };
        continue;
      }
    }

    const managedToolUse =
      item.kind === "tool_call" ? (item.managed_tool_use ?? undefined) : undefined;
    pendingSteps.push({
      id: item.item_id,
      kind: item.kind,
      title: managedToolUse
        ? managedToolUseTitle(managedToolUse, translate)
        : item.title?.trim() || defaultThoughtStepTitle(item.kind, translate),
      status: item.status ?? (item.kind === "tool_call" ? "started" : null),
      content: managedToolUse ? managedToolUseContent(managedToolUse) : content,
      occurredAt: item.occurred_at,
      ...(item.call_id ? { callId: item.call_id } : {}),
      ...(managedToolUse ? { managedToolUse } : {}),
    });
  }

  flushSteps();

  return messages;
}

export function canSendSessionMessage(
  session: Pick<SessionView, "state" | "capabilities"> | null,
  input: string,
): boolean {
  return Boolean(
    session &&
    session.state !== "error" &&
    session.capabilities?.accept_task === true &&
    input.trim(),
  );
}

function isThoughtStepKind(kind: string): kind is SessionChatThoughtStep["kind"] {
  return kind === "thinking" || kind === "tool_call" || kind === "tool_result";
}

function defaultThoughtStepTitle(
  kind: SessionChatThoughtStep["kind"],
  translate: Translator,
): string {
  if (kind === "thinking") return translate("chat.thinking");
  if (kind === "tool_call") return translate("chat.toolCall");
  return translate("chat.toolResult");
}

export function managedToolUseTitle(
  toolUse: ManagedToolUse,
  translate: Translator = defaultTranslate,
): string {
  switch (toolUse.input.type) {
    case "read":
      return translate("chat.readFile");
    case "edit":
      return translate("chat.editFile");
    case "write":
      return translate("chat.writeFile");
    case "bash":
      return translate("chat.runCommand");
    case "file_changes":
      return translate("chat.changeFiles");
    case "plan":
      return translate("chat.plan");
  }
}

export function managedToolUseContent(toolUse: ManagedToolUse): string {
  const input = toolUse.input;
  switch (input.type) {
    case "read":
      return formatReadTarget(input.path, input.start_line, input.end_line);
    case "edit":
      return `${input.path}${input.edits_count ? ` · ${input.edits_count} edit${input.edits_count === 1 ? "" : "s"}` : ""}`;
    case "write":
      return input.path;
    case "bash":
      return input.command;
    case "file_changes":
      return input.changes.map((change) => change.path).join("\n");
    case "plan":
      return input.entries.map((entry) => entry.content).join("\n");
  }
}

function formatReadTarget(
  path: string,
  startLine?: number | null,
  endLine?: number | null,
): string {
  if (startLine !== undefined && startLine !== null && endLine !== undefined && endLine !== null)
    return `${path}:${startLine}-${endLine}`;
  if (startLine !== undefined && startLine !== null) return `${path}:${startLine}`;
  if (endLine !== undefined && endLine !== null) return `${path}:1-${endLine}`;
  return path;
}

function timelineTimestamp(item: TimelineItem): string {
  return item.occurred_at ?? item.item_id;
}

function assistantMessageForTurn(turn: TurnView, translate: Translator): SessionChatMessage {
  const output = textFromUnknown(turn.output?.summary);
  if (output) {
    return {
      id: `${turn.turn_id}:assistant`,
      turnId: turn.turn_id,
      role: "assistant",
      content: output,
      status: "sent",
      createdAt: turn.completed_at ?? turn.created_at,
    };
  }

  if (turn.failure) {
    return {
      id: `${turn.turn_id}:assistant`,
      turnId: turn.turn_id,
      role: "assistant",
      content: textFromUnknown(turn.failure) || translate("chat.failedWithoutReason"),
      status: "failed",
      createdAt: turn.completed_at ?? turn.created_at,
    };
  }

  return {
    id: `${turn.turn_id}:assistant`,
    turnId: turn.turn_id,
    role: "assistant",
    content: activePendingTurnStates.has(turn.state) ? "" : translate("chat.noTurnOutput"),
    status: activePendingTurnStates.has(turn.state) ? "pending" : "sent",
    createdAt: turn.completed_at ?? turn.created_at,
  };
}

function textFromUnknown(value: unknown): string {
  if (typeof value === "string") return value.trim();
  if (value === null || value === undefined) return "";
  if (typeof value === "object") {
    const maybeMessage =
      (value as { message?: unknown; summary?: unknown }).message ??
      (value as { summary?: unknown }).summary;
    if (typeof maybeMessage === "string" && maybeMessage.trim()) return maybeMessage.trim();
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function turnTimestamp(turn: TurnView): string {
  return turn.created_at || turn.started_at || turn.completed_at || "";
}
