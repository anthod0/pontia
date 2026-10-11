import {
  createMutation,
  createQuery,
  MutationObserver,
  queryOptions,
  type MutationObserverOptions,
} from "@tanstack/svelte-query";
import {
  archiveSession as requestArchiveSession,
  createSession as requestCreateSession,
  getSession,
  interruptSession as requestInterruptSession,
  listEvents,
  listInboxMessages,
  listSessionModels,
  listTurns,
  pinSession as requestPinSession,
  resumeSession as requestResumeSession,
  setSessionModel as requestSetSessionModel,
  terminateSession as requestTerminateSession,
  unpinSession as requestUnpinSession,
  updateSession as requestUpdateSession,
} from "../api/client";
import type {
  CreateSessionInput,
  CreateSessionResult,
  EventView,
  InboxMessageView,
  SessionView,
  TurnView,
  UpdateSessionInput,
} from "../api/types";
import { queryClient } from "./queryClient";
import { invalidateSessionOverview } from "./sessionOverview";
import { rememberOptimisticMessage } from "./optimisticChat";

export interface SessionConsoleDetail {
  session: SessionView;
  turns: TurnView[];
  inboxMessages: InboxMessageView[];
  events: EventView[];
}

export const sessionKeys = {
  all: ["sessions"] as const,
  details: () => [...sessionKeys.all, "detail"] as const,
  detail: (sessionId: string) => [...sessionKeys.details(), sessionId] as const,
  turns: (sessionId: string) => [...sessionKeys.detail(sessionId), "turns"] as const,
  inboxMessages: (sessionId: string) =>
    [...sessionKeys.detail(sessionId), "inbox-messages"] as const,
  inboxMessage: (sessionId: string, messageId: string) =>
    [...sessionKeys.inboxMessages(sessionId), messageId] as const,
  optimisticInbox: () => [...sessionKeys.all, "optimistic-inbox"] as const,
  optimisticChat: () => [...sessionKeys.all, "optimistic-chat"] as const,
  events: (sessionId: string) => [...sessionKeys.detail(sessionId), "events"] as const,
  models: (sessionId: string) => [...sessionKeys.detail(sessionId), "models"] as const,
};

export function sessionOptions(sessionId: string, enabled = true) {
  return queryOptions({
    queryKey: sessionKeys.detail(sessionId),
    enabled: enabled && sessionId.length > 0,
    queryFn: ({ signal }) => getSession(sessionId, { signal }),
    refetchOnMount: false,
    retryOnMount: false,
  });
}

export function sessionTurnsOptions(sessionId: string, enabled = true) {
  return queryOptions({
    queryKey: sessionKeys.turns(sessionId),
    enabled: enabled && sessionId.length > 0,
    queryFn: ({ signal }) => listTurns(sessionId, { signal }),
    refetchOnMount: false,
    retryOnMount: false,
  });
}

export function sessionInboxMessagesOptions(sessionId: string, enabled = true) {
  return queryOptions({
    queryKey: sessionKeys.inboxMessages(sessionId),
    enabled: enabled && sessionId.length > 0,
    queryFn: ({ signal }) => listInboxMessages(sessionId, { signal }),
    refetchOnMount: false,
    retryOnMount: false,
  });
}

export function sessionEventsOptions(sessionId: string, enabled = true) {
  return queryOptions({
    queryKey: sessionKeys.events(sessionId),
    enabled: enabled && sessionId.length > 0,
    queryFn: ({ signal }) => listEvents(sessionId, { signal }),
    refetchOnMount: false,
    retryOnMount: false,
  });
}

function sessionModelsOptions(sessionId: string) {
  return queryOptions({
    queryKey: sessionKeys.models(sessionId),
    enabled: sessionId.length > 0,
    queryFn: ({ signal }) => listSessionModels(sessionId, { signal }),
  });
}

export function createSessionQuery(sessionId: () => string, enabled: () => boolean = () => true) {
  return createQuery(
    () => sessionOptions(sessionId(), enabled()),
    () => queryClient,
  );
}

export function createSessionTurnsQuery(
  sessionId: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => sessionTurnsOptions(sessionId(), enabled()),
    () => queryClient,
  );
}

export function createSessionInboxMessagesQuery(
  sessionId: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => sessionInboxMessagesOptions(sessionId(), enabled()),
    () => queryClient,
  );
}

export function createSessionEventsQuery(
  sessionId: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => sessionEventsOptions(sessionId(), enabled()),
    () => queryClient,
  );
}

export function createSessionModelsQuery(sessionId: () => string) {
  return createQuery(
    () => sessionModelsOptions(sessionId()),
    () => queryClient,
  );
}

export async function fetchSessionDetail(sessionId: string): Promise<SessionConsoleDetail> {
  const [session, turns, inboxMessages, events] = await Promise.all([
    queryClient.fetchQuery(sessionOptions(sessionId)),
    queryClient.fetchQuery(sessionTurnsOptions(sessionId)),
    queryClient.fetchQuery(sessionInboxMessagesOptions(sessionId)),
    queryClient.fetchQuery(sessionEventsOptions(sessionId)),
  ]);
  return { session, turns, inboxMessages, events };
}

export function invalidateSessionDetail(sessionId: string): Promise<void> {
  return queryClient.invalidateQueries({ queryKey: sessionKeys.detail(sessionId) });
}

export function clearSessionQueries(): void {
  void queryClient.cancelQueries({ queryKey: sessionKeys.details() });
  queryClient.removeQueries({ queryKey: sessionKeys.details() });
  queryClient.removeQueries({ queryKey: sessionKeys.optimisticInbox() });
  queryClient.removeQueries({ queryKey: sessionKeys.optimisticChat() });
}

