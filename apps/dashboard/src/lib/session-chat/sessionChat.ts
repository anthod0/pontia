import type { ManagedToolUse, SessionView, TimelineItem, TurnView } from '../../api/types';
import { untitledSessionLabel } from '../sessionTitle';

export type ChatSessionFilter = 'active' | 'all';
export type ChatMessageRole = 'user' | 'assistant';
export type ChatMessageStatus = 'sent' | 'pending' | 'failed';

export interface SessionChatThoughtStep {
  id: string;
  kind: 'thinking' | 'tool_call' | 'tool_result';
  title: string;
  status: string | null;
  content: string;
  occurredAt: string | null;
  managedToolUse?: ManagedToolUse;
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

const terminalStates = new Set(['exited', 'error']);
const activePendingTurnStates = new Set(['queued', 'running']);

export function isTerminalChatSession(session: Pick<SessionView, 'state'>): boolean {
  return terminalStates.has(session.state);
}

export function sessionChatTitle(session: Pick<SessionView, 'client_type' | 'title' | 'handle' | 'role' | 'description'>): string {
  const title = session.title?.trim();
  const handle = session.handle?.trim();
  const role = session.role?.trim();
  const description = session.description?.trim();
  if (title) return title;
  if (handle && role) return `${handle} · ${role}`;
  if (handle) return handle;
  if (description) return description;
  if (role) return role;
  return untitledSessionLabel(session.client_type);
}

export function titleFromInitialPrompt(prompt: string, maxLength = 60): string | null {
  const normalized = prompt
    .replace(/^```[\w-]*\s*/i, '')
    .replace(/```$/i, '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length > 0)
    ?.replace(/\s+/g, ' ')
    .trim();
  if (!normalized) return null;
  return normalized.length > maxLength ? `${normalized.slice(0, maxLength - 1).trimEnd()}…` : normalized;
}

export function visibleChatSessions<T extends Pick<SessionView, 'state' | 'created_at' | 'pinned_at' | 'session_id'>>(
  sessions: T[],
  filter: ChatSessionFilter,
): T[] {
  return sessions
    .filter((session) => filter === 'all' || !terminalStates.has(session.state))
    .slice()
    .sort((a, b) => {
      const aPinned = a.pinned_at !== null;
      const bPinned = b.pinned_at !== null;
      if (aPinned !== bPinned) return aPinned ? -1 : 1;
      if (a.pinned_at && b.pinned_at) return b.pinned_at.localeCompare(a.pinned_at);
      const createdComparison = b.created_at.localeCompare(a.created_at);
      if (createdComparison !== 0) return createdComparison;
      return a.session_id.localeCompare(b.session_id);
    });
}

export function turnsToChatMessages(turns: TurnView[]): SessionChatMessage[] {
  return turns
    .slice()
    .sort((a, b) => turnTimestamp(a).localeCompare(turnTimestamp(b)))
    .flatMap((turn) => {
      const input = textFromUnknown(turn.input?.summary ?? turn.input) || 'No input summary was reported.';
      return [
        {
          id: `${turn.turn_id}:user`,
          turnId: turn.turn_id,
          role: 'user',
          content: input,
          status: 'sent',
          createdAt: turn.created_at,
        },
        assistantMessageForTurn(turn),
      ];
    });
}

export function timelineItemsToChatMessages(items: TimelineItem[]): SessionChatMessage[] {
  const messages: SessionChatMessage[] = [];
  let pendingThoughtSteps: SessionChatThoughtStep[] = [];
  let pendingThoughtTurnId: string | null = null;
  let pendingThoughtOccurredAt: string | null = null;

  const flushPendingWorkingMessage = () => {
    if (!pendingThoughtSteps.length) return;
    const turnId = pendingThoughtTurnId ?? pendingThoughtSteps[0]?.id ?? 'pending';
    const lastAssistant = [...messages].reverse().find((message) => message.role === 'assistant' && message.turnId === turnId);
    if (lastAssistant) {
      lastAssistant.thoughtSteps = [...(lastAssistant.thoughtSteps ?? []), ...pendingThoughtSteps];
    } else {
      messages.push({
        id: `${turnId}:working`,
        turnId,
        role: 'assistant',
        content: '',
        status: 'pending',
        createdAt: pendingThoughtOccurredAt ?? '',
        thoughtSteps: pendingThoughtSteps,
      });
    }
    pendingThoughtSteps = [];
    pendingThoughtTurnId = null;
    pendingThoughtOccurredAt = null;
  };

  for (const item of items.slice().sort((a, b) => timelineTimestamp(a).localeCompare(timelineTimestamp(b)))) {
    if (item.kind === 'user') {
      flushPendingWorkingMessage();
      messages.push({
        id: item.item_id,
        turnId: item.turn_id ?? item.item_id,
        role: item.kind,
        content: item.content_preview?.trim() || 'No input was reported.',
        status: item.status === 'error' ? 'failed' : 'sent',
        createdAt: item.occurred_at ?? '',
      });
      continue;
    }

    if (item.kind === 'assistant') {
      const message: SessionChatMessage = {
        id: item.item_id,
        turnId: item.turn_id ?? item.item_id,
        role: item.kind,
        content: item.content_preview?.trim() || 'No assistant output was reported.',
        status: item.status === 'error' ? 'failed' : 'sent',
        createdAt: item.occurred_at ?? '',
      };
      if (pendingThoughtSteps.length) {
        message.thoughtSteps = pendingThoughtSteps;
        pendingThoughtSteps = [];
        pendingThoughtTurnId = null;
        pendingThoughtOccurredAt = null;
      }
      messages.push(message);
      continue;
    }

    if (isThoughtStepKind(item.kind)) {
      pendingThoughtTurnId = item.turn_id ?? pendingThoughtTurnId;
      pendingThoughtOccurredAt ??= item.occurred_at;
      const managedToolUse = item.kind === 'tool_call' ? item.managed_tool_use ?? undefined : undefined;
      pendingThoughtSteps.push({
        id: item.item_id,
        kind: item.kind,
        title: managedToolUse ? managedToolUseTitle(managedToolUse) : item.title?.trim() || defaultThoughtStepTitle(item.kind),
        status: item.status ?? (item.kind === 'tool_call' ? 'started' : null),
        content: managedToolUse ? managedToolUseContent(managedToolUse) : item.content_preview?.trim() || 'No details reported.',
        occurredAt: item.occurred_at,
        ...(managedToolUse ? { managedToolUse } : {}),
      });
    }
  }

  flushPendingWorkingMessage();

  return messages;
}

export function canSendSessionMessage(session: Pick<SessionView, 'state' | 'capabilities'> | null, input: string): boolean {
  return Boolean(session && session.state !== 'error' && session.capabilities?.accept_task === true && input.trim());
}

function isThoughtStepKind(kind: string): kind is SessionChatThoughtStep['kind'] {
  return kind === 'thinking' || kind === 'tool_call' || kind === 'tool_result';
}

function defaultThoughtStepTitle(kind: SessionChatThoughtStep['kind']): string {
  if (kind === 'thinking') return 'Thinking';
  if (kind === 'tool_call') return 'Tool call';
  return 'Tool result';
}

function managedToolUseTitle(toolUse: ManagedToolUse): string {
  switch (toolUse.input.type) {
    case 'read': return 'Read file';
    case 'edit': return 'Edit file';
    case 'write': return 'Write file';
    case 'bash': return 'Run command';
  }
}

function managedToolUseContent(toolUse: ManagedToolUse): string {
  const input = toolUse.input;
  switch (input.type) {
    case 'read': return formatReadTarget(input.path, input.start_line, input.end_line);
    case 'edit': return `${input.path}${input.edits_count ? ` · ${input.edits_count} edit${input.edits_count === 1 ? '' : 's'}` : ''}`;
    case 'write': return input.path;
    case 'bash': return input.command;
  }
}

function formatReadTarget(path: string, startLine?: number | null, endLine?: number | null): string {
  if (startLine !== undefined && startLine !== null && endLine !== undefined && endLine !== null) return `${path}:${startLine}-${endLine}`;
  if (startLine !== undefined && startLine !== null) return `${path}:${startLine}`;
  if (endLine !== undefined && endLine !== null) return `${path}:1-${endLine}`;
  return path;
}

function timelineTimestamp(item: TimelineItem): string {
  return item.occurred_at ?? item.item_id;
}

function assistantMessageForTurn(turn: TurnView): SessionChatMessage {
  const output = textFromUnknown(turn.output?.summary);
  if (output) {
    return {
      id: `${turn.turn_id}:assistant`,
      turnId: turn.turn_id,
      role: 'assistant',
      content: output,
      status: 'sent',
      createdAt: turn.completed_at ?? turn.created_at,
    };
  }

  if (turn.failure) {
    return {
      id: `${turn.turn_id}:assistant`,
      turnId: turn.turn_id,
      role: 'assistant',
      content: textFromUnknown(turn.failure) || 'The turn failed without a reported reason.',
      status: 'failed',
      createdAt: turn.completed_at ?? turn.created_at,
    };
  }

  return {
    id: `${turn.turn_id}:assistant`,
    turnId: turn.turn_id,
    role: 'assistant',
    content: activePendingTurnStates.has(turn.state) ? '' : 'No assistant output was reported for this turn.',
    status: activePendingTurnStates.has(turn.state) ? 'pending' : 'sent',
    createdAt: turn.completed_at ?? turn.created_at,
  };
}

function textFromUnknown(value: unknown): string {
  if (typeof value === 'string') return value.trim();
  if (value === null || value === undefined) return '';
  if (typeof value === 'object') {
    const maybeMessage = (value as { message?: unknown; summary?: unknown }).message ?? (value as { summary?: unknown }).summary;
    if (typeof maybeMessage === 'string' && maybeMessage.trim()) return maybeMessage.trim();
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function turnTimestamp(turn: TurnView): string {
  return turn.created_at || turn.started_at || turn.completed_at || '';
}

function shortId(id: string): string {
  return id.split('_').at(-1)?.slice(0, 10) || id.slice(0, 10);
}
