import { QueriesObserver, type QueryObserverOptions } from "@tanstack/svelte-query";

import { derived, get, readable, writable } from "svelte/store";

import { ApiError } from "../api/errors";
import type {
  CreateSessionInput,
  CreateSessionResult,
  MessageDelivery,
  QueuedMessageView,
  SessionView,
  SendMessageInput,
} from "../api/types";
import { type SessionConsoleDetail } from "../queries/sessions";
import type { DeviceRuntime } from "../device/runtime";

export type { SessionConsoleDetail } from "../queries/sessions";

export type SessionDetailErrorKind = "not_found" | "authentication" | "network" | "request";

type SessionDetailState = {
  detail: SessionConsoleDetail | null;
  loading: boolean;
  error: string | null;
  errorKind: SessionDetailErrorKind | null;
};

type SessionDetailLoadOptions = {
  showLoading?: boolean;
};
export function createSessions(device: DeviceRuntime) {
  const selectedSessionId = writable<string | null>(null);

  function classifySessionDetailError(
    error: unknown,
    sessionFailed: boolean,
  ): SessionDetailErrorKind {
    if (error instanceof ApiError) {
      if (error.status === 404 && sessionFailed) return "not_found";
      if ([401, 403].includes(error.status)) return "authentication";
      return "request";
    }
    return error instanceof TypeError || error instanceof DOMException ? "network" : "request";
  }

  function readSessionDetailState(sessionId: string | null): SessionDetailState {
    if (!sessionId) return { detail: null, loading: false, error: null, errorKind: null };
    const sessionState = device.queryClient.getQueryState<SessionView>(
      device.sessionQueries.sessionKeys.detail(sessionId),
    );
    const turnsState = device.queryClient.getQueryState(
      device.sessionQueries.sessionKeys.turns(sessionId),
    );
    const queueState = device.queryClient.getQueryState<QueuedMessageView[]>(
      device.sessionQueries.sessionKeys.queue(sessionId),
    );
    const eventsState = device.queryClient.getQueryState(
      device.sessionQueries.sessionKeys.events(sessionId),
    );
    const states = [sessionState, turnsState, queueState, eventsState];
    const failedState = states.find((state) => state?.error);
    const detail =
      sessionState?.data && turnsState?.data && queueState?.data && eventsState?.data
        ? {
            session: sessionState.data,
            turns: turnsState.data as SessionConsoleDetail["turns"],
            queuedMessages: queueState.data,
            events: eventsState.data as SessionConsoleDetail["events"],
          }
        : null;
    const error = failedState?.error ?? null;
    return {
      detail,
      loading: !detail && states.some((state) => state?.fetchStatus === "fetching"),
      error: error ? (error instanceof Error ? error.message : String(error)) : null,
      errorKind: error ? classifySessionDetailError(error, failedState === sessionState) : null,
    };
  }

  function detailQueryOptions(
    sessionId: string | null,
  ): Array<QueryObserverOptions<any, any, any, any, any>> {
    const id = sessionId ?? "";
    return [
      device.sessionQueries.sessionOptions(id, false),
      device.sessionQueries.sessionTurnsOptions(id, false),
      device.sessionQueries.sessionQueueOptions(id, false),
      device.sessionQueries.sessionEventsOptions(id, false),
    ];
  }

  const sessionDetailState = readable<SessionDetailState>(readSessionDetailState(null), (set) => {
    let sessionId = get(selectedSessionId);
    const observer = new QueriesObserver(device.queryClient, detailQueryOptions(sessionId));
    const update = () => {
      const state = readSessionDetailState(sessionId);
      set(state);
    };
    const unsubscribeQueries = observer.subscribe(update);
    const unsubscribeSelection = selectedSessionId.subscribe((selected) => {
      sessionId = selected;
      observer.setQueries(detailQueryOptions(sessionId));
      update();
    });
    return () => {
      unsubscribeSelection();
      unsubscribeQueries();
      observer.destroy();
    };
  });

  const sessionDetail = derived(sessionDetailState, (state) => state.detail);

  const sessionDetailLoading = derived(sessionDetailState, (state) => state.loading);

  const sessionDetailError = derived(sessionDetailState, (state) => state.error);

  const sessionDetailErrorKind = derived(sessionDetailState, (state) => state.errorKind);

  function resetSessions(): void {
    device.sessionOverview.clearSessionOverviewQuery();
    device.sessionQueries.clearSessionQueries();
    selectedSessionId.set(null);
  }

  function selectSession(sessionId: string | null): void {
    const previous = get(selectedSessionId);
    if (previous === sessionId) return;
    if (previous)
      void device.queryClient.cancelQueries({
        queryKey: device.sessionQueries.sessionKeys.detail(previous),
      });
    selectedSessionId.set(sessionId);
  }

  async function loadSessionDetail(
    sessionId: string,
    _options: SessionDetailLoadOptions = {},
  ): Promise<SessionConsoleDetail | null> {
    const selected = get(selectedSessionId);
    if (!sessionId || (selected && selected !== sessionId)) return null;
    try {
      const detail = await device.sessionQueries.fetchSessionDetail(sessionId);
      if (get(selectedSessionId) !== sessionId) return null;
      return detail;
    } catch {
      return null;
    }
  }

  function createSession(input: CreateSessionInput): Promise<CreateSessionResult> {
    return device.sessionQueries.mutateCreateSession(input);
  }

  function updateSessionTitle(sessionId: string, title: string | null): Promise<SessionView> {
    return device.sessionQueries.mutateUpdateSession(sessionId, { title });
  }

  function pinSession(sessionId: string): Promise<SessionView> {
    return device.sessionQueries.mutatePinSession(sessionId);
  }

  function unpinSession(sessionId: string): Promise<SessionView> {
    return device.sessionQueries.mutateUnpinSession(sessionId);
  }

  function archiveSession(sessionId: string): Promise<SessionView> {
    return device.sessionQueries.mutateArchiveSession(sessionId);
  }

  function unarchiveSession(sessionId: string): Promise<SessionView> {
    return device.sessionQueries.mutateUnarchiveSession(sessionId);
  }

  function sendMessage(sessionId: string, input: SendMessageInput): Promise<MessageDelivery> {
    return device.queueQueries.mutateSendMessage(sessionId, input);
  }

  function cancelQueuedMessage(sessionId: string, messageId: string): Promise<QueuedMessageView> {
    return device.queueQueries.mutateCancelQueuedMessage(sessionId, messageId);
  }

  async function refreshSelectedSessionAfterControl(sessionId: string): Promise<void> {
    if (get(selectedSessionId) === sessionId) {
      await loadSessionDetail(sessionId, { showLoading: false });
    }
  }

  async function interruptSession(sessionId: string): Promise<void> {
    await device.sessionQueries.mutateInterruptSession(sessionId);
    await refreshSelectedSessionAfterControl(sessionId);
  }

  async function resumeSession(sessionId: string): Promise<void> {
    await device.sessionQueries.mutateResumeSession(sessionId);
    await refreshSelectedSessionAfterControl(sessionId);
  }

  async function terminateSession(sessionId: string): Promise<void> {
    await device.sessionQueries.mutateTerminateSession(sessionId);
    await refreshSelectedSessionAfterControl(sessionId);
  }
  return {
    selectedSessionId,
    sessionDetail,
    sessionDetailLoading,
    sessionDetailError,
    sessionDetailErrorKind,
    resetSessions,
    selectSession,
    loadSessionDetail,
    createSession,
    updateSessionTitle,
    pinSession,
    unpinSession,
    archiveSession,
    unarchiveSession,
    sendMessage,
    cancelQueuedMessage,
    interruptSession,
    resumeSession,
    terminateSession,
  };
}
