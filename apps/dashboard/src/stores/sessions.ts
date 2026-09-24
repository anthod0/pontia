import { get, writable } from 'svelte/store';
import { ApiError } from '../api/errors';
import { getInboxMessage, retryInboxMessage as apiRetryInboxMessage } from '../api/client';
import { rememberSubmission, forgetSubmission, reconcileSubmissions, SubmissionUnconfirmedError, type UnconfirmedSubmission } from './inboxRecovery';
import {
  archiveSession as apiArchiveSession,
  cancelInboxMessage as apiCancelInboxMessage,
  createSession as apiCreateSession,
  dismissInboxMessage as apiDismissInboxMessage,
  getSession,
  interruptSession as apiInterruptSession,
  listEvents,
  listInboxMessages,
  listSessions,
  listTurns,
  pinSession as apiPinSession,
  restartSession as apiRestartSession,
  resumeSession as apiResumeSession,
  submitInboxMessage as apiSubmitInboxMessage,
  terminateSession as apiTerminateSession,
  unarchiveSession as apiUnarchiveSession,
  unpinSession as apiUnpinSession,
  updateSession as apiUpdateSession,
} from '../api/client';
import {
  beginInboxSubmission,
  confirmInboxSubmission,
  failInboxSubmission,
  syncInboxSubmissions,
} from './optimisticInbox';
import type {
  CreateSessionInput,
  CreateSessionResult,
  EventView,
  InboxMessageView,
  SessionView,
  SubmitInboxMessageInput,
  TurnView,
} from '../api/types';

export interface SessionConsoleDetail {
  session: SessionView;
  turns: TurnView[];
  inboxMessages: InboxMessageView[];
  events: EventView[];
}

export const sessions = writable<SessionView[]>([]);
export const sessionsLoading = writable(false);
export const sessionsError = writable<string | null>(null);
export const sessionDetail = writable<SessionConsoleDetail | null>(null);
export const sessionDetailLoading = writable(false);
export const sessionDetailError = writable<string | null>(null);
export type SessionDetailErrorKind = 'not_found' | 'authentication' | 'network' | 'request';
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
let listRequest = 0;

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

const defaultSessionListLimit = 50;

type LoadOptions = {
  showLoading?: boolean;
  limit?: number;
  includePinned?: boolean;
  throwOnError?: boolean;
};

export async function loadSessions(options: LoadOptions = {}): Promise<SessionView[]> {
  const request = ++listRequest;
  const showLoading = options.showLoading ?? true;
  if (showLoading) sessionsLoading.set(true);
  sessionsError.set(null);
  try {
    const loaded = await listSessions({
      limit: options.limit ?? defaultSessionListLimit,
      includePinned: options.includePinned ?? true,
    });
    if (request === listRequest) sessions.set(loaded);
    return loaded;
  } catch (error) {
    if (request === listRequest) sessionsError.set(error instanceof Error ? error.message : String(error));
    if (options.throwOnError) throw error;
    return [];
  } finally {
    if (request === listRequest) sessionsLoading.set(false);
  }
}

export function loadSessionDetail(sessionId: string, options: LoadOptions = {}): Promise<SessionConsoleDetail | null> {
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
        sessions.update((items) => items.map((item) => item.session_id === sessionId ? session : item));
      } catch (error) {
        if (!isCurrent()) return null;
        detail = null;
        const kind: SessionDetailErrorKind = error instanceof ApiError
          ? error.status === 404 && !sessionLoaded ? 'not_found'
            : [401, 403].includes(error.status) ? 'authentication' : 'request'
          : error instanceof TypeError || error instanceof DOMException ? 'network' : 'request';
        if (kind === 'not_found' || kind === 'authentication') sessionDetail.set(null);
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
  sessions.update((items) => {
    const withoutCreated = items.filter((item) => item.session_id !== result.session.session_id);
    return [result.session, ...withoutCreated];
  });
  sessionDetail.set({
    session: result.session,
    turns: result.initial_turn ? [result.initial_turn] : [],
    inboxMessages: [],
    events: [],
  });
  return result;
}

export async function updateSessionTitle(sessionId: string, title: string | null): Promise<SessionView> {
  const session = await apiUpdateSession(sessionId, { title });
  await loadSessions();
  await loadSessionDetail(sessionId);
  return session;
}

