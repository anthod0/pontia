import { afterEach, expect, test, vi } from 'vitest';
import { getSession, getWorkflow, getWorkflowDocument, listWorkflowPatches, retryWorkflow } from '../src/api/client';

function response(data: unknown) { return new Response(JSON.stringify({ data }), { status: 200 }); }
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); vi.useRealTimers(); });

test('workflow Retry sends the observed failure identity as a JSON object', async () => {
  const fetch = vi.fn().mockResolvedValue(response({ workflow: { state: 'recovering' } }));
  vi.stubGlobal('fetch', fetch);
  expect(await retryWorkflow('wf/1', 'failure-1')).toEqual({ state: 'recovering' });
  const [url, init] = fetch.mock.calls[0];
  expect(url).toBe('/api/v1/workflows/wf%2F1/retry');
  expect(init.method).toBe('POST');
  expect(JSON.parse(init.body)).toEqual({ failure_event_id: 'failure-1' });
});

test('reads patch history, Session and documents only through encoded External API routes', async () => {
  const fetch = vi.fn().mockResolvedValueOnce(response({ patches: [] })).mockResolvedValueOnce(response({ document: { content: 'Reason' } })).mockResolvedValueOnce(response({ session: { state: 'idle' } }));
  vi.stubGlobal('fetch', fetch);
  expect(await listWorkflowPatches('wf/1')).toEqual([]);
  expect(await getWorkflowDocument('wf/1', 'requests/a & b.md')).toEqual({ content: 'Reason' });
  expect(await getSession('session/1')).toEqual({ state: 'idle' });
  expect(fetch.mock.calls.map(call => call[0])).toEqual([
    '/api/v1/workflows/wf%2F1/patches',
    '/api/v1/workflows/wf%2F1/documents?ref=requests%2Fa+%26+b.md',
    '/api/v1/sessions/session%2F1',
  ]);
});

test.each(['history', 'session', 'workflow', 'document'])('bounds stalled %s reads and allows a subsequent read', async (kind) => {
  vi.useFakeTimers();
  const timeout = new AbortController();
  const timeoutSpy = vi.spyOn(AbortSignal, 'timeout').mockReturnValue(timeout.signal);
  const fetch = vi.fn((_input, init: RequestInit) => new Promise<Response>((_resolve, reject) => {
    init.signal?.addEventListener('abort', () => reject(init.signal?.reason), { once: true });
  }));
  vi.stubGlobal('fetch', fetch);
  const read = () => kind === 'history' ? listWorkflowPatches('wf') : kind === 'session' ? getSession('s') : kind === 'workflow' ? getWorkflow('wf') : getWorkflowDocument('wf', 'reason.md');
  const pending = read();
  const assertion = expect(pending).rejects.toMatchObject({ name: 'TimeoutError' });
  timeout.abort(new DOMException('Read timed out', 'TimeoutError'));
  await assertion;
  expect(timeoutSpy).toHaveBeenCalledWith(15_000);
  expect(fetch).toHaveBeenCalledTimes(1);
  timeoutSpy.mockReturnValue(new AbortController().signal);
  fetch.mockResolvedValueOnce(response({ patches: [], session: { state: 'idle' }, workflow: {}, document: {} }));
  await expect(read()).resolves.toBeDefined();
});

test('caller cancellation also aborts a bounded read, including a stalled response body', async () => {
  const controller = new AbortController();
  vi.stubGlobal('fetch', vi.fn(async (_input, init: RequestInit) => ({ text: () => new Promise((_resolve, reject) => {
    init.signal?.addEventListener('abort', () => reject(init.signal?.reason), { once: true });
  }) })));
  const pending = getWorkflowDocument('wf', 'reason.md', { signal: controller.signal });
  const assertion = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
  await Promise.resolve(); await Promise.resolve();
  controller.abort();
  await assertion;
});
