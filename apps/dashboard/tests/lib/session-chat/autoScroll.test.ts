import { expect, test, vi } from 'vitest';
import { scrollDocumentToBottom, scrollToBottom } from '../../../src/lib/session-chat/autoScroll';


test('scrollToBottom moves the scroll container to its bottom edge', () => {
  const element = { scrollTop: 0, scrollHeight: 640 };

  scrollToBottom(element);

  expect(element.scrollTop).toBe(640);
});

test('scrollDocumentToBottom scrolls the page to the document bottom edge', () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 2048 });

  scrollDocumentToBottom();

  expect(scrollTo).toHaveBeenCalledWith({ top: 2048 });
});
