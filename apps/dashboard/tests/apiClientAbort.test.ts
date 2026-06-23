import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { listAgentProfiles, listSessions, listTurns, listWorkspaceRootEntries, listWorkspaceRoots, listWorkspaces, refreshWorkspaceGitStatus } from '../src/api/client';

beforeEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  localStorage.clear();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function jsonResponse(data: unknown): Response {
  return new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

test('passes AbortSignal through settings-related read requests', async () => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith('/workspaces')) return jsonResponse({ workspaces: [] });
    if (url.endsWith('/workspace-roots')) return jsonResponse({ roots: [] });
    if (url.includes('/workspace-roots/root-1/entries')) {
      return jsonResponse({ root_id: 'root-1', path: '', canonical_path: '/repo', parent_path: null, entries: [], warnings: [] });
    }
    if (url.endsWith('/agent-profiles')) return jsonResponse({ agent_profiles: [] });
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal('fetch', fetchMock);

  const controller = new AbortController();

  await listWorkspaces({ signal: controller.signal });
  await listWorkspaceRoots({ signal: controller.signal });
  await listWorkspaceRootEntries('root-1', '', { signal: controller.signal });
  await listAgentProfiles(false, { signal: controller.signal });

  expect(fetchMock).toHaveBeenCalledTimes(4);
  for (const [, init] of fetchMock.mock.calls) {
    expect((init as RequestInit).signal).toBe(controller.signal);
  }
});

test('serializes session list limit and pinned inclusion query options', async () => {
  const fetchMock = vi.fn(async () => jsonResponse({ sessions: [] }));
  vi.stubGlobal('fetch', fetchMock);

  await listSessions({ limit: 50, includePinned: true });

  expect(fetchMock).toHaveBeenCalledWith('/external/v1/sessions?limit=50&include_pinned=true', expect.any(Object));
});

test('retries transient fetch failures before surfacing chat data errors', async () => {
  vi.useFakeTimers();
  const fetchMock = vi.fn()
    .mockRejectedValueOnce(new TypeError('Failed to fetch'))
    .mockResolvedValueOnce(jsonResponse({ turns: [] }));
  vi.stubGlobal('fetch', fetchMock);

  const request = listTurns('session-1');
  await vi.advanceTimersByTimeAsync(250);

  await expect(request).resolves.toEqual([]);
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(fetchMock).toHaveBeenLastCalledWith('/external/v1/sessions/session-1/turns', expect.any(Object));
});

test('retries mutating transient fetch failures with the same idempotency key', async () => {
  vi.useFakeTimers();
  const fetchMock = vi.fn()
    .mockRejectedValueOnce(new TypeError('Failed to fetch'))
    .mockResolvedValueOnce(jsonResponse({ git_status: { workspace_id: 'workspace-1', state: 'observed', observed_at: 'now' } }));
  vi.stubGlobal('fetch', fetchMock);
  vi.stubGlobal('crypto', { randomUUID: () => 'idem-1' });

  const request = refreshWorkspaceGitStatus('workspace-1');
  await vi.advanceTimersByTimeAsync(250);

  await expect(request).resolves.toMatchObject({ workspace_id: 'workspace-1', state: 'observed' });
  expect(fetchMock).toHaveBeenCalledTimes(2);
  const firstHeaders = (fetchMock.mock.calls[0][1] as RequestInit).headers as Headers;
  const secondHeaders = (fetchMock.mock.calls[1][1] as RequestInit).headers as Headers;
  expect(firstHeaders.get('Idempotency-Key')).toBe('idem-1');
  expect(secondHeaders.get('Idempotency-Key')).toBe('idem-1');
});

test('does not retry aborted requests', async () => {
  vi.useFakeTimers();
  const abortError = new DOMException('The operation was aborted.', 'AbortError');
  const fetchMock = vi.fn().mockRejectedValueOnce(abortError);
  vi.stubGlobal('fetch', fetchMock);

  const controller = new AbortController();
  await expect(listWorkspaces({ signal: controller.signal })).rejects.toBe(abortError);

  expect(fetchMock).toHaveBeenCalledTimes(1);
});
