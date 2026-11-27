import {
  MutationObserver,
  createMutation,
  type MutationObserverOptions,
} from "@tanstack/svelte-query";

import type { MessageDelivery, QueuedMessageView, SendMessageInput } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

interface SendMessageVariables {
  sessionId: string;
  input: SendMessageInput;
}

interface QueuedMessageVariables {
  sessionId: string;
  messageId: string;
}

export function createQueueQueries(device: DeviceRuntime) {
  async function invalidateAfterMutation(sessionId: string): Promise<void> {
    await Promise.all([
      device.sessionOverview.invalidateSessionOverview(),
      device.sessionQueries.invalidateSessionDetail(sessionId),
    ]);
  }

  const sendMessageMutationOptions = () => ({
    mutationFn: ({ sessionId, input }: SendMessageVariables) =>
      device.api.sendMessage(sessionId, input),
    onSuccess: async (delivery: MessageDelivery, { sessionId, input }: SendMessageVariables) => {
      if (delivery.status === "delivered") {
        device.optimisticChat.rememberOptimisticMessage(sessionId, input.input);
      }
      await invalidateAfterMutation(sessionId);
    },
  });

  const cancelQueuedMessageMutationOptions = () => ({
    mutationFn: ({ sessionId, messageId }: QueuedMessageVariables) =>
      device.api.cancelQueuedMessage(sessionId, messageId),
    onSuccess: async (message: QueuedMessageView) => {
      device.queryClient.setQueryData<QueuedMessageView[]>(
        device.sessionQueries.sessionKeys.queue(message.session_id),
        (messages) => (messages ?? []).filter((item) => item.message_id !== message.message_id),
      );
      await invalidateAfterMutation(message.session_id);
    },
  });

  function createSendMessageMutation() {
    return createMutation(sendMessageMutationOptions, () => device.queryClient);
  }

  function createCancelQueuedMessageMutation() {
    return createMutation(cancelQueuedMessageMutationOptions, () => device.queryClient);
  }

  async function executeMutation<TData, TVariables>(
    options: MutationObserverOptions<TData, Error, TVariables>,
    variables: TVariables,
  ): Promise<TData> {
    const observer = new MutationObserver(device.queryClient, options);
    try {
      return await observer.mutate(variables);
    } finally {
      observer.reset();
    }
  }

  function mutateSendMessage(sessionId: string, input: SendMessageInput): Promise<MessageDelivery> {
    return executeMutation(sendMessageMutationOptions(), { sessionId, input });
  }

  function mutateCancelQueuedMessage(
    sessionId: string,
    messageId: string,
  ): Promise<QueuedMessageView> {
    return executeMutation(cancelQueuedMessageMutationOptions(), { sessionId, messageId });
  }

  return {
    createSendMessageMutation,
    createCancelQueuedMessageMutation,
    mutateSendMessage,
    mutateCancelQueuedMessage,
  };
}
