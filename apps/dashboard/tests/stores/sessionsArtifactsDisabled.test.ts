import { get } from 'svelte/store';
import { beforeEach, expect, test, vi } from 'vitest';
import type { SessionView } from '../../src/api/types';

const mocks = vi.hoisted(() => ({
  cancelInboxMessage: vi.fn(),
  createSession: vi.fn(),
  dismissInboxMessage: vi.fn(),
  discoverArtifacts: vi.fn(async () => []),
  getSession: vi.fn(),
  interruptSession: vi.fn(),
  listArtifacts: vi.fn(async () => []),
  listEvents: vi.fn(async () => []),
  listInboxMessages: vi.fn(async () => []),
  listSessions: vi.fn(async () => []),
  listTurns: vi.fn(async () => []),
  restartSession: vi.fn(),
  resumeSession: vi.fn(),
  submitInboxMessage: vi.fn(),
  terminateSession: vi.fn(),
  updateSession: vi.fn(),
}));

vi.mock('../../src/api/client', () => mocks);

const session = (overrides: Partial<SessionView> = {}): SessionView => ({
  session_id: 'session-1',
  client_type: 'pi',
  handle: 'main',
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  state: 'idle',
  current_turn_id: null,
  workspace_id: 'workspace-1',
  workspace: null,
  capabilities: { context_usage: 'unsupported' },
  model: null,
  context_usage: null,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  metadata: {},
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getSession.mockResolvedValue(session());
});

test('loadSessionDetail does not request artifacts while the artifacts UI is disabled', async () => {
  const { loadSessionDetail, sessionDetail } = await import('../../src/stores/sessions');

  await loadSessionDetail('session-1');

  expect(mocks.getSession).toHaveBeenCalledWith('session-1');
  expect(mocks.listTurns).toHaveBeenCalledWith('session-1');
  expect(mocks.listInboxMessages).toHaveBeenCalledWith('session-1');
  expect(mocks.listEvents).toHaveBeenCalledWith('session-1');
  expect(mocks.listArtifacts).not.toHaveBeenCalled();
  expect(get(sessionDetail)?.artifacts).toEqual([]);
});
