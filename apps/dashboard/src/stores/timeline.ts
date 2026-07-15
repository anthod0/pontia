import { get, writable } from 'svelte/store';
import { getTurnTimeline } from '../api/client';
import { ApiError } from '../api/errors';
import type { TurnTimelineItem, TurnTimelinePage } from '../api/types';

export type TimelineRefreshKind = 'history' | 'tail' | null;
export type TimelineStatus =
  | 'idle'
  | 'loading'
  | 'ready'
  | 'empty'
  | 'query_error'
  | 'capability_unavailable'
  | 'range_unavailable'
  | 'range_invalid'
  | 'source_unavailable'
  | 'error';

export interface TimelineState {
  sessionId: string;
  items: TurnTimelineItem[];
  nextOlderTurnId: string | null;
  latestTurnId: string | null;
  hasMore: boolean;
  loading: boolean;
  refreshing: boolean;
  refreshKind: TimelineRefreshKind;
  status: TimelineStatus;
  errorCode: string | null;
  error: string | null;
}

type LoadMode = 'rebuild' | 'more';

const DEFAULT_HISTORY_LIMIT = 3;
const FORWARD_LIMIT = 100;
const TIMELINE_UPDATE_DEBOUNCE_MS = 100;

function emptyState(sessionId = ''): TimelineState {
  return {
    sessionId,
    items: [],
    nextOlderTurnId: null,
    latestTurnId: null,
    hasMore: false,
    loading: false,
    refreshing: false,
    refreshKind: null,
    status: 'idle',
    errorCode: null,
    error: null,
  };
}

export const timelineState = writable<TimelineState>(emptyState());

type TimelineUpdateQueue = {
  promise: Promise<void>;
  resolve: () => void;
  reject: (error: unknown) => void;
  timer: ReturnType<typeof setTimeout> | null;
  running: boolean;
  dirty: boolean;
  turnId: string | null;
};

const timelineUpdateQueues = new Map<string, TimelineUpdateQueue>();

export function resetTimelineState(sessionId = ''): void {
  timelineState.set(emptyState(sessionId));
  if (sessionId) {
    clearTimelineUpdateQueue(sessionId);
  } else {
    for (const queuedSessionId of timelineUpdateQueues.keys()) clearTimelineUpdateQueue(queuedSessionId);
  }
}

function uniqueTimelineItems(items: TurnTimelineItem[]): TurnTimelineItem[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (seen.has(item.item_id)) return false;
    seen.add(item.item_id);
    return true;
  });
}

function prependUnseenTurnGroups(
  currentItems: TurnTimelineItem[],
  previousItems: TurnTimelineItem[],
): TurnTimelineItem[] {
  const currentTurnIds = new Set(currentItems.map((item) => item.turn_id));
  return [
    ...uniqueTimelineItems(previousItems.filter((item) => !currentTurnIds.has(item.turn_id))),
    ...currentItems,
  ];
}

function replaceReturnedTurnGroups(
  currentItems: TurnTimelineItem[],
  returnedItems: TurnTimelineItem[],
): TurnTimelineItem[] {
  const replacement = uniqueTimelineItems(returnedItems);
  const returnedTurnIds = new Set(replacement.map((item) => item.turn_id));
  if (!returnedTurnIds.size) return currentItems;

  const firstReturnedIndex = currentItems.findIndex((item) => returnedTurnIds.has(item.turn_id));
  if (firstReturnedIndex < 0) return [...currentItems, ...replacement];

  return [
    ...currentItems.slice(0, firstReturnedIndex).filter((item) => !returnedTurnIds.has(item.turn_id)),
    ...replacement,
    ...currentItems.slice(firstReturnedIndex).filter((item) => !returnedTurnIds.has(item.turn_id)),
  ];
}

