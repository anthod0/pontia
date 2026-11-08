import {
  createMutation,
  createQuery,
  MutationObserver,
  queryOptions,
  type MutationObserverOptions,
} from "@tanstack/svelte-query";

import type {
  CreateSessionInput,
  CreateSessionResult,
  EventView,
  InboxMessageView,
  SessionView,
  TurnView,
  UpdateSessionInput,
} from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export interface SessionConsoleDetail {
  session: SessionView;
  turns: TurnView[];
  inboxMessages: InboxMessageView[];
  events: EventView[];
}
export function createSessionQueries(device: DeviceRuntime) {
  const sessionKeys = {
    all: [device.id, "sessions"] as const,
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

  function sessionOptions(sessionId: string, enabled = true) {
    return queryOptions({
      queryKey: sessionKeys.detail(sessionId),
      enabled: enabled && sessionId.length > 0,
      queryFn: ({ signal }) => fetchSessionSnapshot(sessionId, signal, device.api.getSession),
      refetchOnMount: false,
      retryOnMount: false,
    });
  }

  function sessionTurnsOptions(sessionId: string, enabled = true) {
    return queryOptions({
      queryKey: sessionKeys.turns(sessionId),
      enabled: enabled && sessionId.length > 0,
      queryFn: ({ signal }) => fetchSessionSnapshot(sessionId, signal, device.api.listTurns),
      refetchOnMount: false,
      retryOnMount: false,
    });
  }

  function sessionInboxMessagesOptions(sessionId: string, enabled = true) {
    return queryOptions({
      queryKey: sessionKeys.inboxMessages(sessionId),
      enabled: enabled && sessionId.length > 0,
      queryFn: ({ signal }) =>
        fetchSessionSnapshot(sessionId, signal, device.api.listInboxMessages),
      refetchOnMount: false,
      retryOnMount: false,
    });
  }

  function sessionEventsOptions(sessionId: string, enabled = true) {
    return queryOptions({
      queryKey: sessionKeys.events(sessionId),
      enabled: enabled && sessionId.length > 0,
      queryFn: ({ signal }) => fetchSessionSnapshot(sessionId, signal, device.api.listEvents),
      refetchOnMount: false,
      retryOnMount: false,
    });
  }

  function sessionModelsOptions(sessionId: string) {
    return queryOptions({
      queryKey: sessionKeys.models(sessionId),
      enabled: sessionId.length > 0,
      queryFn: ({ signal }) => device.api.listSessionModels(sessionId, { signal }),
    });
  }

  function createSessionQuery(sessionId: () => string, enabled: () => boolean = () => true) {
    return createQuery(
      () => sessionOptions(sessionId(), enabled()),
      () => device.queryClient,
    );
  }

  function createSessionTurnsQuery(sessionId: () => string, enabled: () => boolean = () => true) {
    return createQuery(
      () => sessionTurnsOptions(sessionId(), enabled()),
      () => device.queryClient,
    );
  }

  function createSessionInboxMessagesQuery(
    sessionId: () => string,
    enabled: () => boolean = () => true,
  ) {
    return createQuery(
      () => sessionInboxMessagesOptions(sessionId(), enabled()),
      () => device.queryClient,
    );
  }

  function createSessionEventsQuery(sessionId: () => string, enabled: () => boolean = () => true) {
    return createQuery(
      () => sessionEventsOptions(sessionId(), enabled()),
      () => device.queryClient,
    );
  }

  function createSessionModelsQuery(sessionId: () => string) {
    return createQuery(
      () => sessionModelsOptions(sessionId()),
      () => device.queryClient,
    );
  }

  const sessionSnapshotReads = new Set<{ sessionId: string; invalidated: boolean }>();

  async function fetchSessionSnapshot<T>(
    sessionId: string,
    signal: AbortSignal,
    request: (sessionId: string, options: { signal: AbortSignal }) => Promise<T>,
  ): Promise<T> {
    const read = { sessionId, invalidated: false };
    sessionSnapshotReads.add(read);
    try {
      while (true) {
        read.invalidated = false;
        const snapshot = await request(sessionId, { signal });
        signal.throwIfAborted();
        if (!read.invalidated) return snapshot;
      }
    } finally {
      sessionSnapshotReads.delete(read);
    }
  }

  async function fetchSessionDetail(sessionId: string): Promise<SessionConsoleDetail> {
    const [session, turns, inboxMessages, events] = await Promise.all([
      device.queryClient.fetchQuery(sessionOptions(sessionId)),
      device.queryClient.fetchQuery(sessionTurnsOptions(sessionId)),
      device.queryClient.fetchQuery(sessionInboxMessagesOptions(sessionId)),
      device.queryClient.fetchQuery(sessionEventsOptions(sessionId)),
    ]);
    return { session, turns, inboxMessages, events };
  }

  function invalidateSessionDetail(
    sessionId: string,
    { includeModels = true }: { includeModels?: boolean } = {},
  ): Promise<void> {
    // Invalidating a query does not supersede an older fetchQuery already in flight.
    for (const read of sessionSnapshotReads) {
      if (read.sessionId === sessionId) read.invalidated = true;
    }
    return device.queryClient.invalidateQueries({
      queryKey: sessionKeys.detail(sessionId),
      predicate: (query) => includeModels || query.queryKey[4] !== "models",
    });
  }

  function clearSessionQueries(): void {
    void device.queryClient.cancelQueries({ queryKey: sessionKeys.details() });
    device.queryClient.removeQueries({ queryKey: sessionKeys.details() });
    device.queryClient.removeQueries({ queryKey: sessionKeys.optimisticInbox() });
    device.queryClient.removeQueries({ queryKey: sessionKeys.optimisticChat() });
  }

  function setSessionDetail(detail: SessionConsoleDetail): void {
    const sessionId = detail.session.session_id;
    device.queryClient.setQueryData(sessionKeys.detail(sessionId), detail.session);
    device.queryClient.setQueryData(sessionKeys.turns(sessionId), detail.turns);
    device.queryClient.setQueryData(sessionKeys.inboxMessages(sessionId), detail.inboxMessages);
    device.queryClient.setQueryData(sessionKeys.events(sessionId), detail.events);
  }

  function setSessionInboxMessages(
    sessionId: string,
    update: (messages: InboxMessageView[]) => InboxMessageView[],
  ): void {
    device.queryClient.setQueryData<InboxMessageView[]>(
      sessionKeys.inboxMessages(sessionId),
      (messages) => update(messages ?? []),
    );
  }

  async function applySessionResult(session: SessionView): Promise<void> {
    device.queryClient.setQueryData(sessionKeys.detail(session.session_id), session);
    await device.sessionOverview.invalidateSessionOverview();
  }

  async function invalidateAfterControl(sessionId: string): Promise<void> {
    await Promise.all([
      device.sessionOverview.invalidateSessionOverview(),
      invalidateSessionDetail(sessionId),
    ]);
  }

  const createSessionMutationOptions = () => ({
    mutationFn: device.api.createSession,
    onSuccess: async (result: CreateSessionResult, input: CreateSessionInput) => {
      setSessionDetail({
        session: result.session,
        turns: result.initial_turn ? [result.initial_turn] : [],
        inboxMessages: [],
        events: [],
      });
      if (input.initial_task?.input) {
        device.optimisticChat.rememberOptimisticMessage(
          result.session.session_id,
          input.initial_task.input,
          result.initial_turn,
        );
      }
      await device.sessionOverview.invalidateSessionOverview();
    },
  });

  const updateSessionMutationOptions = () => ({
    mutationFn: ({ sessionId, input }: { sessionId: string; input: UpdateSessionInput }) =>
      device.api.updateSession(sessionId, input),
    onSuccess: applySessionResult,
  });

  const pinSessionMutationOptions = () => ({
    mutationFn: device.api.pinSession,
    onSuccess: applySessionResult,
  });

  const unpinSessionMutationOptions = () => ({
    mutationFn: device.api.unpinSession,
    onSuccess: applySessionResult,
  });

  const archiveSessionMutationOptions = () => ({
    mutationFn: device.api.archiveSession,
    onSuccess: applySessionResult,
  });

  const interruptSessionMutationOptions = () => ({
    mutationFn: device.api.interruptSession,
    onSuccess: async (_result: unknown, sessionId: string) => invalidateAfterControl(sessionId),
  });

  const resumeSessionMutationOptions = () => ({
    mutationFn: device.api.resumeSession,
    onSuccess: async (_result: unknown, sessionId: string) => invalidateAfterControl(sessionId),
  });

  const terminateSessionMutationOptions = () => ({
    mutationFn: device.api.terminateSession,
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
    }) => device.api.setSessionModel(sessionId, model, runtimeId),
    onSuccess: async (_result: void, { sessionId }: { sessionId: string }) =>
      invalidateSessionDetail(sessionId),
  });

  function createCreateSessionMutation() {
    return createMutation(createSessionMutationOptions, () => device.queryClient);
  }

  function createUpdateSessionMutation() {
    return createMutation(updateSessionMutationOptions, () => device.queryClient);
  }

  function createPinSessionMutation() {
    return createMutation(pinSessionMutationOptions, () => device.queryClient);
  }

  function createUnpinSessionMutation() {
    return createMutation(unpinSessionMutationOptions, () => device.queryClient);
  }

  function createArchiveSessionMutation() {
    return createMutation(archiveSessionMutationOptions, () => device.queryClient);
  }

  function createInterruptSessionMutation() {
    return createMutation(interruptSessionMutationOptions, () => device.queryClient);
  }

  function createResumeSessionMutation() {
    return createMutation(resumeSessionMutationOptions, () => device.queryClient);
  }

  function createTerminateSessionMutation() {
    return createMutation(terminateSessionMutationOptions, () => device.queryClient);
  }

  function createSetSessionModelMutation() {
    return createMutation(setSessionModelMutationOptions, () => device.queryClient);
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

  function mutateCreateSession(input: CreateSessionInput): Promise<CreateSessionResult> {
    return executeMutation(createSessionMutationOptions(), input);
  }

  function mutateUpdateSession(sessionId: string, input: UpdateSessionInput): Promise<SessionView> {
    return executeMutation(updateSessionMutationOptions(), { sessionId, input });
  }

  function mutatePinSession(sessionId: string): Promise<SessionView> {
    return executeMutation(pinSessionMutationOptions(), sessionId);
  }

  function mutateUnpinSession(sessionId: string): Promise<SessionView> {
    return executeMutation(unpinSessionMutationOptions(), sessionId);
  }

  function mutateArchiveSession(sessionId: string): Promise<SessionView> {
    return executeMutation(archiveSessionMutationOptions(), sessionId);
  }

  async function mutateInterruptSession(sessionId: string): Promise<void> {
    await executeMutation(interruptSessionMutationOptions(), sessionId);
  }

  async function mutateResumeSession(sessionId: string): Promise<void> {
    await executeMutation(resumeSessionMutationOptions(), sessionId);
  }

  async function mutateTerminateSession(sessionId: string): Promise<void> {
    await executeMutation(terminateSessionMutationOptions(), sessionId);
  }
  return {
    sessionKeys,
    sessionOptions,
    sessionTurnsOptions,
    sessionInboxMessagesOptions,
    sessionEventsOptions,
    createSessionQuery,
    createSessionTurnsQuery,
    createSessionInboxMessagesQuery,
    createSessionEventsQuery,
    createSessionModelsQuery,
    fetchSessionDetail,
    invalidateSessionDetail,
    clearSessionQueries,
    setSessionDetail,
    setSessionInboxMessages,
    createCreateSessionMutation,
    createUpdateSessionMutation,
    createPinSessionMutation,
    createUnpinSessionMutation,
    createArchiveSessionMutation,
    createInterruptSessionMutation,
    createResumeSessionMutation,
    createTerminateSessionMutation,
    createSetSessionModelMutation,
    mutateCreateSession,
    mutateUpdateSession,
    mutatePinSession,
    mutateUnpinSession,
    mutateArchiveSession,
    mutateInterruptSession,
    mutateResumeSession,
    mutateTerminateSession,
  };
}
