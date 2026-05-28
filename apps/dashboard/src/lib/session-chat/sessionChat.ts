import type { SessionView, TurnView } from '../../api/types';

export type ChatSessionFilter = 'active' | 'all';
export type ChatMessageRole = 'user' | 'assistant';
export type ChatMessageStatus = 'sent' | 'pending' | 'failed';

export interface SessionChatMessage {
  id: string;
  turnId: string;
  role: ChatMessageRole;
  content: string;
  status: ChatMessageStatus;
  createdAt: string;
}

const terminalStates = new Set(['exited', 'error']);
const activePendingTurnStates = new Set(['queued', 'running']);

export function isTerminalChatSession(session: Pick<SessionView, 'state'>): boolean {
  return terminalStates.has(session.state);
}

export function sessionChatTitle(session: Pick<SessionView, 'session_id' | 'handle' | 'role' | 'description'>): string {
  const handle = session.handle?.trim();
  const role = session.role?.trim();
  const description = session.description?.trim();
  if (handle && role) return `${handle} · ${role}`;
  if (handle) return handle;
  if (description) return description;
  if (role) return role;
  return shortId(session.session_id);
}

export function visibleChatSessions<T extends Pick<SessionView, 'state' | 'updated_at'>>(
  sessions: T[],
  filter: ChatSessionFilter,
): T[] {
  return sessions
    .filter((session) => filter === 'all' || !terminalStates.has(session.state))
    .slice()
    .sort((a, b) => {
      const aTerminal = terminalStates.has(a.state);
      const bTerminal = terminalStates.has(b.state);
      if (aTerminal !== bTerminal) return aTerminal ? 1 : -1;
      return b.updated_at.localeCompare(a.updated_at);
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

export function canSendSessionMessage(session: Pick<SessionView, 'state'> | null, input: string): boolean {
  return Boolean(session && !terminalStates.has(session.state) && input.trim());
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
    content: activePendingTurnStates.has(turn.state) ? 'Waiting for the assistant response…' : 'No assistant output was reported for this turn.',
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
