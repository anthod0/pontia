import { beforeEach, describe, expect, test, vi } from 'vitest';
import type { SessionView } from '../../src/api/types';

const baseSession: SessionView = {
  session_id: 'session-current',
  client_type: 'pi',
  title: 'Current session',
  handle: null,
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  workspace: null,
  workspace_id: null,
  workspace_ref: null,
  state: 'idle',
  current_turn_id: null,
  state_version: 1,
  metadata: {},
  capabilities: {},
  created_at: '2026-06-22T00:00:00.000Z',
  updated_at: '2026-06-22T00:00:00.000Z',
};

function session(overrides: Partial<SessionView>): SessionView {
  return { ...baseSession, ...overrides };
}

const api = vi.hoisted(() => ({
  createSession: vi.fn(),
  listSessions: vi.fn(),
  getSession: vi.fn(),
  listTurns: vi.fn(),
  listInboxMessages: vi.fn(),
  listEvents: vi.fn(),
  updateSession: vi.fn(),
  submitInboxMessage: vi.fn(),
  cancelInboxMessage: vi.fn(),
  dismissInboxMessage: vi.fn(),
  interruptSession: vi.fn(),
  restartSession: vi.fn(),
  resumeSession: vi.fn(),
  terminateSession: vi.fn(),
  pinSession: vi.fn(),
  unpinSession: vi.fn(),
  archiveSession: vi.fn(),
}));

vi.mock('../../src/api/client', () => api);

describe('sessions store management actions', () => {
  beforeEach(() => {
    vi.resetModules();
    Object.values(api).forEach((mock) => mock.mockReset());
  });

  test('terminating a different session does not replace the current session detail', async () => {
    const current = session({ session_id: 'session-current', title: 'Current session' });
    const otherExited = session({ session_id: 'session-other', title: 'Other session', state: 'exited' });
    api.terminateSession.mockResolvedValue(otherExited);
    api.listSessions.mockResolvedValue([current, otherExited]);
    api.getSession.mockResolvedValue(otherExited);
    api.listTurns.mockResolvedValue([]);
    api.listInboxMessages.mockResolvedValue([]);
    api.listEvents.mockResolvedValue([]);

    const { sessionDetail, terminateSession } = await import('../../src/stores/sessions');
    sessionDetail.set({ session: current, turns: [], inboxMessages: [], events: [] });

    await terminateSession('session-other');

    let detailSessionId: string | null = null;
    const unsubscribe = sessionDetail.subscribe((detail) => { detailSessionId = detail?.session.session_id ?? null; });
    unsubscribe();
    expect(detailSessionId).toBe('session-current');
    expect(api.getSession).not.toHaveBeenCalledWith('session-other');
  });
});
