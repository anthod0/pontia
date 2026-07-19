import { afterEach, expect, test, vi } from 'vitest';
import { waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { token } from '../src/stores/auth';
import { dashboardStreamCursor } from '../src/stores/connection';
import { startEventStream, stopEventStream, subscribeDashboardEvents } from '../src/services/eventStream';
import type { DashboardStreamEvent } from '../src/api/types';

const refreshDashboardSnapshotMock = vi.hoisted(() => vi.fn(async () => {}));

vi.mock('../src/services/dashboardSnapshotRefresh', () => ({
  refreshDashboardSnapshot: refreshDashboardSnapshotMock,
}));

afterEach(() => {
  stopEventStream();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  refreshDashboardSnapshotMock.mockClear();
  dashboardStreamCursor.set(null);
  token.set('');
});

test('clears the saved token when the SSE endpoint is unauthorized', async () => {
  token.set('stale-token');
  vi.stubGlobal('fetch', vi.fn(async () => new Response('unauthorized', { status: 401, statusText: 'Unauthorized' })));

  startEventStream();

  await waitFor(() => expect(localStorage.getItem('pontia.externalApiToken')).toBe(''));
});

test('reconnects the dashboard event stream with the saved cursor', async () => {
  vi.useFakeTimers();
  const firstEvent: DashboardStreamEvent = {
    kind: 'task_event',
    id: 'row-1',
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: 'evt-1',
      task_id: 'task-new',
      event_type: 'task.updated',
      payload: { source: 'test' },
      created_at: '2026-05-14T00:00:00Z',
    },
  };
  const secondEvent = { ...firstEvent, id: 'row-2', event: { ...firstEvent.event, event_id: 'evt-2' } } satisfies DashboardStreamEvent;
  const bodies = [firstEvent, secondEvent].map((event, index) => new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new TextEncoder().encode(`id: cursor-${index + 1}\ndata: ${JSON.stringify(event)}\n\n`));
      controller.close();
    },
  }));
  const fetchMock = vi.fn(async () => new Response(bodies.shift() ?? null, { status: 200 }));
  vi.stubGlobal('fetch', fetchMock);
  token.set('secret-token');

  startEventStream();

  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
  await vi.waitFor(() => expect(get(dashboardStreamCursor)).toBe('cursor-1'));
  await vi.advanceTimersByTimeAsync(1500);

  await vi.waitFor(() => {
    const streamUrls = fetchMock.mock.calls.map((call) => String(call[0])).filter((url) => url.includes('/dashboard/events/stream'));
    expect(streamUrls.length).toBeGreaterThanOrEqual(2);
    expect(streamUrls[1]).toBe('/external/v1/dashboard/events/stream?after=cursor-1');
  });
});

test('falls back to a dashboard snapshot refresh when cursor replay is rejected', async () => {
  dashboardStreamCursor.set('stale-cursor');
  vi.stubGlobal('fetch', vi.fn(async () => new Response('cursor invalid', { status: 409, statusText: 'Conflict' })));
  token.set('secret-token');

  startEventStream();

  await vi.waitFor(() => expect(refreshDashboardSnapshotMock).toHaveBeenCalledWith({ reason: 'sse_fallback' }));
});

test('notifies dashboard event subscribers when an SSE task event arrives', async () => {
  const event: DashboardStreamEvent = {
    kind: 'task_event',
    id: 'row-1',
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: 'evt-1',
      task_id: 'task-new',
      event_type: 'task.updated',
      payload: { source: 'test' },
      created_at: '2026-05-14T00:00:00Z',
    },
  };
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new TextEncoder().encode(`id: cursor-1\ndata: ${JSON.stringify(event)}\n\n`));
      controller.close();
    },
  });
  vi.stubGlobal('fetch', vi.fn(async () => new Response(body, { status: 200 })));
  token.set('secret-token');

  const listener = vi.fn();
  const unsubscribe = subscribeDashboardEvents(listener);
  startEventStream();

  await waitFor(() => expect(listener).toHaveBeenCalledWith(event));
  unsubscribe();
});
