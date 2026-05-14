import assert from 'node:assert/strict';
import { test } from 'node:test';
import { sessionDisplayTitle, visibleSessionsForFilter } from './sessionList.ts';

const session = (overrides) => ({
  session_id: 'session-active-old',
  client_type: 'pi',
  state: 'idle',
  workspace_id: 'workspace-1',
  workspace: null,
  execution_profile_id: null,
  handle: null,
  role: null,
  description: null,
  current_turn_id: null,
  capabilities: {},
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
  metadata: {},
  ...overrides,
});

test('uses handle and role as the primary session display title', () => {
  assert.equal(
    sessionDisplayTitle(session({ handle: '@planner', role: 'execution reviewer', session_id: 'sess_abcdef123456' })),
    '@planner · execution reviewer',
  );
});

test('falls back to handle, role, then short session id in display title', () => {
  assert.equal(sessionDisplayTitle(session({ handle: '@planner', role: null })), '@planner');
  assert.equal(sessionDisplayTitle(session({ session_id: 'active-old', handle: null, role: 'reviewer' })), 'reviewer · active-old');
  assert.equal(sessionDisplayTitle(session({ session_id: 'active-old', handle: null, role: null })), 'active-old');
});

test('shows active sessions by default and sorts newest first', () => {
  const visible = visibleSessionsForFilter([
    session({ session_id: 'exited-new', state: 'exited', updated_at: '2026-01-01T00:30:00Z' }),
    session({ session_id: 'active-old', state: 'idle', updated_at: '2026-01-01T00:00:00Z' }),
    session({ session_id: 'active-new', state: 'running', updated_at: '2026-01-01T00:20:00Z' }),
  ], 'active');

  assert.deepEqual(visible.map((item) => item.session_id), ['active-new', 'active-old']);
});

test('keeps exited and error sessions after active sessions in all view', () => {
  const visible = visibleSessionsForFilter([
    session({ session_id: 'error-newest', state: 'error', updated_at: '2026-01-01T00:40:00Z' }),
    session({ session_id: 'active-old', state: 'idle', updated_at: '2026-01-01T00:00:00Z' }),
    session({ session_id: 'exited-middle', state: 'exited', updated_at: '2026-01-01T00:20:00Z' }),
    session({ session_id: 'active-new', state: 'running', updated_at: '2026-01-01T00:10:00Z' }),
  ], 'all');

  assert.deepEqual(visible.map((item) => item.session_id), ['active-new', 'active-old', 'error-newest', 'exited-middle']);
});

test('shows only terminal sessions in exited view sorted newest first', () => {
  const visible = visibleSessionsForFilter([
    session({ session_id: 'active-new', state: 'running', updated_at: '2026-01-01T00:30:00Z' }),
    session({ session_id: 'exited-old', state: 'exited', updated_at: '2026-01-01T00:00:00Z' }),
    session({ session_id: 'error-new', state: 'error', updated_at: '2026-01-01T00:20:00Z' }),
  ], 'exited');

  assert.deepEqual(visible.map((item) => item.session_id), ['error-new', 'exited-old']);
});
