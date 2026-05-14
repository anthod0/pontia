import type { SessionView } from '../../api/types';

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
