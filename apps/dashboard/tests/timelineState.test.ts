import { get } from 'svelte/store';
import { beforeEach, expect, test, vi } from 'vitest';
import { ApiError } from '../src/api/errors';
import type { TimelinePage } from '../src/api/types';

const mocks = vi.hoisted(() => ({
  getSessionTimeline: vi.fn(),
}));

vi.mock('../src/api/client', () => ({
  getSessionTimeline: mocks.getSessionTimeline,
}));

const {
  handleTimelineMessageUpdated,
  loadSessionTimeline,
  resetTimelineState,
  timelineState,
} = await import('../src/stores/timeline');

function page(overrides: Partial<TimelinePage> = {}): TimelinePage {
  return {
    session_id: 'sess-1',
    binding_id: 'bind-1',
    items: [
      {
        item_id: 'item-1',
        kind: 'assistant',
        role: 'assistant',
        title: null,
        status: null,
        occurred_at: '2026-06-09T00:00:00Z',
        content_preview: 'hello',
        content_ref: 'ref-1',
        turn_id: null,
      },
    ],
    next_cursor: null,
    tail_cursor: 'cursor-tail-1',
    has_more: false,
    is_tail: true,
    source_id: 'pi:/tmp/session.jsonl',
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getSessionTimeline.mockReset();
  resetTimelineState();
});

test('rebuild stores timeline items and cursor scope as one state unit', async () => {
  mocks.getSessionTimeline.mockResolvedValueOnce(page());

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  expect(mocks.getSessionTimeline).toHaveBeenCalledWith('sess-1', { cursor: null, limit: 50 });
  expect(get(timelineState)).toMatchObject({
    sessionId: 'sess-1',
    bindingId: 'bind-1',
    sourceId: 'pi:/tmp/session.jsonl',
    tailCursor: 'cursor-tail-1',
    items: [expect.objectContaining({ item_id: 'item-1' })],
    loading: false,
    error: null,
  });
});

test('rebuild loads all timeline pages until the source tail', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page({
      items: [{ ...page().items[0], item_id: 'item-1' }],
      next_cursor: 'cursor-next-1',
      tail_cursor: 'cursor-tail-page-1',
      has_more: true,
      is_tail: false,
    }))
    .mockResolvedValueOnce(page({
      items: [{ ...page().items[0], item_id: 'item-2', content_ref: 'ref-2' }],
      next_cursor: null,
      tail_cursor: 'cursor-tail-final',
      has_more: false,
      is_tail: true,
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(1, 'sess-1', { cursor: null, limit: 50 });
  expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(2, 'sess-1', { cursor: 'cursor-next-1', limit: 50 });
  expect(get(timelineState)).toMatchObject({
    items: [expect.objectContaining({ item_id: 'item-1' }), expect.objectContaining({ item_id: 'item-2' })],
    nextCursor: null,
    tailCursor: 'cursor-tail-final',
    hasMore: false,
    isTail: true,
  });
});

test('message update with matching binding appends from tail cursor', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page())
    .mockResolvedValueOnce(page({
      items: [{ ...page().items[0], item_id: 'item-2', content_ref: 'ref-2', content_preview: 'next' }],
      tail_cursor: 'cursor-tail-2',
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(mocks.getSessionTimeline).toHaveBeenLastCalledWith('sess-1', { cursor: 'cursor-tail-1', limit: 50 });
  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['item-1', 'item-2']);
  expect(get(timelineState).tailCursor).toBe('cursor-tail-2');
});

test('append response with a new source scope replaces the cursor-coupled state', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page())
    .mockResolvedValueOnce(page({
      source_id: 'pi:/tmp/new-session.jsonl',
      items: [{ ...page().items[0], item_id: 'fresh-source' }],
      tail_cursor: 'cursor-new-source',
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['fresh-source']);
  expect(get(timelineState).sourceId).toBe('pi:/tmp/new-session.jsonl');
});

test('binding mismatch invalidates local timeline state and rebuilds from null cursor', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page())
    .mockResolvedValueOnce(page({ binding_id: 'bind-2', items: [{ ...page().items[0], item_id: 'fresh' }] }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-2');

  expect(mocks.getSessionTimeline).toHaveBeenLastCalledWith('sess-1', { cursor: null, limit: 50 });
  expect(get(timelineState)).toMatchObject({ bindingId: 'bind-2', items: [expect.objectContaining({ item_id: 'fresh' })] });
});

test('not ready keeps timeline empty without recording a global error', async () => {
  mocks.getSessionTimeline
    .mockRejectedValueOnce(new ApiError('capability unavailable: source_unavailable: pi session file missing', 'not_ready', 422));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  expect(get(timelineState)).toMatchObject({
    sessionId: 'sess-1',
    bindingId: null,
    sourceId: null,
    tailCursor: null,
    items: [],
    loading: false,
    refreshing: false,
    error: null,
  });
});

test('cursor invalid clears cursor-coupled state and records the error', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page())
    .mockRejectedValueOnce(new ApiError('bad cursor', 'cursor_invalid', 422));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(get(timelineState)).toMatchObject({
    sessionId: 'sess-1',
    bindingId: null,
    sourceId: null,
    tailCursor: null,
    items: [],
    error: 'bad cursor',
  });
});
