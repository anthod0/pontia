import { describe, expect, test, vi, beforeEach } from 'vitest';
import type { CreateSessionResult, SessionView } from '../../src/api/types';

const session: SessionView = {
  session_id: 'session-fast',
  client_type: 'pi',
  title: 'Fast session',
  handle: null,
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  workspace: null,
  workspace_id: null,
  workspace_ref: null,
  state: 'starting',
  current_turn_id: null,
  state_version: 1,
  metadata: {},
  capabilities: {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: true,
    heartbeat: false,
  },
  created_at: '2026-06-22T00:00:00.000Z',
  updated_at: '2026-06-22T00:00:00.000Z',
};

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
}));

vi.mock('../../src/api/client', () => api);

describe('sessions store createSession', () => {
  beforeEach(() => {
    vi.resetModules();
    Object.values(api).forEach((mock) => mock.mockReset());
  });

  test('returns the created session without waiting for follow-up session detail refreshes', async () => {
    api.createSession.mockResolvedValue({ session, initial_turn: null } satisfies CreateSessionResult);
    api.listSessions.mockImplementation(() => new Promise(() => {}));
    api.getSession.mockImplementation(() => new Promise(() => {}));

    const { createSession, sessions, sessionDetail } = await import('../../src/stores/sessions');
    const result = await createSession({ client_type: 'pi', workspace_id: 'workspace-1' });

    expect(result.session.session_id).toBe('session-fast');
    expect(api.listSessions).not.toHaveBeenCalled();
    expect(api.getSession).not.toHaveBeenCalled();

    let sessionsValue: SessionView[] = [];
    const unsubscribeSessions = sessions.subscribe((value) => { sessionsValue = value; });
    unsubscribeSessions();
    expect(sessionsValue.map((item) => item.session_id)).toEqual(['session-fast']);

    let detailValue: unknown = undefined;
    const unsubscribeDetail = sessionDetail.subscribe((value) => { detailValue = value; });
    unsubscribeDetail();
    expect(detailValue).toEqual({ session, turns: [], inboxMessages: [], events: [] });
  });
});
