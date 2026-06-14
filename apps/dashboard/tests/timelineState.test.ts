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
    head_cursor: null,
    tail_cursor: 'tail-1',
    has_more: false,
    source_id: 'pi:/tmp/session.jsonl',
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getSessionTimeline.mockReset();
  resetTimelineState();
});

test('rebuild stores only the latest timeline page using the default three-round limit', async () => {
  mocks.getSessionTimeline.mockResolvedValueOnce(page({ head_cursor: 'head-cursor', tail_cursor: 'tail-cursor', has_more: true }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  expect(mocks.getSessionTimeline).toHaveBeenCalledTimes(1);
  expect(mocks.getSessionTimeline).toHaveBeenCalledWith('sess-1', { before: null, limit: 3 });
  expect(get(timelineState)).toMatchObject({
    sessionId: 'sess-1',
    bindingId: 'bind-1',
    sourceId: 'pi:/tmp/session.jsonl',
    headCursor: 'head-cursor',
    tailCursor: 'tail-cursor',
    items: [expect.objectContaining({ item_id: 'item-1' })],
    loading: false,
    error: null,
  });
});

test('more loads older rounds from next cursor and prepends them', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page({
      items: [{ ...page().items[0], item_id: 'newer', content_ref: 'ref-newer' }],
      head_cursor: 'head-cursor',
      tail_cursor: 'tail-cursor',
      has_more: true,
    }))
    .mockResolvedValueOnce(page({
      items: [{ ...page().items[0], item_id: 'older', content_ref: 'ref-older' }],
      head_cursor: null,
      tail_cursor: 'ignored-old-tail',
      has_more: false,
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await loadSessionTimeline('sess-1', { mode: 'more' });

  expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(1, 'sess-1', { before: null, limit: 3 });
  expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(2, 'sess-1', { before: 'head-cursor', limit: 3 });
  expect(get(timelineState)).toMatchObject({
    items: [expect.objectContaining({ item_id: 'older' }), expect.objectContaining({ item_id: 'newer' })],
    headCursor: null,
    tailCursor: 'tail-cursor',
    hasMore: false,
  });
});

test('message update with matching binding loads updates after the saved tail cursor', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page({ tail_cursor: 'tail-1' }))
    .mockResolvedValueOnce(page({
      items: [{ ...page().items[0], item_id: 'item-2', content_ref: 'ref-2', content_preview: 'next' }],
      head_cursor: null,
      tail_cursor: 'tail-2',
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(2, 'sess-1', { after: 'tail-1' });
  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['item-1', 'item-2']);
  expect(get(timelineState).tailCursor).toBe('tail-2');
});

test('message updates for the same session are serialized so each refresh uses the latest tail cursor', async () => {
  let resolveFirstUpdate: (value: TimelinePage) => void = () => {};
  const firstRefresh = new Promise<TimelinePage>((resolve) => {
    resolveFirstUpdate = resolve;
  });

  mocks.getSessionTimeline
    .mockResolvedValueOnce(page({ tail_cursor: 'tail-1' }))
    .mockReturnValueOnce(firstRefresh)
    .mockResolvedValueOnce(page({ items: [], tail_cursor: 'tail-2' }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  const firstUpdate = handleTimelineMessageUpdated('sess-1', 'bind-1');
  const secondUpdate = handleTimelineMessageUpdated('sess-1', 'bind-1');

  await vi.waitFor(() => {
    expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(2, 'sess-1', { after: 'tail-1' });
  });

  resolveFirstUpdate(page({
    items: [{ ...page().items[0], item_id: 'item-2', content_ref: 'ref-2', content_preview: 'next' }],
    tail_cursor: 'tail-2',
  }));
  await Promise.all([firstUpdate, secondUpdate]);

  expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(3, 'sess-1', { after: 'tail-2' });
  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['item-1', 'item-2']);
});

test('append keeps timeline items unique by item id', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page())
    .mockResolvedValueOnce(page({
      items: [
        { ...page().items[0], item_id: 'item-1' },
        { ...page().items[0], item_id: 'item-2', content_ref: 'ref-2', content_preview: 'next' },
      ],
      tail_cursor: 'tail-2',
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['item-1', 'item-2']);
});

test('append response with a new source scope replaces the cursor-coupled state', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page())
    .mockResolvedValueOnce(page({
      source_id: 'pi:/tmp/new-session.jsonl',
      items: [{ ...page().items[0], item_id: 'fresh-source' }],
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

  expect(mocks.getSessionTimeline).toHaveBeenLastCalledWith('sess-1', { before: null, limit: 3 });
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
    items: [],
    error: 'bad cursor',
  });
});


test('message update rebuilds latest window when no tail cursor is available', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page({ tail_cursor: null }))
    .mockResolvedValueOnce(page({ items: [{ ...page().items[0], item_id: 'fresh', content_ref: 'ref-fresh' }] }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(mocks.getSessionTimeline).toHaveBeenLastCalledWith('sess-1', { before: null, limit: 3 });
  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['fresh']);
});