function statusForItems(items: TurnTimelineItem[]): TimelineStatus {
  return items.length ? 'ready' : 'empty';
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function errorStatus(error: unknown): TimelineStatus {
  if (!(error instanceof ApiError)) return 'error';
  switch (error.code) {
    case 'invalid_timeline_query': return 'query_error';
    case 'timeline_capability_unavailable': return 'capability_unavailable';
    case 'turn_timeline_unavailable': return 'range_unavailable';
    case 'turn_timeline_invalid': return 'range_invalid';
    case 'timeline_source_unavailable': return 'source_unavailable';
    default: return 'error';
  }
}

function applyTimelineError(sessionId: string, error: unknown): void {
  timelineState.update((state) => state.sessionId !== sessionId ? state : ({
    ...emptyState(sessionId),
    status: errorStatus(error),
    errorCode: error instanceof ApiError ? error.code : 'request_failed',
    error: errorMessage(error),
  }));
}

export async function loadSessionTimeline(
  sessionId: string,
  options: { mode?: LoadMode; limit?: number; latestTurnId?: string | null } = {},
): Promise<TurnTimelinePage | null> {
  if (!sessionId) {
    resetTimelineState();
    return null;
  }

  const mode = options.mode ?? 'rebuild';
  const current = get(timelineState);
  const sameSession = current.sessionId === sessionId;
  const turnId = mode === 'more' && sameSession ? current.nextOlderTurnId : null;
  if (mode === 'more' && !turnId) return null;

  timelineState.update((state) => {
    const scoped = state.sessionId === sessionId ? state : emptyState(sessionId);
    return {
      ...scoped,
      latestTurnId: options.latestTurnId ?? scoped.latestTurnId,
      loading: mode === 'rebuild',
      refreshing: mode === 'more',
      refreshKind: mode === 'more' ? 'history' : null,
      status: mode === 'rebuild' ? 'loading' : scoped.status,
      errorCode: null,
      error: null,
    };
  });

  try {
    const page = await getTurnTimeline(sessionId, {
      direction: 'backward',
      ...(turnId ? { turnId } : {}),
      limit: options.limit ?? DEFAULT_HISTORY_LIMIT,
    });
    timelineState.update((state) => {
      if (state.sessionId !== sessionId) return state;
      const items = mode === 'more'
        ? prependUnseenTurnGroups(state.items, page.items)
        : uniqueTimelineItems(page.items);
      return {
        ...state,
        items,
        nextOlderTurnId: page.next_turn_id,
        latestTurnId: mode === 'rebuild' && options.latestTurnId
          ? options.latestTurnId
          : items.at(-1)?.turn_id ?? state.latestTurnId,
        hasMore: page.next_turn_id !== null,
        loading: false,
        refreshing: false,
        refreshKind: null,
        status: statusForItems(items),
        errorCode: null,
        error: null,
      };
    });
    return page;
  } catch (error) {
    applyTimelineError(sessionId, error);
    return null;
  }
}

async function loadForwardPages(sessionId: string, initialTurnId: string): Promise<TurnTimelineItem[]> {
  const items: TurnTimelineItem[] = [];
  const seenAnchors = new Set<string>();
  let turnId: string | null = initialTurnId;

  while (turnId) {
    if (seenAnchors.has(turnId)) throw new Error('Turn timeline pagination returned a repeated anchor');
    seenAnchors.add(turnId);
    const page = await getTurnTimeline(sessionId, {
      direction: 'forward',
      turnId,
      limit: FORWARD_LIMIT,
    });
    items.push(...page.items);
    turnId = page.next_turn_id;
  }

  return items;
}

async function refreshSessionTimelineUpdates(sessionId: string, latestTurnId: string): Promise<void> {
  timelineState.update((state) => ({
    ...(state.sessionId === sessionId ? state : emptyState(sessionId)),
    refreshing: true,
    refreshKind: 'tail',
    errorCode: null,
    error: null,
  }));

  try {
    const updates = await loadForwardPages(sessionId, latestTurnId);
    timelineState.update((state) => {
      if (state.sessionId !== sessionId) return state;
      const items = replaceReturnedTurnGroups(state.items, updates);
      return {
        ...state,
        items,
        latestTurnId: items.at(-1)?.turn_id ?? latestTurnId,
        loading: false,
        refreshing: false,
        refreshKind: null,
        status: statusForItems(items),
        errorCode: null,
        error: null,
      };
    });
  } catch (error) {
    applyTimelineError(sessionId, error);
  }
}

export function handleTimelineMessageUpdated(sessionId: string, turnId: string | null = null): Promise<void> {
  const existing = timelineUpdateQueues.get(sessionId);
  if (existing) {
    existing.dirty = true;
    if (turnId) existing.turnId = turnId;
    if (!existing.running) scheduleTimelineUpdate(sessionId, existing);
    return existing.promise;
  }

  let resolveQueue: () => void = () => {};
  let rejectQueue: (error: unknown) => void = () => {};
  const queue: TimelineUpdateQueue = {
    promise: new Promise<void>((resolve, reject) => {
      resolveQueue = resolve;
      rejectQueue = reject;
    }),
    resolve: resolveQueue,
    reject: rejectQueue,
    timer: null,
    running: false,
    dirty: true,
    turnId,
  };
  timelineUpdateQueues.set(sessionId, queue);
  scheduleTimelineUpdate(sessionId, queue);
  return queue.promise;
}

function scheduleTimelineUpdate(sessionId: string, queue: TimelineUpdateQueue): void {
  if (queue.timer) clearTimeout(queue.timer);
  queue.timer = setTimeout(() => {
    queue.timer = null;
    void runTimelineUpdateQueue(sessionId, queue);
  }, TIMELINE_UPDATE_DEBOUNCE_MS);
}

async function runTimelineUpdateQueue(sessionId: string, queue: TimelineUpdateQueue): Promise<void> {
  if (queue.running) return;
  queue.running = true;
  queue.dirty = false;
  try {
    await handleTimelineMessageUpdatedNow(sessionId, queue.turnId);
    queue.running = false;
    if (queue.dirty && timelineUpdateQueues.get(sessionId) === queue) {
      scheduleTimelineUpdate(sessionId, queue);
      return;
    }
    if (timelineUpdateQueues.get(sessionId) === queue) timelineUpdateQueues.delete(sessionId);
    queue.resolve();
  } catch (error) {
    queue.running = false;
    if (timelineUpdateQueues.get(sessionId) === queue) timelineUpdateQueues.delete(sessionId);
    queue.reject(error);
  }
}

function clearTimelineUpdateQueue(sessionId: string): void {
  const queue = timelineUpdateQueues.get(sessionId);
  if (!queue) return;
  if (queue.timer) clearTimeout(queue.timer);
  timelineUpdateQueues.delete(sessionId);
  if (!queue.running) queue.resolve();
}

async function handleTimelineMessageUpdatedNow(sessionId: string, turnId: string | null): Promise<void> {
  const current = get(timelineState);
  if (current.sessionId && current.sessionId !== sessionId) return;

  const forwardAnchorTurnId = turnId ?? current.latestTurnId;
  if (!forwardAnchorTurnId) {
    await loadSessionTimeline(sessionId, { mode: 'rebuild' });
    return;
  }
  await refreshSessionTimelineUpdates(sessionId, forwardAnchorTurnId);
}
