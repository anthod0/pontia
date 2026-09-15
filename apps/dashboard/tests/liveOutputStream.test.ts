import { afterEach, expect, test, vi } from 'vitest';
import { token } from '../src/stores/auth';
import { openLiveOutputStream } from '../src/services/liveOutputStream';

function eventStream(data: unknown): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(controller) {
      controller.enqueue(new TextEncoder().encode(`event: snapshot\ndata: ${JSON.stringify(data)}\n\n`));
      controller.close();
    },
  });
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  token.set('');
});

test('uses a Bearer-authenticated fetch stream and parses live output events', async () => {
  const snapshot = {
    type: 'snapshot',
    session_id: 'session/1',
    turn_id: 'turn-1',
    stream_id: 'stream-1',
    sequence: 1,
    items: [],
  };
  const fetchMock = vi.fn(async () => new Response(eventStream(snapshot), { status: 200 }));
  vi.stubGlobal('fetch', fetchMock);
  token.set('secret');
  const onEvent = vi.fn();

  const close = openLiveOutputStream('session/1', { onEvent, onDisconnected: vi.fn() });

  await vi.waitFor(() => expect(onEvent).toHaveBeenCalledWith(snapshot));
  expect(fetchMock.mock.calls[0][0]).toBe('/external/v1/sessions/session%2F1/live-output/stream');
  expect((fetchMock.mock.calls[0][1]?.headers as Record<string, string>).Authorization).toBe('Bearer secret');
  close();
});

test('cancels the active connection and pending reconnect when closed', async () => {
  vi.useFakeTimers();
  const signals: AbortSignal[] = [];
  const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
    signals.push(init?.signal as AbortSignal);
    return new Response(new ReadableStream({
      start(controller) {
        (init?.signal as AbortSignal).addEventListener('abort', () => controller.error(new DOMException('Aborted', 'AbortError')));
      },
    }), { status: 200 });
  });
  vi.stubGlobal('fetch', fetchMock);
  token.set('secret');

  const close = openLiveOutputStream('session-1', { onEvent: vi.fn(), onDisconnected: vi.fn() });
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
  close();
  await vi.advanceTimersByTimeAsync(5_000);

  expect(signals[0].aborted).toBe(true);
  expect(fetchMock).toHaveBeenCalledTimes(1);
});
