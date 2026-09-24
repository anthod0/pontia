import { get } from 'svelte/store';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { ApiError } from '../../src/api/errors';
import type { SessionView } from '../../src/api/types';
import * as api from '../../src/api/client';
import {
  loadSessionDetail, loadSessions, selectSession, selectedSessionId,
  sessionDetail, sessionDetailError, sessionDetailErrorKind, sessionDetailLoading, sessions,
} from '../../src/stores/sessions';

const session = (id: string, state = 'idle') => ({ session_id: id, state, capabilities: {} }) as SessionView;

beforeEach(() => {
  vi.spyOn(api, 'getSession').mockImplementation(async (id) => session(id));
  vi.spyOn(api, 'listTurns').mockResolvedValue([]);
  vi.spyOn(api, 'listInboxMessages').mockResolvedValue([]);
  vi.spyOn(api, 'listEvents').mockResolvedValue([]);
  selectSession('a');
});

afterEach(() => {
  selectSession(null);
  vi.restoreAllMocks();
});

test.each([
  [new TypeError('Failed to fetch'), 'network'],
  [new ApiError('Missing session', 'not_found', 404), 'not_found'],
  [new ApiError('Unauthorized', 'unauthorized', 401), 'authentication'],
  [new ApiError('Forbidden', 'forbidden', 403), 'authentication'],
  [new ApiError('Unavailable', 'unavailable', 503), 'request'],
])('retains the route target and classifies %s as %s', async (error, kind) => {
  vi.mocked(api.getSession).mockRejectedValueOnce(error);
  await loadSessionDetail('a');
  expect(get(selectedSessionId)).toBe('a');
  expect(get(sessionDetail)).toBeNull();
  expect(get(sessionDetailErrorKind)).toBe(kind);
  expect(get(sessionDetailLoading)).toBe(false);

  await loadSessionDetail('a', { showLoading: false });
  expect(get(sessionDetail)?.session.session_id).toBe('a');
  expect(get(sessionDetailError)).toBeNull();
});

test('a missing detail subresource does not claim the Session is missing', async () => {
  vi.mocked(api.listTurns).mockRejectedValueOnce(new ApiError('Missing turns', 'not_found', 404));
  await loadSessionDetail('a');
  expect(get(sessionDetailErrorKind)).toBe('request');
});

test('retains an existing snapshot after a background network failure', async () => {
  await loadSessionDetail('a');
  vi.mocked(api.getSession).mockRejectedValueOnce(new TypeError('Failed to fetch'));
  await loadSessionDetail('a', { showLoading: false });
  expect(get(sessionDetail)?.session.session_id).toBe('a');
  expect(get(sessionDetailErrorKind)).toBe('network');
});

test.each(['success', 'failure'])('ignores an old route request %s after switching away and back', async (outcome) => {
  let finish!: (value: SessionView) => void;
  let fail!: (error: Error) => void;
  let signal: AbortSignal | null | undefined;
  vi.mocked(api.getSession).mockImplementationOnce((_id, options) => {
    signal = options?.signal;
    return new Promise((resolve, reject) => { finish = resolve; fail = reject; });
  });
  const oldRequest = loadSessionDetail('a');
  selectSession('b');
  await loadSessionDetail('b');
  selectSession('a');
  vi.mocked(api.getSession).mockResolvedValueOnce(session('a', 'busy'));
  await loadSessionDetail('a');
  if (outcome === 'success') finish(session('a', 'idle'));
  else fail(new TypeError('Failed to fetch'));
  await oldRequest;

  expect(signal?.aborted).toBe(true);
  expect(get(sessionDetail)?.session.state).toBe('busy');
  expect(get(sessionDetailError)).toBeNull();
  expect(get(sessionDetailLoading)).toBe(false);
});

test('coalesces recovery requests but follows an in-flight failure with a fresh read', async () => {
  let fail!: (error: Error) => void;
  vi.mocked(api.getSession).mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject; }));
  const first = loadSessionDetail('a');
  const recovery = loadSessionDetail('a', { showLoading: false });
  const another = loadSessionDetail('a', { showLoading: false });
  fail(new TypeError('Failed to fetch'));
  await Promise.all([first, recovery, another]);
  expect(api.getSession).toHaveBeenCalledTimes(2);
  expect(get(sessionDetail)?.session.session_id).toBe('a');
  expect(get(sessionDetailError)).toBeNull();
});

test('a delayed action refresh for the previous route cannot select it again', async () => {
  selectSession('b');
  await loadSessionDetail('b');
  await loadSessionDetail('a');
  expect(get(selectedSessionId)).toBe('b');
  expect(get(sessionDetail)?.session.session_id).toBe('b');
});

test('an old failed list request cannot erase a recovered sidebar snapshot', async () => {
  let fail!: (error: Error) => void;
  vi.spyOn(api, 'listSessions')
    .mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject; }))
    .mockResolvedValueOnce([session('b')]);
  const old = loadSessions();
  await loadSessions({ showLoading: false });
  fail(new TypeError('Failed to fetch'));
  await old;
  expect(get(sessions).map((item) => item.session_id)).toEqual(['b']);
});
