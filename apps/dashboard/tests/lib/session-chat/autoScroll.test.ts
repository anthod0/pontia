import { expect, test, vi } from 'vitest';
import { chatAutoScrollKey, scrollDocumentToBottom, scrollToBottom } from '../../../src/lib/session-chat/autoScroll';

const message = (overrides = {}) => ({
  id: 'turn-1:assistant',
  turnId: 'turn-1',
  role: 'assistant',
  content: 'Waiting…',
  status: 'pending',
  createdAt: '2026-01-01T00:00:00Z',
  ...overrides,
});

test('chat auto-scroll key changes when a message is appended or the latest agent output changes', () => {
  const pending = [message()];
  const completed = [message({ content: 'Done', status: 'sent' })];
  const withUserReply = [...completed, message({ id: 'turn-2:user', turnId: 'turn-2', role: 'user', content: 'next', status: 'sent' })];

  expect(chatAutoScrollKey(pending)).not.toBe(chatAutoScrollKey(completed));
  expect(chatAutoScrollKey(completed)).not.toBe(chatAutoScrollKey(withUserReply));
});

test('chat auto-scroll key does not change when older history is prepended', () => {
  const latestMessages = [message({ id: 'turn-2:assistant', turnId: 'turn-2', content: 'Latest', status: 'sent' })];
  const withOlderHistory = [message({ id: 'turn-1:user', turnId: 'turn-1', role: 'user', content: 'Earlier', status: 'sent' }), ...latestMessages];

  expect(chatAutoScrollKey(withOlderHistory)).toBe(chatAutoScrollKey(latestMessages));
});

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
