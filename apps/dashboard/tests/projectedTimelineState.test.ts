import { get } from 'svelte/store';
import { beforeEach, expect, test, vi } from 'vitest';
import { ApiError } from '../src/api/errors';
import type { TurnTimelineGroup, TurnTimelineItem, TurnTimelinePage, TurnTreeHistoryPage, TurnTreeUpdatesPage } from '../src/api/types';

const mocks = vi.hoisted(() => ({
  getTurnTimeline: vi.fn(),
  getTurnTreeHistory: vi.fn(),
  getTurnTreeUpdates: vi.fn(),
}));

vi.mock('../src/api/client', () => ({
  getTurnTimeline: mocks.getTurnTimeline,
  getTurnTreeHistory: mocks.getTurnTreeHistory,
  getTurnTreeUpdates: mocks.getTurnTreeUpdates,
}));

const {
  loadSessionTimeline,
  refreshSessionTimeline,
  resetTimelineState,
  timelineState,
} = await import('../src/stores/timeline');

function item(turnId: string, itemId: string, preview: string): TurnTimelineItem {
  return {
    item_id: itemId,
    kind: 'assistant',
    role: 'assistant',
    title: null,
    status: null,
    occurred_at: '2026-07-15T00:00:00Z',
    content_preview: preview,
    turn_id: turnId,
  };
}

function group(turnId: string, turnIndex: number, preview = turnId, parentTurnId: string | null = null): TurnTimelineGroup {
  return {
    turn_id: turnId,
    turn_index: turnIndex,
    parent_turn_id: parentTurnId,
    state: 'completed',
    items: [item(turnId, `item-${turnId}`, preview)],
  };
}

function historyPage(overrides: Partial<TurnTreeHistoryPage> = {}): TurnTreeHistoryPage {
  return {
    session_id: 'sess-1',
    groups: [group('turn-4', 4, 'branch four', 'turn-1'), group('turn-5', 5, 'branch five', 'turn-4')],
    next_from_turn_id: 'turn-1',
    ...overrides,
  };
}

function updatesPage(overrides: Partial<TurnTreeUpdatesPage> = {}): TurnTreeUpdatesPage {
  return {
    session_id: 'sess-1',
    current_turn_id: 'turn-5',
    retain_through_turn_id: 'turn-1',
    groups: [group('turn-4', 4, 'branch four', 'turn-1'), group('turn-5', 5, 'branch five', 'turn-4')],
    ...overrides,
  };
}

function page(overrides: Partial<TurnTimelinePage> = {}): TurnTimelinePage {
  return {
    session_id: 'sess-1',
    direction: 'backward',
    items: [item('turn-2', 'item-2', 'newer')],
    next_turn_id: 'turn-1',
    ...overrides,
  };
}

beforeEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
  mocks.getTurnTimeline.mockReset();
  mocks.getTurnTreeHistory.mockReset();
  mocks.getTurnTreeUpdates.mockReset();
  resetTimelineState();
});

