import type { SessionView } from "../../api/types";

// Sessions arrive in the order the sidebar shows them.
export function sessionIdAtShortcutIndex(sessions: SessionView[], key: string): string | null {
  if (!/^[1-9]$/.test(key)) return null;
  return sessions[Number(key) - 1]?.session_id ?? null;
}

export function adjacentSessionId(
  sessions: SessionView[],
  currentSessionId: string | null,
  direction: 1 | -1,
): string | null {
  if (!sessions.length) return null;
  const currentIndex = currentSessionId
    ? sessions.findIndex((session) => session.session_id === currentSessionId)
    : -1;
  if (currentIndex === -1)
    return direction === 1 ? sessions[0].session_id : sessions[sessions.length - 1].session_id;
  return sessions[(currentIndex + direction + sessions.length) % sessions.length].session_id;
}

export function sessionIdFromChatPath(relativePath: string): string | null {
  const match = relativePath.match(/^\/chat\/([^/?#]+)/);
  return match ? decodeURIComponent(match[1]) : null;
}

export function isChatRoute(relativePath: string): boolean {
  return relativePath === "/" || relativePath.startsWith("/chat/");
}
