import { describe, expect, test, vi } from 'vitest';
import { listWorkspaceFilePickerEntries } from '../src/api/client';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

describe('workspace file picker API client', () => {
  test('searches workspace file picker entries with query and limit', async () => {
    const fetchMock = vi.fn(async (url: string) => {
      expect(url).toBe('/external/v1/workspaces/workspace-1/file-picker?query=src%2Fmain&limit=25');
      return jsonResponse({
        data: {
          files: [{ path: 'src/main.rs', name: 'main.rs' }],
          truncated: false,
          warnings: [],
        },
      });
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await listWorkspaceFilePickerEntries('workspace-1', 'src/main', { limit: 25 });

    expect(result.files).toEqual([{ path: 'src/main.rs', name: 'main.rs' }]);
    expect(result.truncated).toBe(false);
  });
});
