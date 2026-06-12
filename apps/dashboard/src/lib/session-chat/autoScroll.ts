import type { SessionChatMessage } from './sessionChat';

export type ScrollContainer = Pick<HTMLElement, 'scrollHeight' | 'scrollTop'>;

export function chatAutoScrollKey(messages: SessionChatMessage[]): string {
  const latest = messages.at(-1);
  if (!latest) return '0';
  return [messages.length, latest.id, latest.status, latest.content].join('\u001f');
}

export function scrollToBottom(element: ScrollContainer | null): void {
  if (!element) return;
  element.scrollTop = element.scrollHeight;
}

export function scrollDocumentToBottom(): void {
  window.scrollTo({ top: document.documentElement.scrollHeight });
}
