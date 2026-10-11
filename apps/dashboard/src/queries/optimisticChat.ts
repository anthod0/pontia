import { QueryObserver, queryOptions } from "@tanstack/svelte-query";
import { readable } from "svelte/store";
import type { TurnView } from "../api/types";
import type { SessionChatMessage } from "$lib/session-chat/sessionChat";
import { queryClient } from "./queryClient";
import { sessionKeys } from "./sessions";

function optimisticChatOptions() {
  return queryOptions({
    queryKey: sessionKeys.optimisticChat(),
    enabled: false,
    queryFn: async (): Promise<Record<string, SessionChatMessage[]>> => ({}),
  });
}

export function createOptimisticChatQuery() {
  const observer = new QueryObserver(queryClient, optimisticChatOptions());
  return readable(observer.getCurrentResult(), (set) => {
    set(observer.getCurrentResult());
    const unsubscribe = observer.subscribe(set);
    return () => {
      unsubscribe();
      observer.destroy();
    };
  });
}

export function optimisticChatMessages(sessionId: string): SessionChatMessage[] {
  return (
    queryClient.getQueryData<Record<string, SessionChatMessage[]>>(sessionKeys.optimisticChat())?.[
      sessionId
    ] ?? []
  );
}

function setOptimisticChatMessages(
  sessionId: string,
  update: (messages: SessionChatMessage[]) => SessionChatMessage[],
): void {
  queryClient.setQueryData<Record<string, SessionChatMessage[]>>(
    sessionKeys.optimisticChat(),
    (messages) => {
      const current = messages ?? {};
      const nextSession = update(current[sessionId] ?? []);
      if (nextSession === current[sessionId]) return current;
      if (nextSession.length) return { ...current, [sessionId]: nextSession };
      const next = { ...current };
      delete next[sessionId];
      return next;
    },
  );
}

export function rememberOptimisticMessage(
  sessionId: string,
  input: string,
  turn: Pick<TurnView, "turn_id" | "created_at"> | null = null,
): string | null {
  const content = input.trim();
  if (!sessionId || !content) return null;
  const identity = turn?.turn_id ?? `local_${crypto.randomUUID()}`;
  const message: SessionChatMessage = {
    id: `optimistic:${identity}:user`,
    turnId: identity,
    role: "user",
    content,
    status: "pending",
    createdAt: turn?.created_at ?? new Date().toISOString(),
  };
  setOptimisticChatMessages(sessionId, (messages) => [...messages, message].slice(-50));
  return message.id;
}

export function discardOptimisticMessage(sessionId: string, messageId: string): void {
  setOptimisticChatMessages(sessionId, (messages) =>
    messages.filter((message) => message.id !== messageId),
  );
}

export function reconcileOptimisticMessages(
  sessionId: string,
  loadedMessages: SessionChatMessage[],
): void {
  setOptimisticChatMessages(sessionId, (messages) => {
    const matchedIds = matchedOptimisticMessageIds(messages, loadedMessages);
    return matchedIds.size ? messages.filter((message) => !matchedIds.has(message.id)) : messages;
  });
}

export function chatMessagesWithOptimistic(
  loadedMessages: SessionChatMessage[],
  optimisticMessages: SessionChatMessage[],
): SessionChatMessage[] {
  if (!optimisticMessages.length) return loadedMessages;
  const matchedIds = matchedOptimisticMessageIds(optimisticMessages, loadedMessages);
  return [
    ...loadedMessages,
    ...optimisticMessages.filter((message) => !matchedIds.has(message.id)),
  ];
}

function matchedOptimisticMessageIds(
  optimisticMessages: SessionChatMessage[],
  loadedMessages: SessionChatMessage[],
): Set<string> {
  const matchedIds = new Set<string>();
  const matchedLoadedIndexes = new Set<number>();
  for (const optimistic of optimisticMessages) {
    const matchIndex = loadedMessages.findIndex(
      (message, index) =>
        !matchedLoadedIndexes.has(index) &&
        message.role === "user" &&
        (message.turnId === optimistic.turnId ||
          message.content.trim() === optimistic.content.trim()),
    );
    if (matchIndex < 0) continue;
    matchedLoadedIndexes.add(matchIndex);
    matchedIds.add(optimistic.id);
  }
  return matchedIds;
}
