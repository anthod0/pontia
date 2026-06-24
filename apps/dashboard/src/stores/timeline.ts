import { get, writable } from 'svelte/store';
import { getSessionTimeline } from '../api/client';
import { ApiError } from '../api/errors';
import type { TimelineItem, TimelinePage } from '../api/types';

export type TimelineRefreshKind = 'history' | 'tail' | null;

export interface TimelineState {
  sessionId: string;
  bindingId: string | null;
  items: TimelineItem[];
  headCursor: string | null;
  tailCursor: string | null;
  sourceId: string | null;
  hasMore: boolean;
  loading: boolean;
  refreshing: boolean;
  refreshKind: TimelineRefreshKind;
  error: string | null;
}

type LoadMode = 'rebuild' | 'more';

const INVALIDATING_ERROR_CODES = new Set(['cursor_invalid', 'source_unavailable', 'content_ref_invalid']);
const NON_FATAL_ERROR_CODES = new Set(['not_ready']);
const DEFAULT_LIMIT = 3;

function emptyState(sessionId = ''): TimelineState {
  return {
    sessionId,
    bindingId: null,
    items: [],
    headCursor: null,
    tailCursor: null,
    sourceId: null,
    hasMore: false,
    loading: false,
    refreshing: false,
    refreshKind: null,
    error: null,
  };
}

function refreshKindForLoadMode(mode: LoadMode): TimelineRefreshKind {
  if (mode === 'more') return 'history';
  return null;
}

export const timelineState = writable<TimelineState>(emptyState());

type TimelineUpdateQueue = {
  promise: Promise<void>;
  dirty: boolean;
  bindingId: string | null;
};

const TIMELINE_UPDATE_MIN_INTERVAL_MS = 100;

const timelineUpdateQueues = new Map<string, TimelineUpdateQueue>();
const timelineUpdateStartedAt = new Map<string, number>();

export function resetTimelineState(sessionId = ''): void {
  timelineState.set(emptyState(sessionId));
  if (sessionId) {
    timelineUpdateQueues.delete(sessionId);
    timelineUpdateStartedAt.delete(sessionId);
  } else {
    timelineUpdateQueues.clear();
    timelineUpdateStartedAt.clear();
  }
}

function uniqueTimelineItems(items: TimelineItem[]): TimelineItem[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (seen.has(item.item_id)) return false;
    seen.add(item.item_id);
    return true;
  });
}

function upsertAppendTimelineItems(currentItems: TimelineItem[], nextItems: TimelineItem[]): TimelineItem[] {
  const positions = new Map<string, number>();
  const merged = uniqueTimelineItems(currentItems);
  merged.forEach((item, index) => positions.set(item.item_id, index));
  for (const item of nextItems) {
    const index = positions.get(item.item_id);
    if (index === undefined) {
      positions.set(item.item_id, merged.length);
      merged.push(item);
    } else {
      merged[index] = item;
    }
  }
  return merged;
}

function prependUniqueTimelineItems(currentItems: TimelineItem[], previousItems: TimelineItem[]): TimelineItem[] {
  const current = uniqueTimelineItems(currentItems);
  const seen = new Set(current.map((item) => item.item_id));
  const uniquePrevious = previousItems.filter((item) => {
    if (seen.has(item.item_id)) return false;
    seen.add(item.item_id);
    return true;
  });
  return [...uniqueTimelineItems(uniquePrevious), ...current];
}

