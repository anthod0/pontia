import { get } from 'svelte/store';
import { beforeEach, expect, test, vi } from 'vitest';
import { ApiError } from '../src/api/errors';
import type { TimelinePage, TimelineUpdatesPage } from '../src/api/types';

const mocks = vi.hoisted(() => ({
  getSessionTimeline: vi.fn(),
  getSessionTimelineUpdates: vi.fn(),
}));

vi.mock('../src/api/client', () => ({
  getSessionTimeline: mocks.getSessionTimeline,
  getSessionTimelineUpdates: mocks.getSessionTimelineUpdates,
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
    older_cursor: null,
    has_more: false,
    source_id: 'pi:/tmp/session.jsonl',
    ...overrides,
  };
}

function updates(overrides: Partial<TimelineUpdatesPage> = {}): TimelineUpdatesPage {
  return {
    session_id: 'sess-1',
    binding_id: 'bind-1',
    after_item_id: 'item-1',
    items: [],
    anchor_found: true,
    truncated: false,
    source_id: 'pi:/tmp/session.jsonl',
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getSessionTimeline.mockReset();
  mocks.getSessionTimelineUpdates.mockReset();
  resetTimelineState();
});

test('rebuild stores only the latest timeline page using the default three-round limit', async () => {
  mocks.getSessionTimeline.mockResolvedValueOnce(page({ older_cursor: 'older-cursor', has_more: true }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  expect(mocks.getSessionTimeline).toHaveBeenCalledTimes(1);
  expect(mocks.getSessionTimeline).toHaveBeenCalledWith('sess-1', { olderCursor: null, limit: 3 });
  expect(get(timelineState)).toMatchObject({
    sessionId: 'sess-1',
    bindingId: 'bind-1',
    sourceId: 'pi:/tmp/session.jsonl',
    olderCursor: 'older-cursor',
    items: [expect.objectContaining({ item_id: 'item-1' })],
    loading: false,
    error: null,
  });
});

test('more loads older rounds from next cursor and prepends them', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page({
      items: [{ ...page().items[0], item_id: 'newer', content_ref: 'ref-newer' }],
      older_cursor: 'older-cursor',
      has_more: true,
    }))
    .mockResolvedValueOnce(page({
      items: [{ ...page().items[0], item_id: 'older', content_ref: 'ref-older' }],
      older_cursor: null,
      has_more: false,
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await loadSessionTimeline('sess-1', { mode: 'more' });

  expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(1, 'sess-1', { olderCursor: null, limit: 3 });
  expect(mocks.getSessionTimeline).toHaveBeenNthCalledWith(2, 'sess-1', { olderCursor: 'older-cursor', limit: 3 });
  expect(get(timelineState)).toMatchObject({
    items: [expect.objectContaining({ item_id: 'older' }), expect.objectContaining({ item_id: 'newer' })],
    olderCursor: null,
    hasMore: false,
  });
});

test('message update with matching binding loads updates after the latest known item', async () => {
  mocks.getSessionTimeline.mockResolvedValueOnce(page());
  mocks.getSessionTimelineUpdates.mockResolvedValueOnce(updates({
    items: [{ ...page().items[0], item_id: 'item-2', content_ref: 'ref-2', content_preview: 'next' }],
  }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(mocks.getSessionTimelineUpdates).toHaveBeenCalledWith('sess-1', { afterItemId: 'item-1' });
  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['item-1', 'item-2']);
});

test('message updates for the same session are serialized so each refresh uses the latest item anchor', async () => {
  let resolveFirstUpdate: (value: TimelineUpdatesPage) => void = () => {};
  const firstRefresh = new Promise<TimelineUpdatesPage>((resolve) => {
    resolveFirstUpdate = resolve;
  });

  mocks.getSessionTimeline.mockResolvedValueOnce(page());
  mocks.getSessionTimelineUpdates
    .mockReturnValueOnce(firstRefresh)
    .mockResolvedValueOnce(updates({ after_item_id: 'item-2', items: [] }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  const firstUpdate = handleTimelineMessageUpdated('sess-1', 'bind-1');
  const secondUpdate = handleTimelineMessageUpdated('sess-1', 'bind-1');

  await vi.waitFor(() => {
    expect(mocks.getSessionTimelineUpdates).toHaveBeenNthCalledWith(1, 'sess-1', { afterItemId: 'item-1' });
  });

  resolveFirstUpdate(updates({
    items: [{ ...page().items[0], item_id: 'item-2', content_ref: 'ref-2', content_preview: 'next' }],
  }));
  await Promise.all([firstUpdate, secondUpdate]);

  expect(mocks.getSessionTimelineUpdates).toHaveBeenNthCalledWith(2, 'sess-1', { afterItemId: 'item-2' });
  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['item-1', 'item-2']);
});

test('append keeps timeline items unique by item id', async () => {
  mocks.getSessionTimeline.mockResolvedValueOnce(page());
  mocks.getSessionTimelineUpdates.mockResolvedValueOnce(updates({
    items: [
      { ...page().items[0], item_id: 'item-1' },
      { ...page().items[0], item_id: 'item-2', content_ref: 'ref-2', content_preview: 'next' },
    ],
  }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['item-1', 'item-2']);
});

test('append response with a new source scope replaces the cursor-coupled state', async () => {
  mocks.getSessionTimeline.mockResolvedValueOnce(page());
  mocks.getSessionTimelineUpdates.mockResolvedValueOnce(updates({
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

  expect(mocks.getSessionTimeline).toHaveBeenLastCalledWith('sess-1', { olderCursor: null, limit: 3 });
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
  mocks.getSessionTimeline.mockResolvedValueOnce(page());
  mocks.getSessionTimelineUpdates.mockRejectedValueOnce(new ApiError('bad cursor', 'cursor_invalid', 422));

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


test('message update rebuilds latest window when update anchor is missing', async () => {
  mocks.getSessionTimeline
    .mockResolvedValueOnce(page())
    .mockResolvedValueOnce(page({ items: [{ ...page().items[0], item_id: 'fresh', content_ref: 'ref-fresh' }] }));
  mocks.getSessionTimelineUpdates.mockResolvedValueOnce(updates({ anchor_found: false }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await handleTimelineMessageUpdated('sess-1', 'bind-1');

  expect(mocks.getSessionTimelineUpdates).toHaveBeenCalledWith('sess-1', { afterItemId: 'item-1' });
  expect(mocks.getSessionTimeline).toHaveBeenLastCalledWith('sess-1', { olderCursor: null, limit: 3 });
  expect(get(timelineState).items.map((item) => item.item_id)).toEqual(['fresh']);
});
