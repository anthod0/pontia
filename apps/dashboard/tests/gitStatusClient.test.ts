import { describe, expect, test, vi } from 'vitest';
import { getWorkspaceGitStatus, refreshWorkspaceGitStatus } from '../src/api/client';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

describe('workspace git status API client', () => {
  test('reads and refreshes workspace git status through External API', async () => {
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      if (url.endsWith('/workspaces/workspace-1/git-status') && (!init?.method || init.method === 'GET')) {
        return jsonResponse({ data: { git_status: { workspace_id: 'workspace-1', state: 'unknown', observed_at: null } } });
      }
      if (url.endsWith('/workspaces/workspace-1/git-status/refresh') && init?.method === 'POST') {
        return jsonResponse({ data: { git_status: { workspace_id: 'workspace-1', state: 'observed', branch: 'main', clean: true, observed_at: 'now' } } });
      }
      throw new Error(`unexpected request ${url}`);
    });
    vi.stubGlobal('fetch', fetchMock);

    expect(await getWorkspaceGitStatus('workspace-1')).toMatchObject({ state: 'unknown' });
    expect(await refreshWorkspaceGitStatus('workspace-1')).toMatchObject({ branch: 'main', clean: true });
    expect(fetchMock).toHaveBeenCalledWith('/external/v1/workspaces/workspace-1/git-status', expect.any(Object));
    expect(fetchMock).toHaveBeenCalledWith('/external/v1/workspaces/workspace-1/git-status/refresh', expect.objectContaining({ method: 'POST' }));
  });
});
