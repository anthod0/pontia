import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { get } from 'svelte/store';
import { submitInboxMessage } from '../src/stores/sessions';
import { token } from '../src/stores/auth';
import { unconfirmedSubmissions } from '../src/stores/inboxRecovery';

beforeEach(() => {
  localStorage.clear();
  unconfirmedSubmissions.set([]);
  token.set('test-token');
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

test.each([401, 429])('a lost accepted response followed by HTTP %i retains the original submission identity', async (status) => {
  vi.useFakeTimers();
  const accepted = new Set<string>();
  const fetch = vi.fn(async (url: string) => {
    if (!accepted.size) {
      accepted.add(url);
      throw new TypeError('Response lost after server acceptance');
    }
    return new Response(JSON.stringify({ data: null, error: { code: 'request_rejected', message: 'Rejected later attempt' } }), { status });
  });
  vi.stubGlobal('fetch', fetch);
  const outcome = submitInboxMessage('session', { input: 'execute once' }).catch((error: Error) => error);
  await vi.advanceTimersByTimeAsync(300);
  expect((await outcome as Error).message).toContain('receipt unknown');
  expect(fetch).toHaveBeenCalledTimes(2);
  expect(fetch.mock.calls[1][0]).toBe(fetch.mock.calls[0][0]);
  const saved = get(unconfirmedSubmissions);
  expect(saved).toHaveLength(1);
  expect(fetch.mock.calls[0][0]).toContain(saved[0].messageId);
  expect(accepted.size).toBe(1);
});
