export type ScrollContainer = Pick<HTMLElement, 'scrollHeight' | 'scrollTop'>;

export function scrollToBottom(element: ScrollContainer | null): void {
  if (!element) return;
  element.scrollTop = element.scrollHeight;
}

export function scrollDocumentToBottom(): void {
  window.scrollTo({ top: document.documentElement.scrollHeight });
}
