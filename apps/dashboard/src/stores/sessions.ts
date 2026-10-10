import { get, writable } from "svelte/store";
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
  archiveSession as apiArchiveSession,
  cancelInboxMessage as apiCancelInboxMessage,
  createSession as apiCreateSession,
  dismissInboxMessage as apiDismissInboxMessage,
  getSession,
  getSessionOverview,
  interruptSession as apiInterruptSession,
  listEvents,
  listInboxMessages,
  listTurns,
  pinSession as apiPinSession,
  restartSession as apiRestartSession,
  resumeSession as apiResumeSession,
  submitInboxMessage as apiSubmitInboxMessage,
  terminateSession as apiTerminateSession,
  unarchiveSession as apiUnarchiveSession,
  unpinSession as apiUnpinSession,
  updateSession as apiUpdateSession,
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
  EventView,
  InboxMessageView,
  SessionView,
  SubmitInboxMessageInput,
  TurnView,
} from "../api/types";

export interface SessionConsoleDetail {
  session: SessionView;
  turns: TurnView[];
  inboxMessages: InboxMessageView[];
  events: EventView[];
}

export const sidebarPinnedSessions = writable<SessionView[]>([]);
export const sidebarActiveSessions = writable<SessionView[]>([]);
export const sidebarRecentSessions = writable<SessionView[]>([]);
export const sidebarSessionsLoading = writable(false);
export const sidebarSessionsLoadingMore = writable(false);
export const sidebarSessionsError = writable<string | null>(null);
export const sidebarSessionsNextCursor = writable<string | null>(null);
export const sessionsPagePinnedSessions = writable<SessionView[]>([]);
export const sessionsPageArchivedSessions = writable<SessionView[]>([]);
export const sessionsPageActiveSessions = writable<SessionView[]>([]);
export const sessionsPageListSessions = writable<SessionView[]>([]);
export const sessionsPageLoading = writable(false);
export const sessionsPageLoadingMore = writable(false);
export const sessionsPageError = writable<string | null>(null);
export const sessionsPageNextCursor = writable<string | null>(null);
export const sessionDetail = writable<SessionConsoleDetail | null>(null);
export const sessionDetailLoading = writable(false);
export const sessionDetailError = writable<string | null>(null);
export type SessionDetailErrorKind = "not_found" | "authentication" | "network" | "request";
export const sessionDetailErrorKind = writable<SessionDetailErrorKind | null>(null);
export const selectedSessionId = writable<string | null>(null);

let selectionGeneration = 0;
let detailRequest: {
  sessionId: string;
  generation: number;
  controller: AbortController;
  dirty: boolean;
  promise: Promise<SessionConsoleDetail | null>;
} | null = null;
let sidebarOverviewRequest = 0;
let sidebarLoadMorePromise: Promise<SessionView[]> | null = null;
let sessionsPageOverviewRequest = 0;
let sessionsPageLoadMorePromise: Promise<SessionView[]> | null = null;

export function resetSessions(): void {
  selectionGeneration += 1;
  detailRequest?.controller.abort();
  detailRequest = null;
  sidebarOverviewRequest += 1;
  sidebarLoadMorePromise = null;
  sidebarPinnedSessions.set([]);
  sidebarActiveSessions.set([]);
  sidebarRecentSessions.set([]);
  sidebarSessionsLoading.set(false);
  sidebarSessionsLoadingMore.set(false);
  sidebarSessionsError.set(null);
  sidebarSessionsNextCursor.set(null);
  sessionsPageOverviewRequest += 1;
  sessionsPageLoadMorePromise = null;
  sessionsPagePinnedSessions.set([]);
  sessionsPageArchivedSessions.set([]);
  sessionsPageActiveSessions.set([]);
  sessionsPageListSessions.set([]);
  sessionsPageLoading.set(false);
  sessionsPageLoadingMore.set(false);
  sessionsPageError.set(null);
  sessionsPageNextCursor.set(null);
  selectedSessionId.set(null);
  sessionDetail.set(null);
  sessionDetailLoading.set(false);
  sessionDetailError.set(null);
  sessionDetailErrorKind.set(null);
}

