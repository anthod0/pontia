import {
  createMutation,
  createQuery,
  MutationObserver,
  QueryObserver,
  queryOptions,
  type MutationObserverOptions,
} from "@tanstack/svelte-query";
import {
  cancelInboxMessage as requestCancelInboxMessage,
  dismissInboxMessage as requestDismissInboxMessage,
  getInboxMessage as requestInboxMessage,
  retryInboxMessage as requestRetryInboxMessage,
  submitInboxMessage as requestSubmitInboxMessage,
} from "../api/client";
import { ApiError } from "../api/errors";
import type { InboxMessageView, SessionView, SubmitInboxMessageInput } from "../api/types";
import type { SessionChatMessage } from "$lib/session-chat/sessionChat";
import { readable } from "svelte/store";
import {
  forgetSubmission,
  rememberSubmission,
  SubmissionUnconfirmedError,
  type UnconfirmedSubmission,
} from "../stores/inboxRecovery";
import { queryClient } from "./queryClient";
import { invalidateSessionOverview } from "./sessionOverview";
import { sessionKeys } from "./sessions";

export interface OptimisticInboxSubmission {
  localId: string;
  sessionId: string;
  input: string;
  deliveryPolicy: string | undefined;
  metadata: SubmitInboxMessageInput["metadata"];
  branchTargetTurnId: string | null;
  showInChat: boolean;
  submittedAt: string;
  acceptedMessage: InboxMessageView | null;
}

interface SubmitInboxVariables {
  submission: UnconfirmedSubmission;
  showInChat?: boolean;
}

interface RetryInboxVariables {
  submission: UnconfirmedSubmission;
}

interface InboxMessageVariables {
  sessionId: string;
  messageId: string;
}

const consumedInboxMessageIds = new Set<string>();
const MAX_CONSUMED_INBOX_MESSAGE_IDS = 500;

export function inboxMessageOptions(sessionId: string, messageId: string) {
  return queryOptions({
    queryKey: sessionKeys.inboxMessage(sessionId, messageId),
    enabled: sessionId.length > 0 && messageId.length > 0,
    queryFn: ({ signal }) => requestInboxMessage(sessionId, messageId, { signal }),
  });
}

function optimisticInboxOptions() {
  return queryOptions({
    queryKey: sessionKeys.optimisticInbox(),
    enabled: false,
    queryFn: async (): Promise<Record<string, OptimisticInboxSubmission[]>> => ({}),
  });
}

export function createInboxMessageQuery(sessionId: () => string, messageId: () => string) {
  return createQuery(
    () => inboxMessageOptions(sessionId(), messageId()),
    () => queryClient,
  );
}

export function createOptimisticInboxQuery() {
  const observer = new QueryObserver(queryClient, optimisticInboxOptions());
  return readable(observer.getCurrentResult(), (set) => {
    set(observer.getCurrentResult());
    const unsubscribe = observer.subscribe(set);
    return () => {
      unsubscribe();
      observer.destroy();
    };
  });
}

export function optimisticInboxSubmissions(sessionId: string): OptimisticInboxSubmission[] {
  return (
    queryClient.getQueryData<Record<string, OptimisticInboxSubmission[]>>(
      sessionKeys.optimisticInbox(),
    )?.[sessionId] ?? []
  );
}

function setOptimisticInboxSubmissions(
  sessionId: string,
  update: (submissions: OptimisticInboxSubmission[]) => OptimisticInboxSubmission[],
): void {
  queryClient.setQueryData<Record<string, OptimisticInboxSubmission[]>>(
    sessionKeys.optimisticInbox(),
    (submissions) => {
      const current = submissions ?? {};
      const nextSession = update(current[sessionId] ?? []);
      if (nextSession === current[sessionId]) return current;
      if (nextSession.length) return { ...current, [sessionId]: nextSession };
      const next = { ...current };
      delete next[sessionId];
      return next;
    },
  );
}

function beginInboxSubmission(submission: UnconfirmedSubmission, showInChat: boolean): string {
  const optimistic: OptimisticInboxSubmission = {
    localId: submission.messageId,
    sessionId: submission.sessionId,
    input: submission.input.input.trim(),
    deliveryPolicy: submission.input.delivery_policy,
    metadata: submission.input.metadata,
    branchTargetTurnId: submission.input.branch_target_turn_id ?? null,
    showInChat,
    submittedAt: new Date().toISOString(),
    acceptedMessage: null,
  };
  setOptimisticInboxSubmissions(submission.sessionId, (submissions) =>
    [...submissions, optimistic].slice(-50),
  );
  return optimistic.localId;
}

