import { writable } from 'svelte/store';
import type { TurnView } from '../api/types';
import type { SessionChatMessage } from '$lib/session-chat/sessionChat';

export const optimisticInitialMessages = writable<Record<string, SessionChatMessage[]>>({});

let optimisticMessageSequence = 0;

export function rememberOptimisticMessage(
  sessionId: string,
  input: string,
  turn: Pick<TurnView, 'turn_id' | 'created_at'> | null = null,
): string | null {
  const content = input.trim();
  if (!sessionId || !content) return null;
  const localId = `${sessionId}:${++optimisticMessageSequence}`;
  const turnId = turn?.turn_id ?? localId;
  const message: SessionChatMessage = {
    id: `optimistic:${localId}:user`,
    turnId,
    role: 'user',
    content,
    status: 'pending',
    createdAt: turn?.created_at ?? new Date().toISOString(),
  };
  optimisticInitialMessages.update((messages) => ({
    ...messages,
    [sessionId]: [...(messages[sessionId] ?? []), message].slice(-50),
  }));
  return message.id;
}

export function rememberOptimisticInitialMessage(
  sessionId: string,
  input: string,
  turn: Pick<TurnView, 'turn_id' | 'created_at'> | null = null,
): void {
  rememberOptimisticMessage(sessionId, input, turn);
}

export function discardOptimisticMessage(sessionId: string, messageId: string): void {
  optimisticInitialMessages.update((messages) => {
    const remaining = (messages[sessionId] ?? []).filter((message) => message.id !== messageId);
    if (remaining.length === (messages[sessionId] ?? []).length) return messages;
    const next = { ...messages };
    if (remaining.length) next[sessionId] = remaining;
    else delete next[sessionId];
    return next;
  });
}

export function reconcileOptimisticMessages(sessionId: string, loadedMessages: SessionChatMessage[]): void {
  optimisticInitialMessages.update((messages) => {
    const optimisticMessages = messages[sessionId] ?? [];
    const matchedIds = matchedOptimisticMessageIds(optimisticMessages, loadedMessages);
    if (!matchedIds.size) return messages;
    const remaining = optimisticMessages.filter((message) => !matchedIds.has(message.id));
    const next = { ...messages };
    if (remaining.length) next[sessionId] = remaining;
    else delete next[sessionId];
    return next;
  });
}

export function chatMessagesWithOptimistic(
  sessionId: string,
  loadedMessages: SessionChatMessage[],
  messagesBySessionId: Record<string, SessionChatMessage[]>,
): SessionChatMessage[] {
  const optimisticMessages = messagesBySessionId[sessionId] ?? [];
  if (!optimisticMessages.length) return loadedMessages;
  const matchedIds = matchedOptimisticMessageIds(optimisticMessages, loadedMessages);
  return [...loadedMessages, ...optimisticMessages.filter((message) => !matchedIds.has(message.id))];
}

function matchedOptimisticMessageIds(
  optimisticMessages: SessionChatMessage[],
  loadedMessages: SessionChatMessage[],
): Set<string> {
  const matchedIds = new Set<string>();
  const matchedLoadedIndexes = new Set<number>();
  for (const optimistic of optimisticMessages) {
    const matchIndex = loadedMessages.findIndex((message, index) => (
      !matchedLoadedIndexes.has(index)
      && message.role === 'user'
      && (message.turnId === optimistic.turnId || message.content.trim() === optimistic.content.trim())
    ));
    if (matchIndex < 0) continue;
    matchedLoadedIndexes.add(matchIndex);
    matchedIds.add(optimistic.id);
  }
  return matchedIds;
}