export function selectSession(sessionId: string | null): void {
  if (get(selectedSessionId) === sessionId) return;
  selectionGeneration += 1;
  detailRequest?.controller.abort();
  detailRequest = null;
  selectedSessionId.set(sessionId);
  if (get(sessionDetail)?.session.session_id !== sessionId) sessionDetail.set(null);
  sessionDetailError.set(null);
  sessionDetailErrorKind.set(null);
  sessionDetailLoading.set(false);
}

type SessionDetailLoadOptions = {
  showLoading?: boolean;
};

const defaultSidebarSessionListLimit = 50;

export async function loadSidebarSessionOverview(
  options: {
    showLoading?: boolean;
  } = {},
): Promise<SessionView[]> {
  const request = ++sidebarOverviewRequest;
  sidebarLoadMorePromise = null;
  const showLoading = options.showLoading ?? true;
  if (showLoading) sidebarSessionsLoading.set(true);
  sidebarSessionsLoadingMore.set(false);
  sidebarSessionsError.set(null);
  try {
    const overview = await getSessionOverview({
      sections: ["pinned", "active", "list"],
      limit: defaultSidebarSessionListLimit,
    });
    const pinned = overview.groups.pinned?.sessions ?? [];
    const active = overview.groups.active?.sessions ?? [];
    const recent = overview.groups.list?.sessions ?? [];
    if (request === sidebarOverviewRequest) {
      sidebarPinnedSessions.set(pinned);
      sidebarActiveSessions.set(active);
      sidebarRecentSessions.set(recent);
      sidebarSessionsNextCursor.set(overview.groups.list?.next_cursor ?? null);
    }
    return [...active, ...recent];
  } catch (error) {
    if (request === sidebarOverviewRequest) {
      sidebarSessionsError.set(error instanceof Error ? error.message : String(error));
    }
    return [];
  } finally {
    if (request === sidebarOverviewRequest) sidebarSessionsLoading.set(false);
  }
}

export function loadMoreSidebarSessions(): Promise<SessionView[]> {
  if (sidebarLoadMorePromise) return sidebarLoadMorePromise;
  const cursor = get(sidebarSessionsNextCursor);
  if (!cursor) return Promise.resolve([]);

  const request = sidebarOverviewRequest;
  sidebarSessionsLoadingMore.set(true);
  sidebarSessionsError.set(null);
  const promise = getSessionOverview({
    sections: ["list"],
    limit: defaultSidebarSessionListLimit,
    cursor,
  })
    .then((overview) => {
      const loaded = overview.groups.list?.sessions ?? [];
      if (request !== sidebarOverviewRequest) return [];
      sidebarRecentSessions.update((current) => {
        const byId = new Map(current.map((session) => [session.session_id, session]));
        for (const session of loaded) byId.set(session.session_id, session);
        return [...byId.values()];
      });
      sidebarSessionsNextCursor.set(overview.groups.list?.next_cursor ?? null);
      return loaded;
    })
    .catch((error) => {
      if (request === sidebarOverviewRequest) {
        sidebarSessionsError.set(error instanceof Error ? error.message : String(error));
      }
      return [];
    })
    .finally(() => {
      if (request === sidebarOverviewRequest) sidebarSessionsLoadingMore.set(false);
      if (sidebarLoadMorePromise === promise) sidebarLoadMorePromise = null;
    });
  sidebarLoadMorePromise = promise;
  return promise;
}

const defaultSessionsPageListLimit = 50;

