import { get } from 'svelte/store';
import { beforeEach, expect, test } from 'vitest';
import type { InboxMessageView } from '../../src/api/types';
import type { SessionChatMessage } from '../../src/lib/session-chat/sessionChat';
import {
  beginInboxSubmission,
  confirmInboxSubmission,
  consumeInboxSubmission,
  failInboxSubmission,
  inboxSubmissionMessages,
  optimisticInboxSubmissions,
  reconcileInboxSubmissions,
} from '../../src/stores/optimisticInbox';

const loadedUserMessage = (content: string): SessionChatMessage => ({
  id: 'turn-1:user',
  turnId: 'turn-1',
  role: 'user',
  content,
  status: 'sent',
  createdAt: '2026-05-14T00:00:00Z',
});

const acceptedInboxMessage = (overrides: Partial<InboxMessageView> = {}): InboxMessageView => ({
  message_id: 'message-1',
  session_id: 'session-1',
  state: 'pending',
  delivery_policy: 'after_idle',
  input: { summary: 'follow up' },
  metadata: { source: 'dashboard_chat' },
  branch_target_turn_id: null,
  turn_id: null,
  superseded_by_message_id: null,
  failure_message: null,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  dispatched_at: null,
  cancelled_at: null,
  ...overrides,
});

beforeEach(() => optimisticInboxSubmissions.set({}));

test('represents an ordinary Inbox submission immediately and upgrades it to the server identity', () => {
  const localId = beginInboxSubmission('session-1', {
    input: ' follow up ',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  });

  expect(inboxSubmissionMessages('session-1', [], get(optimisticInboxSubmissions))).toMatchObject([
    { id: `optimistic-inbox:${localId}:user`, content: 'follow up', status: 'pending' },
  ]);

  confirmInboxSubmission(localId, acceptedInboxMessage());

  expect(inboxSubmissionMessages('session-1', [], get(optimisticInboxSubmissions))).toMatchObject([
    { id: 'optimistic-inbox:message-1:user', content: 'follow up', status: 'pending' },
  ]);
});

test('permanently consumes a submission once its projected user message appears', () => {
  const localId = beginInboxSubmission('session-1', {
    input: 'follow up',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  });
  confirmInboxSubmission(localId, acceptedInboxMessage({ turn_id: 'turn-1' }));

  reconcileInboxSubmissions('session-1', [loadedUserMessage('follow up')]);

  expect(inboxSubmissionMessages('session-1', [], get(optimisticInboxSubmissions))).toEqual([]);
});

test('does not restore a submission when its confirmed Turn arrives before the POST response', () => {
  const localId = beginInboxSubmission('session-1', {
    input: 'follow up',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  });

  consumeInboxSubmission('message-raced', 'session-1');
  expect(inboxSubmissionMessages('session-1', [], get(optimisticInboxSubmissions))).toEqual([]);

  confirmInboxSubmission(localId, acceptedInboxMessage({ message_id: 'message-raced' }));
  expect(inboxSubmissionMessages('session-1', [], get(optimisticInboxSubmissions))).toEqual([]);
});

test('keeps a queued Inbox submission out of chat until a projected Turn exists', () => {
  beginInboxSubmission('session-1', {
    input: 'queued while busy',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  }, { showInChat: false });

  expect(inboxSubmissionMessages('session-1', [], get(optimisticInboxSubmissions))).toEqual([]);
});

test('does not invent a chat Turn for a branch-targeted Inbox submission', () => {
  const localId = beginInboxSubmission('session-1', {
    input: 'replacement',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat_branch_edit' },
    branch_target_turn_id: 'turn-old',
  });

  expect(inboxSubmissionMessages('session-1', [], get(optimisticInboxSubmissions))).toEqual([]);
  confirmInboxSubmission(localId, acceptedInboxMessage({ branch_target_turn_id: 'turn-old' }));
  expect(inboxSubmissionMessages('session-1', [], get(optimisticInboxSubmissions))).toEqual([]);
});

test('rolls back a local submission when the Inbox request fails', () => {
  const localId = beginInboxSubmission('session-1', {
    input: 'follow up',
    delivery_policy: 'after_idle',
    metadata: {},
  });

  failInboxSubmission(localId);

  expect(get(optimisticInboxSubmissions)).toEqual({});
});
