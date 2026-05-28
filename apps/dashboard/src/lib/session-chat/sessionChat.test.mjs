import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  canSendSessionMessage,
  sessionChatTitle,
  turnsToChatMessages,
  visibleChatSessions,
} from './sessionChat.ts';

const session = (overrides) => ({
  session_id: 'session_alpha123456789',
  client_type: 'pi',
  state: 'idle',
  workspace_id: 'workspace-1',
  workspace: null,
  execution_profile_id: null,
  execution_profile_version: null,
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

const turn = (overrides) => ({
  turn_id: 'turn-1',
  session_id: 'session_alpha123456789',
  state: 'completed',
  input: { summary: 'implement feature' },
  output: { summary: 'feature implemented' },
  failure: null,
  created_at: '2026-01-01T00:00:00Z',
  started_at: '2026-01-01T00:00:01Z',
  completed_at: '2026-01-01T00:00:02Z',
  metadata: {},
  ...overrides,
});

test('uses friendly chat title without exposing raw ids when metadata exists', () => {
  assert.equal(sessionChatTitle(session({ handle: '@assistant', role: 'executor' })), '@assistant · executor');
  assert.equal(sessionChatTitle(session({ handle: null, role: null, description: 'Website polish' })), 'Website polish');
});

test('filters chat sessions to active sessions by default and sorts newest first', () => {
  const visible = visibleChatSessions([
    session({ session_id: 'old-active', state: 'idle', updated_at: '2026-01-01T00:00:00Z' }),
    session({ session_id: 'new-exited', state: 'exited', updated_at: '2026-01-01T00:30:00Z' }),
    session({ session_id: 'new-active', state: 'busy', updated_at: '2026-01-01T00:20:00Z' }),
  ], 'active');

  assert.deepEqual(visible.map((item) => item.session_id), ['new-active', 'old-active']);
});

test('maps each turn into user and assistant chat messages in chronological order', () => {
  const messages = turnsToChatMessages([
    turn({ turn_id: 'turn-2', input: { summary: 'second input' }, output: { summary: 'second output' }, created_at: '2026-01-01T00:10:00Z' }),
    turn({ turn_id: 'turn-1', input: { summary: 'first input' }, output: { summary: 'first output' }, created_at: '2026-01-01T00:00:00Z' }),
  ]);

  assert.deepEqual(messages.map((message) => [message.role, message.content]), [
    ['user', 'first input'],
    ['assistant', 'first output'],
    ['user', 'second input'],
    ['assistant', 'second output'],
  ]);
});

test('renders failed and pending turns as assistant status messages', () => {
  const messages = turnsToChatMessages([
    turn({ turn_id: 'failed', state: 'failed', output: null, failure: { message: 'tool failed' } }),
    turn({ turn_id: 'running', state: 'running', input: { summary: 'still working' }, output: null, failure: null, created_at: '2026-01-01T00:01:00Z' }),
  ]);

  assert.equal(messages[1].status, 'failed');
  assert.match(messages[1].content, /tool failed/);
  assert.equal(messages[3].status, 'pending');
  assert.match(messages[3].content, /Waiting/);
});

test('only allows sending non-empty messages to non-terminal sessions', () => {
  assert.equal(canSendSessionMessage(session({ state: 'idle' }), 'hello'), true);
  assert.equal(canSendSessionMessage(session({ state: 'exited' }), 'hello'), false);
  assert.equal(canSendSessionMessage(session({ state: 'idle' }), '   '), false);
  assert.equal(canSendSessionMessage(null, 'hello'), false);
});
