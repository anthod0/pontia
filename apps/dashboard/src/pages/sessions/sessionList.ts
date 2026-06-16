import type { SessionView } from '../../api/types';
import { untitledSessionLabel } from '../../lib/sessionTitle';

export type SessionFilter = 'active' | 'exited' | 'all';

const terminalStates = new Set(['exited', 'error']);

export function isTerminalSession(session: Pick<SessionView, 'state'>): boolean {
  return terminalStates.has(session.state);
}

export function sortSessionsForConsole<T extends Pick<SessionView, 'state' | 'updated_at'>>(sessions: T[]): T[] {
  return sessions.slice().sort((a, b) => {
    const aTerminal = terminalStates.has(a.state);
    const bTerminal = terminalStates.has(b.state);
    if (aTerminal !== bTerminal) return aTerminal ? 1 : -1;
    return b.updated_at.localeCompare(a.updated_at);
  });
}

export function sessionDisplayTitle(session: Pick<SessionView, 'session_id' | 'client_type' | 'title' | 'handle' | 'role'>): string {
  const title = session.title?.trim();
  const handle = session.handle?.trim();
  const role = session.role?.trim();
  const short = shortSessionId(session.session_id);
  if (title) return title;
  if (handle && role) return `${handle} · ${role}`;
  if (handle) return handle;
  if (role) return `${role} · ${short}`;
  return untitledSessionLabel(session.client_type);
}

function shortSessionId(sessionId: string): string {
  return sessionId.split('_').at(-1)?.slice(0, 10) || sessionId;
}

export function visibleSessionsForFilter<T extends Pick<SessionView, 'state' | 'updated_at'>>(
  sessions: T[],
  filter: SessionFilter,
): T[] {
  const filtered = sessions.filter((session) => {
    const terminal = terminalStates.has(session.state);
    if (filter === 'active') return !terminal;
    if (filter === 'exited') return terminal;
    return true;
  });

  return sortSessionsForConsole(filtered);
}
