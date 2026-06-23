import { afterEach, expect, test, vi } from 'vitest';
import { waitFor } from '@testing-library/svelte';
import { token } from '../src/stores/auth';
import { startEventStream, stopEventStream, subscribeDashboardEvents } from '../src/services/eventStream';
import type { DashboardStreamEvent } from '../src/api/types';

afterEach(() => {
  stopEventStream();
  vi.unstubAllGlobals();
  token.set('');
});

test('clears the saved token when the SSE endpoint is unauthorized', async () => {
  token.set('stale-token');
  vi.stubGlobal('fetch', vi.fn(async () => new Response('unauthorized', { status: 401, statusText: 'Unauthorized' })));

  startEventStream();

  await waitFor(() => expect(localStorage.getItem('pontia.externalApiToken')).toBe(''));
});

test('notifies dashboard event subscribers when an SSE task event arrives', async () => {
  const event: DashboardStreamEvent = {
    kind: 'task_event',
    id: 'row-1',
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: 'evt-1',
      task_id: 'task-new',
      event_type: 'dag.approved',
      payload: { proposal_id: 'proposal-1' },
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