export async function loadSessionsPageOverview(): Promise<SessionView[]> {
  const request = ++sessionsPageOverviewRequest;
  sessionsPageLoadMorePromise = null;
  sessionsPageLoading.set(true);
  sessionsPageLoadingMore.set(false);
  sessionsPageError.set(null);
  try {
    const overview = await getSessionOverview({
      sections: ["active", "list", "archived", "pinned"],
      limit: defaultSessionsPageListLimit,
    });
    const pinned = overview.groups.pinned?.sessions ?? [];
    const archived = overview.groups.archived?.sessions ?? [];
    const active = overview.groups.active?.sessions ?? [];
    const list = overview.groups.list?.sessions ?? [];
    if (request === sessionsPageOverviewRequest) {
      sessionsPagePinnedSessions.set(pinned);
      sessionsPageArchivedSessions.set(archived);
      sessionsPageActiveSessions.set(active);
      sessionsPageListSessions.set(list);
      sessionsPageNextCursor.set(overview.groups.list?.next_cursor ?? null);
    }
    return [...active, ...list];
  } catch (error) {
    if (request === sessionsPageOverviewRequest) {
      sessionsPageError.set(error instanceof Error ? error.message : String(error));
    }
    return [];
  } finally {
    if (request === sessionsPageOverviewRequest) sessionsPageLoading.set(false);
  }
}

export function loadMoreSessionsPageSessions(): Promise<SessionView[]> {
  if (sessionsPageLoadMorePromise) return sessionsPageLoadMorePromise;
  const cursor = get(sessionsPageNextCursor);
  if (!cursor) return Promise.resolve([]);

  const request = sessionsPageOverviewRequest;
  sessionsPageLoadingMore.set(true);
  sessionsPageError.set(null);
  const promise = getSessionOverview({
    sections: ["list"],
    limit: defaultSessionsPageListLimit,
    cursor,
  })
    .then((overview) => {
      const loaded = overview.groups.list?.sessions ?? [];
      if (request !== sessionsPageOverviewRequest) return [];
      sessionsPageListSessions.update((current) => {
        const byId = new Map(current.map((session) => [session.session_id, session]));
        for (const session of loaded) byId.set(session.session_id, session);
        return [...byId.values()];
      });
      sessionsPageNextCursor.set(overview.groups.list?.next_cursor ?? null);
      return loaded;
    })
    .catch((error) => {
      if (request === sessionsPageOverviewRequest) {
        sessionsPageError.set(error instanceof Error ? error.message : String(error));
      }
      return [];
    })
    .finally(() => {
      if (request === sessionsPageOverviewRequest) sessionsPageLoadingMore.set(false);
      if (sessionsPageLoadMorePromise === promise) sessionsPageLoadMorePromise = null;
    });
  sessionsPageLoadMorePromise = promise;
  return promise;
}

export function loadSessionDetail(
  sessionId: string,
  options: SessionDetailLoadOptions = {},
): Promise<SessionConsoleDetail | null> {
  const selected = get(selectedSessionId);
  if (!sessionId || (selected && selected !== sessionId)) return Promise.resolve(null);
  if (detailRequest?.sessionId === sessionId && detailRequest.generation === selectionGeneration) {
    detailRequest.dirty = true;
    return detailRequest.promise;
  }

  detailRequest?.controller.abort();
  const request = {
    sessionId,
    generation: selectionGeneration,
    controller: new AbortController(),
    dirty: false,
    promise: Promise.resolve<SessionConsoleDetail | null>(null),
  };
  detailRequest = request;
  const isCurrent = () => detailRequest === request && request.generation === selectionGeneration;
  if (options.showLoading !== false || !get(sessionDetail)) sessionDetailLoading.set(true);

  request.promise = (async () => {
    let detail: SessionConsoleDetail | null = null;
    do {
      request.dirty = false;
      let sessionLoaded = false;
      const readOptions = { signal: request.controller.signal };
      try {
        const session = await getSession(sessionId, readOptions);
        if (!isCurrent()) return null;
        sessionLoaded = true;
        const [turns, inboxMessages, events] = await Promise.all([
          listTurns(sessionId, readOptions),
          listInboxMessages(sessionId, readOptions),
          listEvents(sessionId, readOptions),
        ]);
        if (!isCurrent()) return null;
        detail = { session, turns, inboxMessages, events };
        syncInboxSubmissions(inboxMessages);
        reconcileSubmissions(inboxMessages);
        sessionDetail.set(detail);
        sessionDetailError.set(null);
        sessionDetailErrorKind.set(null);
      } catch (error) {
        if (!isCurrent()) return null;
        detail = null;
        const kind: SessionDetailErrorKind =
          error instanceof ApiError
            ? error.status === 404 && !sessionLoaded
              ? "not_found"
              : [401, 403].includes(error.status)
                ? "authentication"
                : "request"
            : error instanceof TypeError || error instanceof DOMException
              ? "network"
              : "request";
        if (kind === "not_found" || kind === "authentication") sessionDetail.set(null);
        sessionDetailErrorKind.set(kind);
        sessionDetailError.set(error instanceof Error ? error.message : String(error));
      }
    } while (request.dirty && isCurrent());
    return detail;
  })().finally(() => {
    if (isCurrent()) {
      detailRequest = null;
      sessionDetailLoading.set(false);
    }
  });
  return request.promise;
}