function applySessionManagementResult(session: SessionView): void {
  listRequest += 1;
  sessionsLoading.set(false);
  sessions.update((items) => {
    const remaining = items.filter((item) => item.session_id !== session.session_id);
    return session.archived_at ? remaining : [session, ...remaining];
  });
  if (get(sessionDetail)?.session.session_id === session.session_id) {
    sessionDetail.update((detail) => detail ? { ...detail, session } : detail);
  }
  if (detailRequest?.sessionId === session.session_id) detailRequest.dirty = true;
}

async function refreshAfterSessionManagement(session: SessionView): Promise<SessionView> {
  applySessionManagementResult(session);
  await loadSessions({ showLoading: false });
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
  if (session.archived_at) throw new Error('The session is still archived. Refresh and try again.');
  applySessionManagementResult(session);
  return session;
}

export async function submitInboxMessage(
  sessionId: string,
  input: SubmitInboxMessageInput,
  options: { showInChat?: boolean } = {},
): Promise<InboxMessageView> {
  const detailSession = get(sessionDetail)?.session;
  const currentSession = detailSession?.session_id === sessionId
    ? detailSession
    : get(sessions).find((session) => session.session_id === sessionId);
  const submission = { messageId: `msg_${crypto.randomUUID()}`, sessionId, input };
  const localSubmissionId = beginInboxSubmission(sessionId, input, {
    messageId: submission.messageId,
    showInChat: options.showInChat
      ?? (!input.branch_target_turn_id && currentSession?.state !== 'busy'),
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
    const inboxMessages = detail.inboxMessages.filter((item) => item.message_id !== message.message_id);
    return { ...detail, inboxMessages: [...inboxMessages, message] };
  });
  await loadSessions();
  await loadSessionDetail(sessionId);
  return message;
}

async function deliverSubmission(submission: UnconfirmedSubmission, recovering = false): Promise<InboxMessageView> {
  let message: InboxMessageView;
  try {
    message = submission.retryOf
      ? await apiRetryInboxMessage(submission.sessionId, submission.retryOf, submission.messageId, submission.allowUnknown ?? false)
      : await apiSubmitInboxMessage(submission.sessionId, submission.input, submission.messageId);
  } catch (error) {
    if (!recovering && error instanceof ApiError && !error.afterNetworkFailure
      && [400, 401, 403, 404, 409, 422].includes(error.status)
      && !['invalid_json', 'missing_data'].includes(error.code)) {
      forgetSubmission(submission.messageId);
      throw error;
    }
    throw new SubmissionUnconfirmedError();
  }
  try { forgetSubmission(submission.messageId); }
  catch { throw new SubmissionUnconfirmedError(); }
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
  await loadSessions();
  await loadSessionDetail(submission.sessionId);
}

export async function retryInboxMessage(sessionId: string, original: InboxMessageView, allowUnknown = false): Promise<void> {
  const submission: UnconfirmedSubmission = {
    messageId: `msg_${crypto.randomUUID()}`, sessionId,
    input: { input: original.input.summary }, retryOf: original.message_id, allowUnknown,
  };
  rememberSubmission(submission);
  await deliverSubmission(submission);
  await loadSessions();
  await loadSessionDetail(sessionId);
}

export async function cancelInboxMessage(sessionId: string, messageId: string): Promise<InboxMessageView> {
  const message = await apiCancelInboxMessage(sessionId, messageId);
  await loadSessions();
  await loadSessionDetail(sessionId);
  return message;
}

export async function dismissInboxMessage(sessionId: string, messageId: string): Promise<InboxMessageView> {
  const message = await apiDismissInboxMessage(sessionId, messageId);
  await loadSessions();
  await loadSessionDetail(sessionId);
  return message;
}

export async function interruptSession(sessionId: string): Promise<void> {
  await apiInterruptSession(sessionId);
  await loadSessions();
  await loadSessionDetail(sessionId);
}

export async function restartSession(sessionId: string): Promise<void> {
  await apiRestartSession(sessionId);
  await loadSessions();
  await loadSessionDetail(sessionId);
}

export async function resumeSession(sessionId: string): Promise<void> {
  await apiResumeSession(sessionId);
  await loadSessions();
  await loadSessionDetail(sessionId);
}

export async function terminateSession(sessionId: string): Promise<void> {
  await apiTerminateSession(sessionId);
  await loadSessions();
  if (get(sessionDetail)?.session.session_id === sessionId) {
    await loadSessionDetail(sessionId);
  }
}