function applyPage(current: TimelineState, page: TimelinePage, mode: LoadMode): TimelineState {
  const sameScope = (!current.bindingId || current.bindingId === page.binding_id)
    && (!current.sourceId || current.sourceId === page.source_id);
  const merge = mode !== 'rebuild' && current.sessionId === page.session_id && sameScope;
  const items = !merge
    ? uniqueTimelineItems(page.items)
    : mode === 'more'
      ? prependUniqueTimelineItems(current.items, page.items)
      : upsertAppendTimelineItems(current.items, page.items);
  return {
    sessionId: page.session_id,
    bindingId: page.binding_id,
    items,
    headCursor: page.head_cursor,
    tailCursor: mode === 'more' && merge ? current.tailCursor : page.tail_cursor,
    sourceId: page.source_id,
    hasMore: page.has_more,
    loading: false,
    refreshing: false,
    refreshKind: null,
    error: null,
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function shouldInvalidate(error: unknown): boolean {
  return error instanceof ApiError && INVALIDATING_ERROR_CODES.has(error.code);
}

function isNonFatal(error: unknown): boolean {
  return error instanceof ApiError && NON_FATAL_ERROR_CODES.has(error.code);
}

export async function loadSessionTimeline(
  sessionId: string,
  options: { mode?: LoadMode; limit?: number } = {},
): Promise<TimelinePage | null> {
  if (!sessionId) {
    resetTimelineState();
    return null;
  }

  const mode = options.mode ?? 'rebuild';
  const current = get(timelineState);
  const existingStateMatches = current.sessionId === sessionId;
  const headCursor = mode === 'more' && existingStateMatches ? current.headCursor : null;

  timelineState.update((state) => ({
    ...(state.sessionId === sessionId ? state : emptyState(sessionId)),
    loading: mode === 'rebuild',
    refreshing: mode !== 'rebuild',
    refreshKind: refreshKindForLoadMode(mode),
    error: null,
  }));

  try {
    const page = await getSessionTimeline(sessionId, { before: headCursor ?? null, limit: options.limit ?? DEFAULT_LIMIT });
    timelineState.update((state) => applyPage(state.sessionId === sessionId ? state : emptyState(sessionId), page, mode));
    return page;
  } catch (error) {
    const message = errorMessage(error);
    if (isNonFatal(error)) {
      timelineState.update((state) => ({
        ...(state.sessionId === sessionId ? state : emptyState(sessionId)),
        sessionId,
        loading: false,
        refreshing: false,
        refreshKind: null,
        error: null,
      }));
    } else if (shouldInvalidate(error)) {
      timelineState.set({ ...emptyState(sessionId), error: message });
    } else {
      timelineState.update((state) => ({ ...state, sessionId, loading: false, refreshing: false, refreshKind: null, error: message }));
    }
    return null;
  }
}

function applyUpdates(current: TimelineState, updates: TimelinePage): TimelineState {
  const sameScope = current.sessionId === updates.session_id
    && (!current.bindingId || current.bindingId === updates.binding_id)
    && (!current.sourceId || current.sourceId === updates.source_id);
  if (!sameScope) {
    return {
      ...emptyState(updates.session_id),
      bindingId: updates.binding_id,
      sourceId: updates.source_id,
      items: uniqueTimelineItems(updates.items),
      headCursor: updates.head_cursor,
      tailCursor: updates.tail_cursor,
      hasMore: updates.has_more,
      loading: false,
      refreshing: false,
      refreshKind: null,
      error: null,
    };
  }
  return {
    ...current,
    bindingId: updates.binding_id,
    sourceId: updates.source_id,
    items: upsertAppendTimelineItems(current.items, updates.items),
    tailCursor: updates.tail_cursor,
    loading: false,
    refreshing: false,
    refreshKind: null,
    error: null,
  };
}

async function refreshSessionTimelineUpdates(sessionId: string, tailCursor: string | null): Promise<void> {
  if (!tailCursor) {
    await loadSessionTimeline(sessionId, { mode: 'rebuild' });
    return;
  }

  timelineState.update((state) => ({
    ...(state.sessionId === sessionId ? state : emptyState(sessionId)),
    refreshing: true,
    refreshKind: 'tail',
    error: null,
  }));

  try {
    const updates = await getSessionTimeline(sessionId, { after: tailCursor });
    timelineState.update((state) => applyUpdates(state.sessionId === sessionId ? state : emptyState(sessionId), updates));
  } catch (error) {
    const message = errorMessage(error);
    if (isNonFatal(error)) {
      timelineState.update((state) => ({
        ...(state.sessionId === sessionId ? state : emptyState(sessionId)),
        sessionId,
        loading: false,
        refreshing: false,
        refreshKind: null,
        error: null,
      }));
    } else if (shouldInvalidate(error)) {
      timelineState.set({ ...emptyState(sessionId), error: message });
    } else {
      timelineState.update((state) => ({ ...state, sessionId, loading: false, refreshing: false, refreshKind: null, error: message }));
    }
  }
}

export function handleTimelineMessageUpdated(sessionId: string, bindingId: string | null = null): Promise<void> {
  const existing = timelineUpdateQueues.get(sessionId);
  if (existing) {
    existing.dirty = true;
    if (bindingId) existing.bindingId = bindingId;
    return existing.promise;
  }

  const queue: TimelineUpdateQueue = {
    promise: Promise.resolve(),
    dirty: false,
    bindingId,
  };
  queue.promise = runTimelineUpdateQueue(sessionId, queue);
  timelineUpdateQueues.set(sessionId, queue);
  return queue.promise;
}

async function runTimelineUpdateQueue(sessionId: string, queue: TimelineUpdateQueue): Promise<void> {
  try {
    do {
      queue.dirty = false;
      await waitForTimelineUpdateSlot(sessionId);
      timelineUpdateStartedAt.set(sessionId, Date.now());
      await handleTimelineMessageUpdatedNow(sessionId, queue.bindingId);
    } while (queue.dirty);
  } finally {
    if (timelineUpdateQueues.get(sessionId) === queue) timelineUpdateQueues.delete(sessionId);
  }
}

async function waitForTimelineUpdateSlot(sessionId: string): Promise<void> {
  const lastStartedAt = timelineUpdateStartedAt.get(sessionId);
  if (lastStartedAt === undefined) return;
  const remainingMs = TIMELINE_UPDATE_MIN_INTERVAL_MS - (Date.now() - lastStartedAt);
  if (remainingMs <= 0) return;
  await new Promise((resolve) => setTimeout(resolve, remainingMs));
}

async function handleTimelineMessageUpdatedNow(sessionId: string, bindingId: string | null = null): Promise<void> {
  const current = get(timelineState);
  if (current.sessionId && current.sessionId !== sessionId) return;

  if (bindingId && current.bindingId && current.bindingId !== bindingId) {
    resetTimelineState(sessionId);
    await loadSessionTimeline(sessionId, { mode: 'rebuild' });
    return;
  }

  if (!current.items.length) {
    await loadSessionTimeline(sessionId, { mode: 'rebuild' });
    return;
  }

  await refreshSessionTimelineUpdates(sessionId, current.tailCursor);
}