export function setSessionDetail(detail: SessionConsoleDetail): void {
  const sessionId = detail.session.session_id;
  queryClient.setQueryData(sessionKeys.detail(sessionId), detail.session);
  queryClient.setQueryData(sessionKeys.turns(sessionId), detail.turns);
  queryClient.setQueryData(sessionKeys.inboxMessages(sessionId), detail.inboxMessages);
  queryClient.setQueryData(sessionKeys.events(sessionId), detail.events);
}

export function setSessionInboxMessages(
  sessionId: string,
  update: (messages: InboxMessageView[]) => InboxMessageView[],
): void {
  queryClient.setQueryData<InboxMessageView[]>(sessionKeys.inboxMessages(sessionId), (messages) =>
    update(messages ?? []),
  );
}

async function applySessionResult(session: SessionView): Promise<void> {
  queryClient.setQueryData(sessionKeys.detail(session.session_id), session);
  await invalidateSessionOverview();
}

async function invalidateAfterControl(sessionId: string): Promise<void> {
  await Promise.all([invalidateSessionOverview(), invalidateSessionDetail(sessionId)]);
}

const createSessionMutationOptions = () => ({
  mutationFn: requestCreateSession,
  onSuccess: async (result: CreateSessionResult, input: CreateSessionInput) => {
    setSessionDetail({
      session: result.session,
      turns: result.initial_turn ? [result.initial_turn] : [],
      inboxMessages: [],
      events: [],
    });
    if (input.initial_task?.input) {
      rememberOptimisticMessage(
        result.session.session_id,
        input.initial_task.input,
        result.initial_turn,
      );
    }
    await invalidateSessionOverview();
  },
});

const updateSessionMutationOptions = () => ({
  mutationFn: ({ sessionId, input }: { sessionId: string; input: UpdateSessionInput }) =>
    requestUpdateSession(sessionId, input),
  onSuccess: applySessionResult,
});

const pinSessionMutationOptions = () => ({
  mutationFn: requestPinSession,
  onSuccess: applySessionResult,
});

const unpinSessionMutationOptions = () => ({
  mutationFn: requestUnpinSession,
  onSuccess: applySessionResult,
});

const archiveSessionMutationOptions = () => ({
  mutationFn: requestArchiveSession,
  onSuccess: applySessionResult,
});

const interruptSessionMutationOptions = () => ({
  mutationFn: requestInterruptSession,
  onSuccess: async (_result: unknown, sessionId: string) => invalidateAfterControl(sessionId),
});

const resumeSessionMutationOptions = () => ({
  mutationFn: requestResumeSession,
  onSuccess: async (_result: unknown, sessionId: string) => invalidateAfterControl(sessionId),
});

const terminateSessionMutationOptions = () => ({
  mutationFn: requestTerminateSession,
  onSuccess: async (_result: unknown, sessionId: string) => invalidateAfterControl(sessionId),
});

const setSessionModelMutationOptions = () => ({
  mutationFn: ({
    sessionId,
    model,
    runtimeId,
  }: {
    sessionId: string;
    model: string;
    runtimeId: string | null;
  }) => requestSetSessionModel(sessionId, model, runtimeId),
  onSuccess: async (_result: void, { sessionId }: { sessionId: string }) =>
    invalidateSessionDetail(sessionId),
});

export function createCreateSessionMutation() {
  return createMutation(createSessionMutationOptions, () => queryClient);
}

export function createUpdateSessionMutation() {
  return createMutation(updateSessionMutationOptions, () => queryClient);
}

export function createPinSessionMutation() {
  return createMutation(pinSessionMutationOptions, () => queryClient);
}

export function createUnpinSessionMutation() {
  return createMutation(unpinSessionMutationOptions, () => queryClient);
}

export function createArchiveSessionMutation() {
  return createMutation(archiveSessionMutationOptions, () => queryClient);
}

export function createInterruptSessionMutation() {
  return createMutation(interruptSessionMutationOptions, () => queryClient);
}

export function createResumeSessionMutation() {
  return createMutation(resumeSessionMutationOptions, () => queryClient);
}

export function createTerminateSessionMutation() {
  return createMutation(terminateSessionMutationOptions, () => queryClient);
}

export function createSetSessionModelMutation() {
  return createMutation(setSessionModelMutationOptions, () => queryClient);
}

async function executeMutation<TData, TVariables>(
  options: MutationObserverOptions<TData, Error, TVariables>,
  variables: TVariables,
): Promise<TData> {
  const observer = new MutationObserver(queryClient, options);
  try {
    return await observer.mutate(variables);
  } finally {
    observer.reset();
  }
}

export function mutateCreateSession(input: CreateSessionInput): Promise<CreateSessionResult> {
  return executeMutation(createSessionMutationOptions(), input);
}

export function mutateUpdateSession(
  sessionId: string,
  input: UpdateSessionInput,
): Promise<SessionView> {
  return executeMutation(updateSessionMutationOptions(), { sessionId, input });
}

export function mutatePinSession(sessionId: string): Promise<SessionView> {
  return executeMutation(pinSessionMutationOptions(), sessionId);
}

export function mutateUnpinSession(sessionId: string): Promise<SessionView> {
  return executeMutation(unpinSessionMutationOptions(), sessionId);
}

export function mutateArchiveSession(sessionId: string): Promise<SessionView> {
  return executeMutation(archiveSessionMutationOptions(), sessionId);
}

export async function mutateInterruptSession(sessionId: string): Promise<void> {
  await executeMutation(interruptSessionMutationOptions(), sessionId);
}

export async function mutateResumeSession(sessionId: string): Promise<void> {
  await executeMutation(resumeSessionMutationOptions(), sessionId);
}

export async function mutateTerminateSession(sessionId: string): Promise<void> {
  await executeMutation(terminateSessionMutationOptions(), sessionId);
}
