import { QueriesObserver, type QueryObserverOptions } from "@tanstack/svelte-query";
import { derived, get, readable, writable } from "svelte/store";
import { ApiError } from "../api/errors";
import { getInboxMessage, retryInboxMessage as apiRetryInboxMessage } from "../api/client";
import {
  rememberSubmission,
  forgetSubmission,
  reconcileSubmissions,
  SubmissionUnconfirmedError,
  type UnconfirmedSubmission,
} from "./inboxRecovery";
import {
  cancelInboxMessage as apiCancelInboxMessage,
  dismissInboxMessage as apiDismissInboxMessage,
  submitInboxMessage as apiSubmitInboxMessage,
} from "../api/client";
import {
  beginInboxSubmission,
  confirmInboxSubmission,
  failInboxSubmission,
  syncInboxSubmissions,
} from "./optimisticInbox";
import type {
  CreateSessionInput,
  CreateSessionResult,
  InboxMessageView,
  SessionView,
  SubmitInboxMessageInput,
} from "../api/types";
import { clearSessionOverviewQuery, invalidateSessionOverview } from "../queries/sessionOverview";
import { queryClient } from "../queries/queryClient";
import {
  clearSessionQueries,
  fetchSessionDetail,
  mutateArchiveSession,
  mutateCreateSession,
  mutateInterruptSession,
  mutatePinSession,
  mutateResumeSession,
  mutateTerminateSession,
  mutateUnpinSession,
  mutateUpdateSession,
  sessionEventsOptions,
  sessionInboxMessagesOptions,
  sessionKeys,
  sessionOptions,
  sessionTurnsOptions,
  setSessionInboxMessages,
  type SessionConsoleDetail,
} from "../queries/sessions";

export type { SessionConsoleDetail } from "../queries/sessions";
export type SessionDetailErrorKind = "not_found" | "authentication" | "network" | "request";
export const selectedSessionId = writable<string | null>(null);

type SessionDetailState = {
  detail: SessionConsoleDetail | null;
  loading: boolean;
  error: string | null;
  errorKind: SessionDetailErrorKind | null;
};

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
  const sessionState = queryClient.getQueryState<SessionView>(sessionKeys.detail(sessionId));
  const turnsState = queryClient.getQueryState(sessionKeys.turns(sessionId));
  const inboxState = queryClient.getQueryState<InboxMessageView[]>(
    sessionKeys.inboxMessages(sessionId),
  );
  const eventsState = queryClient.getQueryState(sessionKeys.events(sessionId));
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
    sessionOptions(id, false),
    sessionTurnsOptions(id, false),
    sessionInboxMessagesOptions(id, false),
    sessionEventsOptions(id, false),
  ];
}

