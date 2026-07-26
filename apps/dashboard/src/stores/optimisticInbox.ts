import { writable } from 'svelte/store';
import type { InboxMessageView, SubmitInboxMessageInput } from '../api/types';
import type { SessionChatMessage } from '$lib/session-chat/sessionChat';

export interface OptimisticInboxSubmission {
  localId: string;
  sessionId: string;
  input: string;
  deliveryPolicy: string | undefined;
  metadata: SubmitInboxMessageInput['metadata'];
  branchTargetTurnId: string | null;
  submittedAt: string;
  acceptedMessage: InboxMessageView | null;
}

export const optimisticInboxSubmissions = writable<Record<string, OptimisticInboxSubmission[]>>({});

let submissionSequence = 0;
const consumedInboxMessageIds = new Set<string>();
const MAX_CONSUMED_INBOX_MESSAGE_IDS = 500;

export function beginInboxSubmission(sessionId: string, input: SubmitInboxMessageInput): string {
  const localId = `${sessionId}:${++submissionSequence}`;
  const submission: OptimisticInboxSubmission = {
    localId,
    sessionId,
    input: input.input.trim(),
    deliveryPolicy: input.delivery_policy,
    metadata: input.metadata,
    branchTargetTurnId: input.branch_target_turn_id ?? null,
    submittedAt: new Date().toISOString(),
    acceptedMessage: null,
  };
  optimisticInboxSubmissions.update((submissions) => ({
    ...submissions,
    [sessionId]: [...(submissions[sessionId] ?? []), submission].slice(-50),
  }));
  return localId;
}

export function confirmInboxSubmission(localId: string, message: InboxMessageView): void {
  if (consumedInboxMessageIds.has(message.message_id) || message.branch_target_turn_id) {
    failInboxSubmission(localId);
    return;
  }
  optimisticInboxSubmissions.update((submissions) => updateSubmission(submissions, localId, (submission) => ({
    ...submission,
    input: message.input.summary.trim(),
    deliveryPolicy: message.delivery_policy,
    metadata: message.metadata,
    branchTargetTurnId: message.branch_target_turn_id,
    acceptedMessage: message,
  })));
}

export function consumeInboxSubmission(messageId: string, sessionId?: string): void {
  consumedInboxMessageIds.add(messageId);
  if (consumedInboxMessageIds.size > MAX_CONSUMED_INBOX_MESSAGE_IDS) {
    const oldestMessageId = consumedInboxMessageIds.values().next().value;
    if (oldestMessageId) consumedInboxMessageIds.delete(oldestMessageId);
  }
  optimisticInboxSubmissions.update((submissions) => {
    const matchedIds = new Set(Object.values(submissions)
      .flat()
      .filter((submission) => submission.acceptedMessage?.message_id === messageId)
      .map((submission) => submission.localId));
    if (!matchedIds.size && sessionId) {
      const unresolved = (submissions[sessionId] ?? []).find((submission) => !submission.acceptedMessage);
      if (unresolved) matchedIds.add(unresolved.localId);
    }
    return matchedIds.size ? removeSubmissions(submissions, matchedIds) : submissions;
  });
}

export function syncInboxSubmissions(messages: InboxMessageView[]): void {
  const messagesById = new Map(messages.map((message) => [message.message_id, message]));
  optimisticInboxSubmissions.update((submissions) => {
    let next = submissions;
    for (const submission of Object.values(submissions).flat()) {
      const messageId = submission.acceptedMessage?.message_id;
      const latest = messageId ? messagesById.get(messageId) : undefined;
      if (!latest) continue;
      if (latest.state !== 'pending' && latest.state !== 'dispatching') {
        next = removeSubmissions(next, new Set([submission.localId]));
        continue;
      }
      next = updateSubmission(next, submission.localId, (current) => ({ ...current, acceptedMessage: latest }));
    }
    return next;
  });
}

export function failInboxSubmission(localId: string): void {
  optimisticInboxSubmissions.update((submissions) => removeSubmissions(submissions, new Set([localId])));
}

export function reconcileInboxSubmissions(sessionId: string, loadedMessages: SessionChatMessage[]): void {
  // Inspect the value inside update so concurrent submissions are not lost.
  optimisticInboxSubmissions.update((submissions) => {
    const matchedIds = matchedSubmissionIds(submissions[sessionId] ?? [], loadedMessages);
    return matchedIds.size ? removeSubmissions(submissions, matchedIds) : submissions;
  });
}

export function inboxSubmissionMessages(
  sessionId: string,
  loadedMessages: SessionChatMessage[],
  submissionsBySessionId: Record<string, OptimisticInboxSubmission[]>,
): SessionChatMessage[] {
  const submissions = submissionsBySessionId[sessionId] ?? [];
  const matched = matchedSubmissionIds(submissions, loadedMessages);
  const optimisticMessages = submissions
    .filter((submission) => !submission.branchTargetTurnId && !matched.has(submission.localId))
    .map(submissionToChatMessage);
  return [...loadedMessages, ...optimisticMessages];
}

function submissionToChatMessage(submission: OptimisticInboxSubmission): SessionChatMessage {
  const identity = submission.acceptedMessage?.message_id ?? submission.localId;
  return {
    id: `optimistic-inbox:${identity}:user`,
    turnId: submission.acceptedMessage?.turn_id ?? `optimistic-inbox:${identity}`,
    role: 'user',
    content: submission.input,
    status: 'pending',
    createdAt: submission.acceptedMessage?.created_at ?? submission.submittedAt,
  };
}

function matchedSubmissionIds(
  submissions: OptimisticInboxSubmission[],
  loadedMessages: SessionChatMessage[],
): Set<string> {
  const matchedIds = new Set<string>();
  const matchedLoadedIndexes = new Set<number>();
  for (const submission of submissions) {
    const accepted = submission.acceptedMessage;
    if (!accepted || submission.branchTargetTurnId) continue;
    const matchIndex = loadedMessages.findIndex((message, index) => {
      if (matchedLoadedIndexes.has(index) || message.role !== 'user') return false;
      return Boolean(accepted.turn_id && message.turnId === accepted.turn_id);
    });
    if (matchIndex < 0) continue;
    matchedLoadedIndexes.add(matchIndex);
    matchedIds.add(submission.localId);
  }
  return matchedIds;
}

function updateSubmission(
  submissions: Record<string, OptimisticInboxSubmission[]>,
  localId: string,
  update: (submission: OptimisticInboxSubmission) => OptimisticInboxSubmission,
): Record<string, OptimisticInboxSubmission[]> {
  for (const [sessionId, sessionSubmissions] of Object.entries(submissions)) {
    const index = sessionSubmissions.findIndex((submission) => submission.localId === localId);
    if (index < 0) continue;
    const nextSessionSubmissions = sessionSubmissions.slice();
    nextSessionSubmissions[index] = update(nextSessionSubmissions[index]);
    return { ...submissions, [sessionId]: nextSessionSubmissions };
  }
  return submissions;
}

function removeSubmissions(
  submissions: Record<string, OptimisticInboxSubmission[]>,
  localIds: Set<string>,
): Record<string, OptimisticInboxSubmission[]> {
  let changed = false;
  const next = { ...submissions };
  for (const [sessionId, sessionSubmissions] of Object.entries(submissions)) {
    const remaining = sessionSubmissions.filter((submission) => !localIds.has(submission.localId));
    if (remaining.length === sessionSubmissions.length) continue;
    changed = true;
    if (remaining.length) next[sessionId] = remaining;
    else delete next[sessionId];
  }
  return changed ? next : submissions;
}
