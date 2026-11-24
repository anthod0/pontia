export type ScrollContainer = Pick<HTMLElement, "scrollHeight" | "scrollTop">;

export function scrollToBottom(element: ScrollContainer | null): void {
  if (!element) return;
  element.scrollTop = element.scrollHeight;
}

export function scrollDocumentToBottom(): void {
  const mobileBody = document.querySelector<HTMLElement>(".mobile-body");
  if (mobileBody) {
    scrollToBottom(mobileBody);
    return;
  }
  window.scrollTo({ top: document.documentElement.scrollHeight });
}
