import { get, writable } from 'svelte/store';
import { getSessionTimeline } from '../api/client';
import { ApiError } from '../api/errors';
import type { TimelineItem, TimelinePage } from '../api/types';

export interface TimelineState {
  sessionId: string;
  bindingId: string | null;
  items: TimelineItem[];
  nextCursor: string | null;
  tailCursor: string | null;
  sourceId: string | null;
  hasMore: boolean;
  isTail: boolean;
  loading: boolean;
  refreshing: boolean;
  error: string | null;
}

type LoadMode = 'rebuild' | 'append' | 'more';

const INVALIDATING_ERROR_CODES = new Set(['cursor_invalid', 'source_unavailable', 'content_ref_invalid']);
const DEFAULT_LIMIT = 50;

function emptyState(sessionId = ''): TimelineState {
  return {
    sessionId,
    bindingId: null,
    items: [],
    nextCursor: null,
    tailCursor: null,
    sourceId: null,
    hasMore: false,
    isTail: true,
    loading: false,
    refreshing: false,
    error: null,
  };
}

export const timelineState = writable<TimelineState>(emptyState());

export function resetTimelineState(sessionId = ''): void {
  timelineState.set(emptyState(sessionId));
}

function applyPage(current: TimelineState, page: TimelinePage, mode: LoadMode): TimelineState {
  const sameScope = (!current.bindingId || current.bindingId === page.binding_id)
    && (!current.sourceId || current.sourceId === page.source_id);
  const append = mode !== 'rebuild' && current.sessionId === page.session_id && sameScope;
  return {
    sessionId: page.session_id,
    bindingId: page.binding_id,
    items: append ? [...current.items, ...page.items] : page.items,
    nextCursor: page.next_cursor,
    tailCursor: page.tail_cursor,
    sourceId: page.source_id,
    hasMore: page.has_more,
    isTail: page.is_tail,
    loading: false,
    refreshing: false,
    error: null,
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function shouldInvalidate(error: unknown): boolean {
  return error instanceof ApiError && INVALIDATING_ERROR_CODES.has(error.code);
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
  const cursor = mode === 'more'
    ? (existingStateMatches ? current.nextCursor : null)
    : mode === 'append'
      ? (existingStateMatches ? current.tailCursor : null)
      : null;

  timelineState.update((state) => ({
    ...(state.sessionId === sessionId ? state : emptyState(sessionId)),
    loading: mode === 'rebuild',
    refreshing: mode !== 'rebuild',
    error: null,
  }));

  try {
    let nextCursor = cursor ?? null;
    let nextMode: LoadMode = mode;
    let lastPage: TimelinePage | null = null;

    do {
      const page = await getSessionTimeline(sessionId, { cursor: nextCursor, limit: options.limit ?? DEFAULT_LIMIT });
      timelineState.update((state) => applyPage(state.sessionId === sessionId ? state : emptyState(sessionId), page, nextMode));
      lastPage = page;
      nextCursor = page.has_more ? page.next_cursor : null;
      nextMode = 'more';
    } while (nextCursor);

    return lastPage;
  } catch (error) {
    const message = errorMessage(error);
    if (shouldInvalidate(error)) {
      timelineState.set({ ...emptyState(sessionId), error: message });
    } else {
      timelineState.update((state) => ({ ...state, sessionId, loading: false, refreshing: false, error: message }));
    }
    return null;
  }
}

export async function handleTimelineMessageUpdated(sessionId: string, bindingId: string | null = null): Promise<void> {
  const current = get(timelineState);
  if (current.sessionId && current.sessionId !== sessionId) return;

  if (bindingId && current.bindingId && current.bindingId !== bindingId) {
    resetTimelineState(sessionId);
    await loadSessionTimeline(sessionId, { mode: 'rebuild' });
    return;
  }

  await loadSessionTimeline(sessionId, { mode: current.tailCursor ? 'append' : 'rebuild' });
}
