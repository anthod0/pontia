import { beforeEach, describe, expect, test, vi } from 'vitest';

const api = vi.hoisted(() => ({
  archiveSession: vi.fn(),
  cancelInboxMessage: vi.fn(),
  createSession: vi.fn(),
  dismissInboxMessage: vi.fn(),
  getSession: vi.fn(),
  interruptSession: vi.fn(),
  listEvents: vi.fn(),
  listInboxMessages: vi.fn(),
  listSessions: vi.fn(),
  listTurns: vi.fn(),
  pinSession: vi.fn(),
  restartSession: vi.fn(),
  resumeSession: vi.fn(),
  submitInboxMessage: vi.fn(),
  terminateSession: vi.fn(),
  unpinSession: vi.fn(),
  updateSession: vi.fn(),
}));

vi.mock('../../src/api/client', () => api);

describe('sessions store list options', () => {
  beforeEach(() => {
    vi.resetModules();
    Object.values(api).forEach((mock) => mock.mockReset());
    api.listSessions.mockResolvedValue([]);
  });

  test('loads backend-limited recent sessions and includes pinned sessions beyond the limit by default', async () => {
    const { loadSessions } = await import('../../src/stores/sessions');

    await loadSessions();

    expect(api.listSessions).toHaveBeenCalledWith({ limit: 50, includePinned: true });
  });
});
