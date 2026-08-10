import { afterEach, expect, test, vi } from 'vitest';
import { waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { token } from '../src/stores/auth';
import { dashboardStreamCursor, lastConnectionError } from '../src/stores/connection';
import { startEventStream, stopEventStream, subscribeDashboardEvents } from '../src/services/eventStream';
import type { DashboardStreamEvent } from '../src/api/types';

const refreshDashboardSnapshotMock = vi.hoisted(() => vi.fn(async () => {}));
const initialVisibilityState = document.visibilityState;

function createAbortableStreamFetch(signals: AbortSignal[] = []) {
  return vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
    const signal = init?.signal as AbortSignal;
    signals.push(signal);
    return new Response(new ReadableStream<Uint8Array>({
      start(streamController) {
        signal.addEventListener('abort', () => streamController.error(new DOMException('Aborted', 'AbortError')), { once: true });
      },
    }), { status: 200 });
  });
}

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
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: initialVisibilityState });
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

test('visible recovery aborts the old stream and reconnects with the saved cursor', async () => {
  const signals: AbortSignal[] = [];
  const fetchMock = createAbortableStreamFetch(signals);
  vi.stubGlobal('fetch', fetchMock);
  dashboardStreamCursor.set('session:4;task:9');
  token.set('secret-token');

  startEventStream();
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));

  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
  document.dispatchEvent(new Event('visibilitychange'));

  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  expect(signals[0].aborted).toBe(true);
  expect(String(fetchMock.mock.calls[1][0])).toBe('/external/v1/dashboard/events/stream?after=session%3A4%3Btask%3A9');
  expect(get(dashboardStreamCursor)).toBe('session:4;task:9');
});

test('an aborted stale stream cannot schedule another reconnect', async () => {
  vi.useFakeTimers();
  const fetchMock = createAbortableStreamFetch();
  vi.stubGlobal('fetch', fetchMock);
  token.set('secret-token');

  startEventStream();
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
  document.dispatchEvent(new Event('visibilitychange'));
  await vi.runAllTicks();
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));

  await vi.advanceTimersByTimeAsync(10_000);

  expect(fetchMock).toHaveBeenCalledTimes(2);
});

test('coalesces near-simultaneous browser recovery signals into one replacement stream', async () => {
  const signals: AbortSignal[] = [];
  const fetchMock = createAbortableStreamFetch(signals);
  vi.stubGlobal('fetch', fetchMock);
  token.set('secret-token');

  startEventStream();
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });

  document.dispatchEvent(new Event('visibilitychange'));
  window.dispatchEvent(new Event('online'));
  window.dispatchEvent(new Event('pageshow'));

  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  expect(signals.filter((signal) => !signal.aborted)).toHaveLength(1);
});

test('does not refresh the dashboard snapshot when an SSE connection opens', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(new ReadableStream<Uint8Array>({ start() {} }), { status: 200 })));
  token.set('secret-token');

  startEventStream();

  await vi.waitFor(() => expect(fetch).toHaveBeenCalledTimes(1));
  expect(refreshDashboardSnapshotMock).not.toHaveBeenCalled();
});

test.each([409, 410])('falls back once to a dashboard snapshot when cursor replay is rejected with %s', async (status) => {
  dashboardStreamCursor.set('stale-cursor');
  const fetchMock = vi.fn(async () => new Response('cursor invalid', { status, statusText: 'Cursor rejected' }));
  vi.stubGlobal('fetch', fetchMock);
  token.set('secret-token');

  startEventStream();

  await vi.waitFor(() => expect(refreshDashboardSnapshotMock).toHaveBeenCalledWith({ reason: 'sse_fallback' }));
  expect(refreshDashboardSnapshotMock).toHaveBeenCalledTimes(1);
  expect(get(dashboardStreamCursor)).toBeNull();
});

test.each([
  ['server failure', async () => new Response('unavailable', { status: 503, statusText: 'Unavailable' })],
  ['network failure', async () => { throw new TypeError('Failed to fetch'); }],
])('retains the cursor without a snapshot fallback after %s', async (_name, fetchImplementation) => {
  dashboardStreamCursor.set('session:4;task:9');
  vi.stubGlobal('fetch', vi.fn(fetchImplementation));
  token.set('secret-token');

  startEventStream();

  await vi.waitFor(() => expect(get(lastConnectionError)).not.toBeNull());
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(get(dashboardStreamCursor)).toBe('session:4;task:9');
  expect(refreshDashboardSnapshotMock).not.toHaveBeenCalled();
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
