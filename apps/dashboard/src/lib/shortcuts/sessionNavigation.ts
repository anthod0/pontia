import type { SessionView } from '../../api/types';
import { visibleChatSessions } from '../session-chat/sessionChat';

export function activeChatSessions(sessions: SessionView[]): SessionView[] {
  return visibleChatSessions(sessions, 'active');
}

export function sessionIdAtShortcutIndex(sessions: SessionView[], key: string): string | null {
  if (!/^[1-9]$/.test(key)) return null;
  return activeChatSessions(sessions)[Number(key) - 1]?.session_id ?? null;
}

export function adjacentActiveSessionId(sessions: SessionView[], currentSessionId: string | null, direction: 1 | -1): string | null {
  const activeSessions = activeChatSessions(sessions);
  if (!activeSessions.length) return null;
  const currentIndex = currentSessionId ? activeSessions.findIndex((session) => session.session_id === currentSessionId) : -1;
  if (currentIndex === -1) return direction === 1 ? activeSessions[0].session_id : activeSessions[activeSessions.length - 1].session_id;
  const nextIndex = (currentIndex + direction + activeSessions.length) % activeSessions.length;
  return activeSessions[nextIndex]?.session_id ?? null;
}

export function sessionIdFromChatPath(pathname: string): string | null {
  const normalized = pathname.replace(/^\/dashboard/, '');
  const match = normalized.match(/^\/chat\/([^/?#]+)/);
  return match ? decodeURIComponent(match[1]) : null;
}

export function isChatRoute(pathname: string): boolean {
  const normalized = pathname.replace(/^\/dashboard/, '');
  return normalized === '/chat' || normalized.startsWith('/chat/');
}