test('tree history follows backend groups and paginates without a duplicate boundary Turn', async () => {
  mocks.getTurnTreeHistory
    .mockResolvedValueOnce(historyPage())
    .mockResolvedValueOnce(historyPage({
      groups: [group('turn-1', 1, 'root')],
      next_from_turn_id: null,
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild', topology: true });
  await loadSessionTimeline('sess-1', { mode: 'more', topology: true });

  expect(mocks.getTurnTreeHistory).toHaveBeenNthCalledWith(1, 'sess-1', { limit: 5 });
  expect(mocks.getTurnTreeHistory).toHaveBeenNthCalledWith(2, 'sess-1', {
    fromTurnId: 'turn-1',
    limit: 5,
  });
  expect(get(timelineState).groups.map((entry) => entry.turn_id)).toEqual([
    'turn-1',
    'turn-4',
    'turn-5',
  ]);
  expect(get(timelineState)).toMatchObject({
    mode: 'tree',
    latestTurnId: 'turn-5',
    nextOlderTurnId: null,
    hasMore: false,
  });
});

test('tree updates discard a divergent suffix after the LCA and append the replacement branch', async () => {
  mocks.getTurnTreeHistory.mockResolvedValueOnce(historyPage({
    groups: [
      group('turn-1', 1, 'root'),
      group('turn-2', 2, 'old two', 'turn-1'),
      group('turn-3', 3, 'old three', 'turn-2'),
    ],
    next_from_turn_id: null,
  }));
  mocks.getTurnTreeUpdates.mockResolvedValueOnce(updatesPage());

  await loadSessionTimeline('sess-1', { mode: 'rebuild', topology: true });
  await refreshSessionTimeline('sess-1');

  expect(mocks.getTurnTreeUpdates).toHaveBeenCalledWith('sess-1', { fromTurnId: 'turn-3' });
  expect(get(timelineState).groups.map((entry) => entry.turn_id)).toEqual([
    'turn-1',
    'turn-4',
    'turn-5',
  ]);
  expect(get(timelineState).items.map((entry) => entry.content_preview)).toEqual([
    'root',
    'branch four',
    'branch five',
  ]);
});

test('realtime refresh uses an inclusive forward Turn anchor and atomically replaces returned groups', async () => {
  mocks.getTurnTimeline
    .mockResolvedValueOnce(page({
      items: [
        item('turn-1', 'item-1', 'older'),
        item('turn-2', 'item-2-stale', 'partial'),
      ],
      next_turn_id: null,
    }))
    .mockResolvedValueOnce(page({
      direction: 'forward',
      items: [
        item('turn-2', 'item-2-final', 'sealed'),
        item('turn-3', 'item-3', 'newest'),
      ],
      next_turn_id: null,
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await refreshSessionTimeline('sess-1');

  expect(mocks.getTurnTimeline).toHaveBeenNthCalledWith(2, 'sess-1', {
    direction: 'forward',
    turnId: 'turn-2',
    limit: 100,
  });
  expect(get(timelineState).items.map(({ turn_id, item_id }) => [turn_id, item_id])).toEqual([
    ['turn-1', 'item-1'],
    ['turn-2', 'item-2-final'],
    ['turn-3', 'item-3'],
  ]);
  expect(get(timelineState).latestTurnId).toBe('turn-3');
});

test('initial history loads chronological projected Turn items in the backward direction', async () => {
  mocks.getTurnTimeline.mockResolvedValueOnce(page());

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  expect(mocks.getTurnTimeline).toHaveBeenCalledWith('sess-1', {
    direction: 'backward',
    limit: 5,
  });
  expect(get(timelineState)).toMatchObject({
    sessionId: 'sess-1',
    items: [expect.objectContaining({ item_id: 'item-2', turn_id: 'turn-2' })],
    nextOlderTurnId: 'turn-1',
    latestTurnId: 'turn-2',
    hasMore: true,
    status: 'ready',
    error: null,
  });
});

test('older history uses next_turn_id and prepends only previously unseen Turn groups', async () => {
  mocks.getTurnTimeline
    .mockResolvedValueOnce(page())
    .mockResolvedValueOnce(page({
      items: [
        item('turn-1', 'item-1', 'older'),
        item('turn-2', 'item-2-boundary', 'duplicate boundary Turn'),
      ],
      next_turn_id: null,
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await loadSessionTimeline('sess-1', { mode: 'more' });

  expect(mocks.getTurnTimeline).toHaveBeenNthCalledWith(2, 'sess-1', {
    direction: 'backward',
    turnId: 'turn-1',
    limit: 5,
  });
  expect(get(timelineState).items.map(({ turn_id, item_id }) => [turn_id, item_id])).toEqual([
    ['turn-1', 'item-1'],
    ['turn-2', 'item-2'],
  ]);
  expect(get(timelineState)).toMatchObject({ nextOlderTurnId: null, hasMore: false });
});

test('an active Turn grows from empty output and converges on its sealed group through forward replacement', async () => {
  mocks.getTurnTimeline
    .mockResolvedValueOnce(page({ items: [], next_turn_id: null }))
    .mockResolvedValueOnce(page({
      direction: 'forward',
      items: [item('turn-active', 'item-partial', 'working')],
      next_turn_id: null,
    }))
    .mockResolvedValueOnce(page({
      direction: 'forward',
      items: [item('turn-active', 'item-final', 'done')],
      next_turn_id: null,
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild', latestTurnId: 'turn-active' });
  expect(get(timelineState)).toMatchObject({ items: [], latestTurnId: 'turn-active', status: 'empty' });

  await refreshSessionTimeline('sess-1', 'turn-active');
  expect(get(timelineState).items.map((entry) => entry.item_id)).toEqual(['item-partial']);

  await refreshSessionTimeline('sess-1', 'turn-active');
  expect(get(timelineState).items.map((entry) => entry.item_id)).toEqual(['item-final']);
  expect(mocks.getTurnTimeline).toHaveBeenNthCalledWith(3, 'sess-1', {
    direction: 'forward',
    turnId: 'turn-active',
    limit: 100,
  });
});

test('an itemless active Turn remains the forward anchor when initial history includes older sealed items', async () => {
  mocks.getTurnTimeline
    .mockResolvedValueOnce(page({ items: [item('turn-sealed', 'item-sealed', 'done')], next_turn_id: null }))
    .mockResolvedValueOnce(page({
      direction: 'forward',
      items: [item('turn-active', 'item-active', 'working')],
      next_turn_id: null,
    }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild', latestTurnId: 'turn-active' });
  expect(get(timelineState).latestTurnId).toBe('turn-active');

  await refreshSessionTimeline('sess-1');

  expect(mocks.getTurnTimeline).toHaveBeenNthCalledWith(2, 'sess-1', {
    direction: 'forward',
    turnId: 'turn-active',
    limit: 100,
  });
  expect(get(timelineState).items.map((entry) => entry.item_id)).toEqual(['item-sealed', 'item-active']);
});

test('a Session without Turns is ready with an empty conversation history', async () => {
  mocks.getTurnTimeline.mockResolvedValueOnce(page({ items: [], next_turn_id: null }));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });

  expect(get(timelineState)).toMatchObject({
    sessionId: 'sess-1',
    items: [],
    latestTurnId: null,
    nextOlderTurnId: null,
    hasMore: false,
    status: 'empty',
    error: null,
  });
});

test.each([
  ['invalid_timeline_query', 'query_error'],
  ['timeline_capability_unavailable', 'capability_unavailable'],
  ['turn_timeline_unavailable', 'range_unavailable'],
  ['turn_timeline_invalid', 'range_invalid'],
  ['timeline_source_unavailable', 'source_unavailable'],
] as const)('surfaces %s as an explicit %s state without partial history', async (code, status) => {
  mocks.getTurnTimeline
    .mockResolvedValueOnce(page())
    .mockRejectedValueOnce(new ApiError(`timeline failed: ${code}`, code, 409));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await loadSessionTimeline('sess-1', { mode: 'more' });

  expect(get(timelineState)).toMatchObject({
    sessionId: 'sess-1',
    items: [],
    status,
    errorCode: code,
    error: `timeline failed: ${code}`,
  });
});

test('forward pagination commits no intermediate page when a later range fails', async () => {
  mocks.getTurnTimeline
    .mockResolvedValueOnce(page({ items: [item('turn-1', 'item-stale', 'stale')], next_turn_id: null }))
    .mockResolvedValueOnce(page({
      direction: 'forward',
      items: [item('turn-1', 'item-refreshed', 'refreshed')],
      next_turn_id: 'turn-2',
    }))
    .mockRejectedValueOnce(new ApiError('Turn turn-2 has an invalid timeline range', 'turn_timeline_invalid', 409));

  await loadSessionTimeline('sess-1', { mode: 'rebuild' });
  await refreshSessionTimeline('sess-1');

  expect(mocks.getTurnTimeline).toHaveBeenNthCalledWith(3, 'sess-1', {
    direction: 'forward',
    turnId: 'turn-2',
    limit: 100,
  });
  expect(get(timelineState)).toMatchObject({
    items: [],
    status: 'range_invalid',
    errorCode: 'turn_timeline_invalid',
  });
});