export function confirmInboxSubmission(
  sessionId: string,
  localId: string,
  message: InboxMessageView,
): void {
  if (consumedInboxMessageIds.has(message.message_id) || message.branch_target_turn_id) {
    failInboxSubmission(sessionId, localId);
    return;
  }
  setOptimisticInboxSubmissions(sessionId, (submissions) =>
    submissions.map((submission) =>
      submission.localId === localId
        ? {
            ...submission,
            input: message.input.summary.trim(),
            deliveryPolicy: message.delivery_policy,
            metadata: message.metadata,
            branchTargetTurnId: message.branch_target_turn_id,
            acceptedMessage: message,
          }
        : submission,
    ),
  );
}

function failInboxSubmission(sessionId: string, localId: string): void {
  setOptimisticInboxSubmissions(sessionId, (submissions) =>
    submissions.filter((submission) => submission.localId !== localId),
  );
}

export function consumeInboxSubmission(sessionId: string, messageId: string): void {
  consumedInboxMessageIds.add(messageId);
  if (consumedInboxMessageIds.size > MAX_CONSUMED_INBOX_MESSAGE_IDS) {
    const oldestMessageId = consumedInboxMessageIds.values().next().value;
    if (oldestMessageId) consumedInboxMessageIds.delete(oldestMessageId);
  }
  setOptimisticInboxSubmissions(sessionId, (submissions) =>
    submissions.filter(
      (submission) =>
        submission.acceptedMessage?.message_id !== messageId && submission.localId !== messageId,
    ),
  );
}

export function syncInboxSubmissions(sessionId: string, messages: InboxMessageView[]): void {
  const messagesById = new Map(messages.map((message) => [message.message_id, message]));
  setOptimisticInboxSubmissions(sessionId, (submissions) =>
    submissions.flatMap((submission) => {
      const messageId = submission.acceptedMessage?.message_id;
      const latest = messageId ? messagesById.get(messageId) : undefined;
      if (!latest) return [submission];
      if (latest.state !== "pending" && latest.state !== "dispatching") return [];
      return [{ ...submission, acceptedMessage: latest }];
    }),
  );
}

export function reconcileInboxSubmissions(
  sessionId: string,
  loadedMessages: SessionChatMessage[],
): void {
  setOptimisticInboxSubmissions(sessionId, (submissions) => {
    const matchedIds = matchedSubmissionIds(submissions, loadedMessages);
    return matchedIds.size
      ? submissions.filter((submission) => !matchedIds.has(submission.localId))
      : submissions;
  });
}

export function inboxSubmissionMessages(
  loadedMessages: SessionChatMessage[],
  submissions: OptimisticInboxSubmission[],
): SessionChatMessage[] {
  const matched = matchedSubmissionIds(submissions, loadedMessages);
  const optimisticMessages = submissions
    .filter(
      (submission) =>
        submission.showInChat && !submission.branchTargetTurnId && !matched.has(submission.localId),
    )
    .map(submissionToChatMessage);
  return [...loadedMessages, ...optimisticMessages];
}

function submissionToChatMessage(submission: OptimisticInboxSubmission): SessionChatMessage {
  const identity = submission.acceptedMessage?.message_id ?? submission.localId;
  return {
    id: `optimistic-inbox:${identity}:user`,
    turnId: submission.acceptedMessage?.turn_id ?? `optimistic-inbox:${identity}`,
    role: "user",
    content: submission.input,
    status: "pending",
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
    const matchIndex = loadedMessages.findIndex(
      (message, index) =>
        !matchedLoadedIndexes.has(index) &&
        message.role === "user" &&
        Boolean(accepted.turn_id && message.turnId === accepted.turn_id),
    );
    if (matchIndex < 0) continue;
    matchedLoadedIndexes.add(matchIndex);
    matchedIds.add(submission.localId);
  }
  return matchedIds;
}

async function deliverSubmission(
  submission: UnconfirmedSubmission,
  recovering = false,
): Promise<InboxMessageView> {
  let message: InboxMessageView;
  try {
    message = submission.retryOf
      ? await requestRetryInboxMessage(
          submission.sessionId,
          submission.retryOf,
          submission.messageId,
          submission.allowUnknown ?? false,
        )
      : await requestSubmitInboxMessage(
          submission.sessionId,
          submission.input,
          submission.messageId,
        );
  } catch (error) {
    if (
      !recovering &&
      error instanceof ApiError &&
      !error.afterNetworkFailure &&
      [400, 401, 403, 404, 409, 422].includes(error.status) &&
      !["invalid_json", "missing_data"].includes(error.code)
    ) {
      forgetSubmission(submission.messageId);
      throw error;
    }
    throw new SubmissionUnconfirmedError();
  }
  try {
    forgetSubmission(submission.messageId);
  } catch {
    throw new SubmissionUnconfirmedError();
  }
  return message;
}

function setInboxMessage(message: InboxMessageView): void {
  queryClient.setQueryData(
    sessionKeys.inboxMessage(message.session_id, message.message_id),
    message,
  );
  queryClient.setQueryData<InboxMessageView[]>(
    sessionKeys.inboxMessages(message.session_id),
    (messages) => [
      ...(messages ?? []).filter((item) => item.message_id !== message.message_id),
      message,
    ],
  );
}

