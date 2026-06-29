import { expect, test } from 'vitest';
import {
  canSendSessionMessage,
  sessionChatTitle,
  titleFromInitialPrompt,
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
  title: null,
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

test('uses explicit session title before identity metadata', () => {
  expect(sessionChatTitle(session({ title: 'Fix dashboard title', handle: '@assistant', role: 'executor' }))).toBe('Fix dashboard title');
});

test('uses friendly chat title without exposing raw ids when metadata exists', () => {
  expect(sessionChatTitle(session({ handle: '@assistant', role: 'executor' }))).toBe('@assistant · executor');
  expect(sessionChatTitle(session({ handle: null, role: null, description: 'Website polish' }))).toBe('Website polish');
});

test('falls back to an untitled client session label when no display metadata exists', () => {
  expect(sessionChatTitle(session({ client_type: 'pi', title: null, handle: null, role: null, description: null }))).toBe('Untitled pi session');
});

test('generates a compact title from the initial prompt', () => {
  expect(titleFromInitialPrompt('  Implement automatic session titles\n\nDetails...')).toBe('Implement automatic session titles');
  expect(titleFromInitialPrompt('```ts\nconst answer = 42\n```')).toBe('const answer = 42');
  expect(titleFromInitialPrompt('')).toBeNull();
});

test('filters chat sessions to active sessions and sorts by creation time so output updates do not reorder them', () => {
  const visible = visibleChatSessions([
    session({ session_id: 'older-created-active', state: 'idle', created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:30:00Z' }),
    session({ session_id: 'newer-created-exited', state: 'exited', created_at: '2026-01-01T00:20:00Z', updated_at: '2026-01-01T00:20:00Z' }),
    session({ session_id: 'newer-created-active', state: 'busy', created_at: '2026-01-01T00:10:00Z', updated_at: '2026-01-01T00:00:00Z' }),
  ], 'active');

  expect(visible.map((item) => item.session_id)).toEqual(['newer-created-active', 'older-created-active']);
});

test('sorts active chat sessions before terminal sessions, then pinned and creation time within each group', () => {
  const visible = visibleChatSessions([
    session({ session_id: 'terminal-pinned', state: 'exited', pinned_at: '2026-01-01T00:40:00Z', created_at: '2026-01-01T00:40:00Z', updated_at: '2026-01-01T00:40:00Z' }),
    session({ session_id: 'active-new-unpinned', state: 'idle', pinned_at: null, created_at: '2026-01-01T00:30:00Z', updated_at: '2026-01-01T00:30:00Z' }),
    session({ session_id: 'active-old-pinned', state: 'idle', pinned_at: '2026-01-01T00:10:00Z', created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' }),
    session({ session_id: 'active-new-pinned', state: 'idle', pinned_at: '2026-01-01T00:20:00Z', created_at: '2026-01-01T00:05:00Z', updated_at: '2026-01-01T00:05:00Z' }),
    session({ session_id: 'terminal-unpinned', state: 'error', pinned_at: null, created_at: '2026-01-01T00:50:00Z', updated_at: '2026-01-01T00:50:00Z' }),
  ], 'all');

  expect(visible.map((item) => item.session_id)).toEqual([
    'active-new-pinned',
    'active-old-pinned',
    'active-new-unpinned',
    'terminal-pinned',
    'terminal-unpinned',
  ]);
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

test('keeps managed tool use data and formats display content from structured inputs', () => {
  const messages = timelineItemsToChatMessages([
    timelineItem({
      item_id: '1',
      kind: 'tool_call',
      role: 'tool',
      title: 'read',
      content_preview: 'read {"path":"src/app.ts","start_line":5}',
      occurred_at: '2026-01-01T00:00:00Z',
      turn_id: 'turn-live',
      managed_tool_use: {
        tool_name: 'read',
        input: { type: 'read', path: 'src/app.ts', start_line: 5 },
      },
    }),
  ]);

  expect(messages[0].thoughtSteps?.[0]).toMatchObject({
    title: 'Read file',
    content: 'src/app.ts:5',
    managedToolUse: {
      tool_name: 'read',
      input: { type: 'read', path: 'src/app.ts', start_line: 5 },
    },
  });
});

test('creates a pending assistant placeholder message for live thought steps before final output', () => {
  const messages = timelineItemsToChatMessages([
    timelineItem({ item_id: '1', kind: 'user', role: 'user', content_preview: 'Build the feature', occurred_at: '2026-01-01T00:00:00Z', turn_id: 'turn-live' }),
    timelineItem({ item_id: '2', kind: 'thinking', role: 'assistant', content_preview: 'Need to inspect files', occurred_at: '2026-01-01T00:00:01Z', turn_id: 'turn-live' }),
    timelineItem({ item_id: '3', kind: 'tool_call', role: 'tool', title: 'read', content_preview: 'read {"path":"src/app.ts"}', occurred_at: '2026-01-01T00:00:02Z', turn_id: 'turn-live' }),
  ]);

  expect(messages.map((message) => [message.role, message.status, message.content])).toEqual([
    ['user', 'sent', 'Build the feature'],
    ['assistant', 'pending', ''],
  ]);
  expect(messages[1].id).toBe('turn-live:working');
  expect(messages[1].thoughtSteps?.map((step) => step.content)).toEqual(['Need to inspect files', 'read {"path":"src/app.ts"}']);
});

test('renders failed turns as messages and pending turns as empty assistant placeholders', () => {
  const messages = turnsToChatMessages([
    turn({ turn_id: 'failed', state: 'failed', output: null, failure: { message: 'tool failed' } }),
    turn({ turn_id: 'running', state: 'running', input: { summary: 'still working' }, output: null, failure: null, created_at: '2026-01-01T00:01:00Z' }),
  ]);

  expect(messages[1].status).toBe('failed');
  expect(messages[1].content).toMatch(/tool failed/);
  expect(messages[3].status).toBe('pending');
  expect(messages[3].content).toBe('');
});

test('allows sending non-empty messages only when the session advertises web-write capability', () => {
  expect(canSendSessionMessage(session({ state: 'idle', capabilities: { accept_task: true } }), 'hello')).toBe(true);
  expect(canSendSessionMessage(session({ state: 'exited', capabilities: { accept_task: true } }), 'hello')).toBe(true);
  expect(canSendSessionMessage(session({ state: 'idle', capabilities: { accept_task: false } }), 'hello')).toBe(false);
  expect(canSendSessionMessage(session({ state: 'idle', capabilities: {} }), 'hello')).toBe(false);
  expect(canSendSessionMessage(session({ state: 'error', capabilities: { accept_task: true } }), 'hello')).toBe(false);
  expect(canSendSessionMessage(session({ state: 'idle', capabilities: { accept_task: true } }), '   ')).toBe(false);
  expect(canSendSessionMessage(null, 'hello')).toBe(false);
});
