import { QueriesObserver, type QueryObserverOptions } from "@tanstack/svelte-query";

import { derived, get, readable, writable } from "svelte/store";

import { ApiError } from "../api/errors";
import { type UnconfirmedSubmission } from "./inboxRecovery";

import type {
  CreateSessionInput,
  CreateSessionResult,
  InboxMessageView,
  SessionView,
  SubmitInboxMessageInput,
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
    const inboxState = device.queryClient.getQueryState<InboxMessageView[]>(
      device.sessionQueries.sessionKeys.inboxMessages(sessionId),
    );
    const eventsState = device.queryClient.getQueryState(
      device.sessionQueries.sessionKeys.events(sessionId),
    );
    const states = [sessionState, turnsState, inboxState, eventsState];
    const failedState = states.find((state) => state?.error);
    const detail =
      sessionState?.data && turnsState?.data && inboxState?.data && eventsState?.data
        ? {
            session: sessionState.data,
            turns: turnsState.data as SessionConsoleDetail["turns"],
            inboxMessages: inboxState.data,
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
      device.sessionQueries.sessionInboxMessagesOptions(id, false),
      device.sessionQueries.sessionEventsOptions(id, false),
    ];
  }

  const sessionDetailState = readable<SessionDetailState>(readSessionDetailState(null), (set) => {
    let sessionId = get(selectedSessionId);
    let lastInboxMessages: InboxMessageView[] | undefined;
    const observer = new QueriesObserver(device.queryClient, detailQueryOptions(sessionId));
    const update = () => {
      const state = readSessionDetailState(sessionId);
      if (state.detail?.inboxMessages !== lastInboxMessages) {
        lastInboxMessages = state.detail?.inboxMessages;
        if (lastInboxMessages) {
          device.inboxQueries.syncInboxSubmissions(sessionId ?? "", lastInboxMessages);
          device.inboxRecovery.reconcileSubmissions(lastInboxMessages);
        }
      }
      set(state);
    };
    const unsubscribeQueries = observer.subscribe(update);
    const unsubscribeSelection = selectedSessionId.subscribe((selected) => {
      sessionId = selected;
      lastInboxMessages = undefined;
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
      device.inboxQueries.syncInboxSubmissions(sessionId, detail.inboxMessages);
      device.inboxRecovery.reconcileSubmissions(detail.inboxMessages);
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

  function submitInboxMessage(
    sessionId: string,
    input: SubmitInboxMessageInput,
    options: { showInChat?: boolean } = {},
  ): Promise<InboxMessageView> {
    return device.inboxQueries.mutateSubmitInboxMessage(sessionId, input, options);
  }

  function recoverInboxSubmission(submission: UnconfirmedSubmission): Promise<void> {
    return device.inboxQueries.recoverInboxSubmission(submission);
  }

  function retryInboxMessage(
    sessionId: string,
    original: InboxMessageView,
    allowUnknown = false,
  ): Promise<void> {
    return device.inboxQueries.mutateRetryInboxMessage(sessionId, original, allowUnknown);
  }

  function cancelInboxMessage(sessionId: string, messageId: string): Promise<InboxMessageView> {
    return device.inboxQueries.mutateCancelInboxMessage(sessionId, messageId);
  }

  function dismissInboxMessage(sessionId: string, messageId: string): Promise<InboxMessageView> {
    return device.inboxQueries.mutateDismissInboxMessage(sessionId, messageId);
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
    submitInboxMessage,
    recoverInboxSubmission,
    retryInboxMessage,
    cancelInboxMessage,
    dismissInboxMessage,
    interruptSession,
    resumeSession,
    terminateSession,
  };
}
