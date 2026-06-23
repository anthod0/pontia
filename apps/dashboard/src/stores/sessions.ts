import { get, writable } from 'svelte/store';
import {
  archiveSession as apiArchiveSession,
  cancelInboxMessage as apiCancelInboxMessage,
  createSession as apiCreateSession,
  dismissInboxMessage as apiDismissInboxMessage,
  discoverArtifacts,
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
  unpinSession as apiUnpinSession,
  updateSession as apiUpdateSession,
} from '../api/client';
import type {
  ArtifactView,
  CreateSessionInput,
  CreateSessionResult,
  EventView,
  InboxMessageView,
  SessionView,
  SubmitInboxMessageInput,
  TaskDagView,
  TaskView,
  TurnView,
} from '../api/types';

export interface TaskSessionDetail {
  session: SessionView;
  turns: TurnView[];
  events: EventView[];
  referencedBy: string[];
}

export const taskSessions = writable<TaskSessionDetail[]>([]);
export const taskSessionsLoading = writable(false);
export const taskSessionsError = writable<string | null>(null);

export interface SessionConsoleDetail {
  session: SessionView;
  turns: TurnView[];
  inboxMessages: InboxMessageView[];
  events: EventView[];
  artifacts: ArtifactView[];
}

export const sessions = writable<SessionView[]>([]);
export const sessionsLoading = writable(false);
export const sessionsError = writable<string | null>(null);
export const sessionDetail = writable<SessionConsoleDetail | null>(null);
export const sessionDetailLoading = writable(false);
export const sessionDetailError = writable<string | null>(null);

const defaultSessionListLimit = 50;

function taskSessionRefs(task: TaskView | null, dag: TaskDagView | null): Map<string, Set<string>> {
  const refs = new Map<string, Set<string>>();
  const add = (sessionId: string | null | undefined, ref: string) => {
    if (!sessionId) return;
    const existing = refs.get(sessionId) ?? new Set<string>();
    existing.add(ref);
    refs.set(sessionId, existing);
  };

  add(task?.session_id, 'task');
  for (const run of dag?.runs ?? []) add(run.session_id, `run ${run.run_id}`);
  for (const item of dag?.work_items ?? []) add(item.runtime?.session_id, `work item ${item.work_item_id}`);
  for (const signal of dag?.signals ?? []) add(signal.source_session_id, `signal ${signal.signal_id}`);
  return refs;
}

type LoadOptions = {
  showLoading?: boolean;
  limit?: number;
  includePinned?: boolean;
};

export async function loadSessions(options: LoadOptions = {}): Promise<SessionView[]> {
  const showLoading = options.showLoading ?? true;
  if (showLoading) sessionsLoading.set(true);
  sessionsError.set(null);
  try {
    const loaded = await listSessions({
      limit: options.limit ?? defaultSessionListLimit,
      includePinned: options.includePinned ?? true,
    });
    sessions.set(loaded);
    return loaded;
  } catch (error) {
    sessions.set([]);
    sessionsError.set(error instanceof Error ? error.message : String(error));
    return [];
  } finally {
    if (showLoading) sessionsLoading.set(false);
  }
}

export async function loadSessionDetail(sessionId: string, options: LoadOptions = {}): Promise<SessionConsoleDetail | null> {
  if (!sessionId) {
    sessionDetail.set(null);
    return null;
  }
  const showLoading = options.showLoading ?? true;
  if (showLoading) sessionDetailLoading.set(true);
  sessionDetailError.set(null);
  try {
    const [session, turns, inboxMessages, events] = await Promise.all([
      getSession(sessionId),
      listTurns(sessionId),
      listInboxMessages(sessionId),
      listEvents(sessionId),
    ]);
    const detail = { session, turns, inboxMessages, events, artifacts: [] } satisfies SessionConsoleDetail;
    sessionDetail.set(detail);
    sessions.update((items) => items.map((item) => item.session_id === session.session_id ? session : item));
    return detail;
  } catch (error) {
    if (showLoading) sessionDetail.set(null);
    sessionDetailError.set(error instanceof Error ? error.message : String(error));
    return null;
  } finally {
    if (showLoading) sessionDetailLoading.set(false);
  }
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
    artifacts: [],
  });
  return result;
}

export async function updateSessionTitle(sessionId: string, title: string | null): Promise<SessionView> {
  const session = await apiUpdateSession(sessionId, { title });
  await loadSessions();
  await loadSessionDetail(sessionId);
  return session;
}

async function refreshAfterSessionManagement(session: SessionView): Promise<SessionView> {
  await loadSessions({ showLoading: false });
  if (get(sessionDetail)?.session.session_id === session.session_id) {
    sessionDetail.update((detail) => detail ? { ...detail, session } : detail);
  }
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

export async function submitInboxMessage(sessionId: string, input: SubmitInboxMessageInput): Promise<InboxMessageView> {
  const message = await apiSubmitInboxMessage(sessionId, input);
  await loadSessions();
  await loadSessionDetail(sessionId);
  return message;
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
  await loadSessionDetail(sessionId);
}

export async function discoverSessionArtifacts(sessionId: string): Promise<void> {
  await discoverArtifacts(sessionId);
  await loadSessionDetail(sessionId);
}

export async function loadTaskSessions(task: TaskView | null, dag: TaskDagView | null): Promise<void> {
  const refs = taskSessionRefs(task, dag);
  taskSessionsLoading.set(true);
  taskSessionsError.set(null);
  try {
    const details = await Promise.all([...refs.entries()].map(async ([sessionId, referencedBy]) => {
      const [session, turns, events] = await Promise.all([getSession(sessionId), listTurns(sessionId), listEvents(sessionId)]);
      return { session, turns, events, referencedBy: [...referencedBy] } satisfies TaskSessionDetail;
    }));
    taskSessions.set(details.sort((a, b) => b.session.updated_at.localeCompare(a.session.updated_at)));
  } catch (error) {
    taskSessions.set([]);
    taskSessionsError.set(error instanceof Error ? error.message : String(error));
  } finally {
    taskSessionsLoading.set(false);
  }
}