const sessionDetailState = readable<SessionDetailState>(readSessionDetailState(null), (set) => {
  let sessionId = get(selectedSessionId);
  let lastInboxMessages: InboxMessageView[] | undefined;
  const observer = new QueriesObserver(queryClient, detailQueryOptions(sessionId));
  const update = () => {
    const state = readSessionDetailState(sessionId);
    if (state.detail?.inboxMessages !== lastInboxMessages) {
      lastInboxMessages = state.detail?.inboxMessages;
      if (lastInboxMessages) {
        syncInboxSubmissions(lastInboxMessages);
        reconcileSubmissions(lastInboxMessages);
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

export const sessionDetail = derived(sessionDetailState, (state) => state.detail);
export const sessionDetailLoading = derived(sessionDetailState, (state) => state.loading);
export const sessionDetailError = derived(sessionDetailState, (state) => state.error);
export const sessionDetailErrorKind = derived(sessionDetailState, (state) => state.errorKind);

export function resetSessions(): void {
  clearSessionOverviewQuery();
  clearSessionQueries();
  selectedSessionId.set(null);
}

export function selectSession(sessionId: string | null): void {
  const previous = get(selectedSessionId);
  if (previous === sessionId) return;
  if (previous) void queryClient.cancelQueries({ queryKey: sessionKeys.detail(previous) });
  selectedSessionId.set(sessionId);
}

type SessionDetailLoadOptions = {
  showLoading?: boolean;
};

export async function loadSessionDetail(
  sessionId: string,
  _options: SessionDetailLoadOptions = {},
): Promise<SessionConsoleDetail | null> {
  const selected = get(selectedSessionId);
  if (!sessionId || (selected && selected !== sessionId)) return null;
  try {
    const detail = await fetchSessionDetail(sessionId);
    if (get(selectedSessionId) !== sessionId) return null;
    syncInboxSubmissions(detail.inboxMessages);
    reconcileSubmissions(detail.inboxMessages);
    return detail;
  } catch {
    return null;
  }
}

export function createSession(input: CreateSessionInput): Promise<CreateSessionResult> {
  return mutateCreateSession(input);
}

export function updateSessionTitle(sessionId: string, title: string | null): Promise<SessionView> {
  return mutateUpdateSession(sessionId, { title });
}

export function pinSession(sessionId: string): Promise<SessionView> {
  return mutatePinSession(sessionId);
}

export function unpinSession(sessionId: string): Promise<SessionView> {
  return mutateUnpinSession(sessionId);
}

export function archiveSession(sessionId: string): Promise<SessionView> {
  return mutateArchiveSession(sessionId);
}

async function refreshSidebarAndSelectedSession(sessionId: string): Promise<void> {
  await Promise.all([
    invalidateSessionOverview(),
    get(selectedSessionId) === sessionId
      ? queryClient.invalidateQueries({ queryKey: sessionKeys.detail(sessionId) })
      : Promise.resolve(),
  ]);
}

export async function submitInboxMessage(
  sessionId: string,
  input: SubmitInboxMessageInput,
  options: { showInChat?: boolean } = {},
): Promise<InboxMessageView> {
  const detailSession = get(sessionDetail)?.session;
  const currentSession = detailSession?.session_id === sessionId ? detailSession : null;
  const submission = { messageId: `msg_${crypto.randomUUID()}`, sessionId, input };
  const localSubmissionId = beginInboxSubmission(sessionId, input, {
    messageId: submission.messageId,
    showInChat:
      options.showInChat ?? (!input.branch_target_turn_id && currentSession?.state !== "busy"),
  });
  let message: InboxMessageView;
  try {
    rememberSubmission(submission);
    message = await deliverSubmission(submission);
  } catch (error) {
    failInboxSubmission(localSubmissionId);
    throw error;
  }

  confirmInboxSubmission(localSubmissionId, message);
  setSessionInboxMessages(sessionId, (messages) => {
    const withoutAcceptedMessage = messages.filter(
      (item) => item.message_id !== message.message_id,
    );
    return [...withoutAcceptedMessage, message];
  });
  await refreshSidebarAndSelectedSession(sessionId);
  return message;
}

async function deliverSubmission(
  submission: UnconfirmedSubmission,
  recovering = false,
): Promise<InboxMessageView> {
  let message: InboxMessageView;
  try {
    message = submission.retryOf
      ? await apiRetryInboxMessage(
          submission.sessionId,
          submission.retryOf,
          submission.messageId,
          submission.allowUnknown ?? false,
        )
      : await apiSubmitInboxMessage(submission.sessionId, submission.input, submission.messageId);
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

export async function recoverInboxSubmission(submission: UnconfirmedSubmission): Promise<void> {
  try {
    await getInboxMessage(submission.sessionId, submission.messageId);
    forgetSubmission(submission.messageId);
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 404) throw error;
    await deliverSubmission(submission, true);
  }
  await refreshSidebarAndSelectedSession(submission.sessionId);
}

export async function retryInboxMessage(
  sessionId: string,
  original: InboxMessageView,
  allowUnknown = false,
): Promise<void> {
  const submission: UnconfirmedSubmission = {
    messageId: `msg_${crypto.randomUUID()}`,
    sessionId,
    input: { input: original.input.summary },
    retryOf: original.message_id,
    allowUnknown,
  };
  rememberSubmission(submission);
  await deliverSubmission(submission);
  await refreshSidebarAndSelectedSession(sessionId);
}

export async function cancelInboxMessage(
  sessionId: string,
  messageId: string,
): Promise<InboxMessageView> {
  const message = await apiCancelInboxMessage(sessionId, messageId);
  await refreshSidebarAndSelectedSession(sessionId);
  return message;
}

export async function dismissInboxMessage(
  sessionId: string,
  messageId: string,
): Promise<InboxMessageView> {
  const message = await apiDismissInboxMessage(sessionId, messageId);
  await refreshSidebarAndSelectedSession(sessionId);
  return message;
}

async function refreshSelectedSessionAfterControl(sessionId: string): Promise<void> {
  if (get(selectedSessionId) === sessionId) {
    await loadSessionDetail(sessionId, { showLoading: false });
  }
}

export async function interruptSession(sessionId: string): Promise<void> {
  await mutateInterruptSession(sessionId);
  await refreshSelectedSessionAfterControl(sessionId);
}

export async function resumeSession(sessionId: string): Promise<void> {
  await mutateResumeSession(sessionId);
  await refreshSelectedSessionAfterControl(sessionId);
}

export async function terminateSession(sessionId: string): Promise<void> {
  await mutateTerminateSession(sessionId);
  await refreshSelectedSessionAfterControl(sessionId);
}
