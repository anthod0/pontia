import { expect, test } from 'vitest';
import {
  canSendSessionMessage,
  sessionChatTitle,
  timelineItemsToChatMessages,
  turnsToChatMessages,
  visibleChatSessions,
} from '../../../src/lib/session-chat/sessionChat';

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

const timelineItem = (overrides) => ({
  item_id: 'item-1',
  kind: 'assistant',
  raw_kind: null,
  role: 'assistant',
  title: null,
  status: null,
  occurred_at: '2026-01-01T00:00:00Z',
  content_preview: 'preview',
  content_ref: 'ref-1',
  turn_id: null,
  ...overrides,
});

test('uses friendly chat title without exposing raw ids when metadata exists', () => {
  expect(sessionChatTitle(session({ handle: '@assistant', role: 'executor' }))).toBe('@assistant · executor');
  expect(sessionChatTitle(session({ handle: null, role: null, description: 'Website polish' }))).toBe('Website polish');
});

test('filters chat sessions to active sessions by default and sorts newest first', () => {
  const visible = visibleChatSessions([
    session({ session_id: 'old-active', state: 'idle', updated_at: '2026-01-01T00:00:00Z' }),
    session({ session_id: 'new-exited', state: 'exited', updated_at: '2026-01-01T00:30:00Z' }),
    session({ session_id: 'new-active', state: 'busy', updated_at: '2026-01-01T00:20:00Z' }),
  ], 'active');

  expect(visible.map((item) => item.session_id)).toEqual(['new-active', 'old-active']);
});

test('maps each turn into user and assistant chat messages in chronological order', () => {
  const messages = turnsToChatMessages([
    turn({ turn_id: 'turn-2', input: { summary: 'second input' }, output: { summary: 'second output' }, created_at: '2026-01-01T00:10:00Z' }),
    turn({ turn_id: 'turn-1', input: { summary: 'first input' }, output: { summary: 'first output' }, created_at: '2026-01-01T00:00:00Z' }),
  ]);

  expect(messages.map((message) => [message.role, message.content])).toEqual([
    ['user', 'first input'],
    ['assistant', 'first output'],
    ['user', 'second input'],
    ['assistant', 'second output'],
  ]);
});

test('maps timeline items into primary chat messages with assistant thought steps', () => {
  const messages = timelineItemsToChatMessages([
    timelineItem({ item_id: '1', kind: 'user', role: 'user', content_preview: 'Build the feature', occurred_at: '2026-01-01T00:00:00Z' }),
    timelineItem({ item_id: '2', kind: 'thinking', role: 'assistant', content_preview: 'Need to inspect files', occurred_at: '2026-01-01T00:00:01Z' }),
    timelineItem({ item_id: '3', kind: 'tool_call', role: 'tool', title: 'read', content_preview: 'read {"path":"src/app.ts"}', occurred_at: '2026-01-01T00:00:02Z' }),
    timelineItem({ item_id: '4', kind: 'tool_result', role: 'tool', title: 'read', status: 'completed', content_preview: 'file contents', occurred_at: '2026-01-01T00:00:03Z' }),
    timelineItem({ item_id: '5', kind: 'assistant', role: 'assistant', content_preview: 'Done.', occurred_at: '2026-01-01T00:00:04Z' }),
  ]);

  expect(messages.map((message) => [message.role, message.content])).toEqual([
    ['user', 'Build the feature'],
    ['assistant', 'Done.'],
  ]);
  expect(messages[1].thoughtSteps).toEqual([
    { id: '2', kind: 'thinking', title: 'Thinking', status: null, content: 'Need to inspect files', occurredAt: '2026-01-01T00:00:01Z' },
    { id: '3', kind: 'tool_call', title: 'read', status: 'started', content: 'read {"path":"src/app.ts"}', occurredAt: '2026-01-01T00:00:02Z' },
    { id: '4', kind: 'tool_result', title: 'read', status: 'completed', content: 'file contents', occurredAt: '2026-01-01T00:00:03Z' },
  ]);
});

test('renders failed and pending turns as assistant status messages', () => {
  const messages = turnsToChatMessages([
    turn({ turn_id: 'failed', state: 'failed', output: null, failure: { message: 'tool failed' } }),
    turn({ turn_id: 'running', state: 'running', input: { summary: 'still working' }, output: null, failure: null, created_at: '2026-01-01T00:01:00Z' }),
  ]);

  expect(messages[1].status).toBe('failed');
  expect(messages[1].content).toMatch(/tool failed/);
  expect(messages[3].status).toBe('pending');
  expect(messages[3].content).toMatch(/Waiting/);
});

test('allows sending non-empty messages unless the session is missing or errored', () => {
  expect(canSendSessionMessage(session({ state: 'idle' }), 'hello')).toBe(true);
  expect(canSendSessionMessage(session({ state: 'exited' }), 'hello')).toBe(true);
  expect(canSendSessionMessage(session({ state: 'error' }), 'hello')).toBe(false);
  expect(canSendSessionMessage(session({ state: 'idle' }), '   ')).toBe(false);
  expect(canSendSessionMessage(null, 'hello')).toBe(false);
});
