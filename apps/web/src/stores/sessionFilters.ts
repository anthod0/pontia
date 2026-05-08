import type { SessionView } from '../api/types';

export function visibleSessions(sessions: SessionView[]): SessionView[] {
  return sessions.filter((session) => session.state !== 'exited');
}