export async function createSession(input: CreateSessionInput): Promise<CreateSessionResult> {
  const result = await apiCreateSession(input);
  sessionDetail.set({
    session: result.session,
    turns: result.initial_turn ? [result.initial_turn] : [],
    inboxMessages: [],
    events: [],
  });
  void loadSidebarSessionOverview({ showLoading: false });
  return result;
}

export async function updateSessionTitle(
  sessionId: string,
  title: string | null,
): Promise<SessionView> {
  const session = await apiUpdateSession(sessionId, { title });
  await refreshSidebarAndSelectedSession(sessionId);
  return session;
}

function applySessionManagementResult(session: SessionView): void {
  if (get(sessionDetail)?.session.session_id === session.session_id) {
    sessionDetail.update((detail) => (detail ? { ...detail, session } : detail));
  }
  if (detailRequest?.sessionId === session.session_id) detailRequest.dirty = true;
}

async function refreshAfterSessionManagement(session: SessionView): Promise<SessionView> {
  applySessionManagementResult(session);
  await loadSidebarSessionOverview({ showLoading: false });
  return session;
}

export async function pinSession(sessionId: string): Promise<SessionView> {
  return refreshAfterSessionManagement(await apiPinSession(sessionId));
}

export async function unpinSession(sessionId: string): Promise<SessionView> {
  return refreshAfterSessionManagement(await apiUnpinSession(sessionId));
}

export async function archiveSession(sessionId: string): Promise<SessionView> {
  return refreshAfterSessionManagement(await apiArchiveSession(sessionId));
}

export async function unarchiveSession(sessionId: string): Promise<SessionView> {
  const session = await apiUnarchiveSession(sessionId);
  if (session.archived_at) throw new Error("The session is still archived. Refresh and try again.");
  applySessionManagementResult(session);
  await loadSidebarSessionOverview({ showLoading: false });
  return session;
}

async function refreshSidebarAndSelectedSession(sessionId: string): Promise<void> {
  await Promise.all([
    loadSidebarSessionOverview({ showLoading: false }),
    get(selectedSessionId) === sessionId
      ? loadSessionDetail(sessionId, { showLoading: false })
      : Promise.resolve(null),
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
  sessionDetail.update((detail) => {
    if (detail?.session.session_id !== sessionId) return detail;
    const inboxMessages = detail.inboxMessages.filter(
      (item) => item.message_id !== message.message_id,
    );
    return { ...detail, inboxMessages: [...inboxMessages, message] };
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

export async function interruptSession(sessionId: string): Promise<void> {
  await apiInterruptSession(sessionId);
  await refreshSidebarAndSelectedSession(sessionId);
}

export async function restartSession(sessionId: string): Promise<void> {
  await apiRestartSession(sessionId);
  await refreshSidebarAndSelectedSession(sessionId);
}

export async function resumeSession(sessionId: string): Promise<void> {
  await apiResumeSession(sessionId);
  await refreshSidebarAndSelectedSession(sessionId);
}

export async function terminateSession(sessionId: string): Promise<void> {
  await apiTerminateSession(sessionId);
  await refreshSidebarAndSelectedSession(sessionId);
}
