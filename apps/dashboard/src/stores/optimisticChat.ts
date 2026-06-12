import { writable } from 'svelte/store';
import type { TurnView } from '../api/types';
import type { SessionChatMessage } from '$lib/session-chat/sessionChat';

export const optimisticInitialMessages = writable<Record<string, SessionChatMessage>>({});

export function rememberOptimisticInitialMessage(
  sessionId: string,
  input: string,
  turn: Pick<TurnView, 'turn_id' | 'created_at'> | null = null,
): void {
  const content = input.trim();
  if (!sessionId || !content) return;
  const turnId = turn?.turn_id ?? `${sessionId}:initial`;
  optimisticInitialMessages.update((messages) => ({
    ...messages,
    [sessionId]: {
      id: `optimistic:${turnId}:user`,
      turnId,
      role: 'user',
      content,
      status: 'sent',
      createdAt: turn?.created_at ?? new Date().toISOString(),
    },
  }));
}

export function chatMessagesWithOptimistic(
  sessionId: string,
  loadedMessages: SessionChatMessage[],
  messagesBySessionId: Record<string, SessionChatMessage>,
): SessionChatMessage[] {
  const optimistic = messagesBySessionId[sessionId];
  if (!optimistic) return loadedMessages;
  const alreadyLoaded = loadedMessages.some((message) => (
    message.role === 'user'
    && (message.turnId === optimistic.turnId || message.content.trim() === optimistic.content.trim())
  ));
  return alreadyLoaded ? loadedMessages : [optimistic, ...loadedMessages];
}
