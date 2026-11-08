import { QueryObserver, queryOptions } from "@tanstack/svelte-query";

import { readable } from "svelte/store";

import type { TurnView } from "../api/types";

import type { SessionChatMessage } from "$lib/session-chat/sessionChat";
import type { DeviceRuntime } from "../device/runtime";

export function createOptimisticChat(device: DeviceRuntime) {
  function optimisticChatOptions() {
    return queryOptions({
      queryKey: device.sessionQueries.sessionKeys.optimisticChat(),
      enabled: false,
      queryFn: async (): Promise<Record<string, SessionChatMessage[]>> => ({}),
    });
  }

  function createOptimisticChatQuery() {
    const observer = new QueryObserver(device.queryClient, optimisticChatOptions());
    return readable(observer.getCurrentResult(), (set) => {
      set(observer.getCurrentResult());
      const unsubscribe = observer.subscribe(set);
      return () => {
        unsubscribe();
        observer.destroy();
      };
    });
  }

  function optimisticChatMessages(sessionId: string): SessionChatMessage[] {
    return (
      device.queryClient.getQueryData<Record<string, SessionChatMessage[]>>(
        device.sessionQueries.sessionKeys.optimisticChat(),
      )?.[sessionId] ?? []
    );
  }

  function setOptimisticChatMessages(
    sessionId: string,
    update: (messages: SessionChatMessage[]) => SessionChatMessage[],
  ): void {
    device.queryClient.setQueryData<Record<string, SessionChatMessage[]>>(
      device.sessionQueries.sessionKeys.optimisticChat(),
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

  function rememberOptimisticMessage(
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

  function discardOptimisticMessage(sessionId: string, messageId: string): void {
    setOptimisticChatMessages(sessionId, (messages) =>
      messages.filter((message) => message.id !== messageId),
    );
  }

  function reconcileOptimisticMessages(
    sessionId: string,
    loadedMessages: SessionChatMessage[],
  ): void {
    setOptimisticChatMessages(sessionId, (messages) => {
      const matchedIds = matchedOptimisticMessageIds(messages, loadedMessages);
      return matchedIds.size ? messages.filter((message) => !matchedIds.has(message.id)) : messages;
    });
  }

  function chatMessagesWithOptimistic(
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
  return {
    createOptimisticChatQuery,
    optimisticChatMessages,
    rememberOptimisticMessage,
    discardOptimisticMessage,
    reconcileOptimisticMessages,
    chatMessagesWithOptimistic,
  };
}
