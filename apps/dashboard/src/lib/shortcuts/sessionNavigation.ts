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
  return adjacentValue(
    sessions.map((session) => session.session_id),
    currentSessionId,
    direction,
  );
}

// Wraps at both ends; an unknown current value starts from the first or last entry.
export function adjacentValue(
  values: string[],
  current: string | null,
  direction: 1 | -1,
): string | null {
  if (!values.length) return null;
  const currentIndex = current === null ? -1 : values.indexOf(current);
  if (currentIndex === -1) return direction === 1 ? values[0] : values[values.length - 1];
  return values[(currentIndex + direction + values.length) % values.length];
}

export function sessionIdFromChatPath(relativePath: string): string | null {
  const match = relativePath.match(/^\/chat\/([^/?#]+)/);
  return match ? decodeURIComponent(match[1]) : null;
}

export function isChatRoute(relativePath: string): boolean {
  return relativePath === "/" || relativePath.startsWith("/chat/");
}