async function invalidateAfterInboxMutation(sessionId: string): Promise<void> {
  await Promise.all([
    invalidateSessionOverview(),
    queryClient.invalidateQueries({ queryKey: sessionKeys.detail(sessionId) }),
  ]);
}

const submitInboxMutationOptions = () => ({
  mutationFn: ({ submission }: SubmitInboxVariables) => deliverSubmission(submission),
  onMutate: ({ submission, showInChat }: SubmitInboxVariables) => {
    rememberSubmission(submission);
    const session = queryClient.getQueryData<SessionView>(sessionKeys.detail(submission.sessionId));
    const visible =
      showInChat ?? (!submission.input.branch_target_turn_id && session?.state !== "busy");
    return { localId: beginInboxSubmission(submission, visible) };
  },
  onError: (_error: Error, { submission }: SubmitInboxVariables, context?: { localId: string }) => {
    if (context) failInboxSubmission(submission.sessionId, context.localId);
  },
  onSuccess: async (
    message: InboxMessageView,
    { submission }: SubmitInboxVariables,
    context?: { localId: string },
  ) => {
    if (context) confirmInboxSubmission(submission.sessionId, context.localId, message);
    setInboxMessage(message);
    await invalidateAfterInboxMutation(submission.sessionId);
  },
});

const retryInboxMutationOptions = () => ({
  mutationFn: ({ submission }: RetryInboxVariables) => {
    rememberSubmission(submission);
    return deliverSubmission(submission);
  },
  onSuccess: async (message: InboxMessageView, { submission }: RetryInboxVariables) => {
    setInboxMessage(message);
    await invalidateAfterInboxMutation(submission.sessionId);
  },
});

const cancelInboxMutationOptions = () => ({
  mutationFn: ({ sessionId, messageId }: InboxMessageVariables) =>
    requestCancelInboxMessage(sessionId, messageId),
  onSuccess: async (message: InboxMessageView) => {
    setInboxMessage(message);
    await invalidateAfterInboxMutation(message.session_id);
  },
});

const dismissInboxMutationOptions = () => ({
  mutationFn: ({ sessionId, messageId }: InboxMessageVariables) =>
    requestDismissInboxMessage(sessionId, messageId),
  onSuccess: async (message: InboxMessageView) => {
    setInboxMessage(message);
    await invalidateAfterInboxMutation(message.session_id);
  },
});

export function createSubmitInboxMutation() {
  return createMutation(submitInboxMutationOptions, () => queryClient);
}

export function createRetryInboxMutation() {
  return createMutation(retryInboxMutationOptions, () => queryClient);
}

export function createCancelInboxMutation() {
  return createMutation(cancelInboxMutationOptions, () => queryClient);
}

export function createDismissInboxMutation() {
  return createMutation(dismissInboxMutationOptions, () => queryClient);
}

async function executeMutation<TData, TVariables, TContext = unknown>(
  options: MutationObserverOptions<TData, Error, TVariables, TContext>,
  variables: TVariables,
): Promise<TData> {
  const observer = new MutationObserver(queryClient, options);
  try {
    return await observer.mutate(variables);
  } finally {
    observer.reset();
  }
}

export function mutateSubmitInboxMessage(
  sessionId: string,
  input: SubmitInboxMessageInput,
  options: { showInChat?: boolean } = {},
): Promise<InboxMessageView> {
  return executeMutation(submitInboxMutationOptions(), {
    submission: { messageId: `msg_${crypto.randomUUID()}`, sessionId, input },
    showInChat: options.showInChat,
  });
}

export async function mutateRetryInboxMessage(
  sessionId: string,
  original: InboxMessageView,
  allowUnknown = false,
): Promise<void> {
  await executeMutation(retryInboxMutationOptions(), {
    submission: {
      messageId: `msg_${crypto.randomUUID()}`,
      sessionId,
      input: { input: original.input.summary },
      retryOf: original.message_id,
      allowUnknown,
    },
  });
}

export function mutateCancelInboxMessage(
  sessionId: string,
  messageId: string,
): Promise<InboxMessageView> {
  return executeMutation(cancelInboxMutationOptions(), { sessionId, messageId });
}

export function mutateDismissInboxMessage(
  sessionId: string,
  messageId: string,
): Promise<InboxMessageView> {
  return executeMutation(dismissInboxMutationOptions(), { sessionId, messageId });
}

export async function recoverInboxSubmission(submission: UnconfirmedSubmission): Promise<void> {
  try {
    await queryClient.fetchQuery(inboxMessageOptions(submission.sessionId, submission.messageId));
    forgetSubmission(submission.messageId);
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 404) throw error;
    const message = await deliverSubmission(submission, true);
    setInboxMessage(message);
  }
  await invalidateAfterInboxMutation(submission.sessionId);
}
