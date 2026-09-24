import { beforeEach, expect, test, vi } from 'vitest';
import { get } from 'svelte/store';

const api = vi.hoisted(() => ({
  submitInboxMessage: vi.fn(), getInboxMessage: vi.fn(), retryInboxMessage: vi.fn(),
  listSessions: vi.fn(async () => []), getSession: vi.fn(async () => ({ session_id: 'session' })),
  listTurns: vi.fn(async () => []), listInboxMessages: vi.fn(async () => []), listEvents: vi.fn(async () => []),
}));
vi.mock('../../src/api/client', () => api);

beforeEach(() => {
  localStorage.clear();
  vi.resetModules();
  vi.clearAllMocks();
});

const message = (messageId: string) => ({
  message_id: messageId, session_id: 'session', input: { summary: 'same text' },
  state: 'pending', delivery_policy: 'after_idle', metadata: {}, branch_target_turn_id: null,
});

test('lost receipt survives reload and recovery queries the same identity without executing again', async () => {
  const actions = await import('../../src/stores/sessions');
  api.submitInboxMessage.mockRejectedValue(new TypeError('Failed to fetch'));
  await expect(actions.submitInboxMessage('session', { input: 'same text' })).rejects.toThrow('receipt unknown');
  const originalId = api.submitInboxMessage.mock.calls[0][2];
  vi.resetModules();
  const saved = await import('../../src/stores/inboxRecovery');
  const restored = get(saved.unconfirmedSubmissions)[0];
  expect(restored.messageId).toBe(originalId);
  api.getInboxMessage.mockResolvedValue(message(originalId));
  await (await import('../../src/stores/sessions')).recoverInboxSubmission(restored);
  expect(api.getInboxMessage).toHaveBeenCalledWith('session', originalId);
  expect(api.submitInboxMessage).toHaveBeenCalledTimes(1);
  expect(get(saved.unconfirmedSubmissions)).toEqual([]);
});

test('unaccepted recovery resends its identity while intentionally identical new input gets another', async () => {
  const actions = await import('../../src/stores/sessions');
  const saved = await import('../../src/stores/inboxRecovery');
  const { ApiError } = await import('../../src/api/errors');
  api.submitInboxMessage.mockRejectedValueOnce(new TypeError('response lost'));
  await expect(actions.submitInboxMessage('session', { input: 'same text' })).rejects.toThrow();
  const original = get(saved.unconfirmedSubmissions)[0];
  api.getInboxMessage.mockRejectedValue(new ApiError('absent', 'not_found', 404));
  api.submitInboxMessage.mockImplementation(async (_session, _input, id) => message(id));
  await actions.recoverInboxSubmission(original);
  await actions.submitInboxMessage('session', { input: 'same text' });
  const identities = api.submitInboxMessage.mock.calls.map((call) => call[2]);
  expect(identities[1]).toBe(identities[0]);
  expect(identities[2]).not.toBe(identities[0]);
});

test('unknown retry response preserves the original link and explicit duplicate-execution acknowledgement', async () => {
  const actions = await import('../../src/stores/sessions');
  const saved = await import('../../src/stores/inboxRecovery');
  const { ApiError } = await import('../../src/api/errors');
  api.retryInboxMessage.mockRejectedValueOnce(new TypeError('response lost'));
  await expect(actions.retryInboxMessage('session', message('old') as never, true)).rejects.toThrow();
  const original = get(saved.unconfirmedSubmissions)[0];
  api.getInboxMessage.mockRejectedValue(new ApiError('absent', 'not_found', 404));
  api.retryInboxMessage.mockResolvedValue(message(original.messageId));
  await actions.recoverInboxSubmission(original);
  expect(api.retryInboxMessage.mock.calls).toEqual([
    ['session', 'old', original.messageId, true], ['session', 'old', original.messageId, true],
  ]);
});

test('a refusal during manual recovery cannot erase uncertainty from an earlier attempt', async () => {
  const actions = await import('../../src/stores/sessions');
  const saved = await import('../../src/stores/inboxRecovery');
  const { ApiError } = await import('../../src/api/errors');
  api.submitInboxMessage.mockRejectedValueOnce(new TypeError('response lost'));
  await expect(actions.submitInboxMessage('session', { input: 'same text' })).rejects.toThrow();
  const original = get(saved.unconfirmedSubmissions)[0];
  api.getInboxMessage.mockRejectedValue(new ApiError('absent', 'not_found', 404));
  api.submitInboxMessage.mockRejectedValue(new ApiError('auth changed', 'authentication_failed', 401));
  await expect(actions.recoverInboxSubmission(original)).rejects.toThrow('receipt unknown');
  expect(get(saved.unconfirmedSubmissions)).toEqual([original]);
});
