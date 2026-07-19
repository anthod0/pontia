import { get } from 'svelte/store';
import { token } from '../stores/auth';
import { ApiError } from './errors';
import type {
  AgentProfileView,
  ApiEnvelope,
  CreateSessionInput,
  CreateSessionResult,
  EventView,
  InboxMessageView,
  RegisterWorkspaceInput,
  RenameWorkspaceInput,
  SessionView,
  SubmitInboxMessageInput,
  TurnTimelineDirection,
  TurnTimelinePage,
  UpsertAgentProfileInput,
  TaskEventView,
  TaskView,
  TurnView,
  UpdateSessionInput,
  WorkspaceDirectoryListingView,
  FilePickerResultView,
  WorkspaceGitStatusView,
  WorkspaceRootView,
  WorkspaceView,
} from './types';

const API_BASE = '/external/v1';

type RequestOptions = Omit<RequestInit, 'body'> & { body?: unknown; mutating?: boolean };
export type ReadRequestOptions = Pick<RequestOptions, 'signal'>;

const TRANSIENT_NETWORK_RETRY_DELAYS_MS = [250, 750, 1500];

function idempotencyKey(): string {
  return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError';
}

export function isTransientNetworkError(error: unknown): boolean {
  if (isAbortError(error)) return false;
  if (error instanceof TypeError) return true;
  if (error instanceof DOMException && error.name === 'NetworkError') return true;

  const message = error instanceof Error ? error.message : typeof error === 'string' ? error : '';
  return /Failed to fetch|NetworkError|ERR_NETWORK_CHANGED|ERR_INTERNET_DISCONNECTED|ERR_NETWORK_ACCESS_DENIED|Load failed/i.test(message);
}

export function isAuthenticationFailure(status: number): boolean {
  return status === 401 || status === 403;
}

function delay(ms: number, signal?: AbortSignal | null): Promise<void> {
  if (signal?.aborted) return Promise.reject(new DOMException('The operation was aborted.', 'AbortError'));
  return new Promise((resolve, reject) => {
    const cleanup = () => signal?.removeEventListener('abort', abort);
    const timeout = window.setTimeout(() => {
      cleanup();
      resolve();
    }, ms);
    const abort = () => {
      window.clearTimeout(timeout);
      cleanup();
      reject(new DOMException('The operation was aborted.', 'AbortError'));
    };
    signal?.addEventListener('abort', abort, { once: true });
  });
}

async function fetchWithTransientNetworkRetry(input: RequestInfo | URL, init: RequestInit): Promise<Response> {
  let attempt = 0;
  while (true) {
    try {
      return await fetch(input, init);
    } catch (error) {
      if (!isTransientNetworkError(error) || attempt >= TRANSIENT_NETWORK_RETRY_DELAYS_MS.length) throw error;
      await delay(TRANSIENT_NETWORK_RETRY_DELAYS_MS[attempt], init.signal);
      attempt += 1;
    }
  }
}

export async function validateExternalApiToken(candidateToken: string): Promise<void> {
  const headers = new Headers();
  headers.set('Authorization', `Bearer ${candidateToken}`);
  const response = await fetchWithTransientNetworkRetry(`${API_BASE}/auth/validate`, { headers });
  const text = await response.text();
  let envelope: ApiEnvelope<unknown> | null = null;
  try {
    envelope = text ? JSON.parse(text) as ApiEnvelope<unknown> : null;
  } catch {
    throw new ApiError(text || response.statusText, 'invalid_json', response.status);
  }
  if (!response.ok || envelope?.error) {
    throw new ApiError(
      envelope?.error?.message ?? response.statusText,
      envelope?.error?.code ?? 'request_failed',
      response.status,
    );
  }
}

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const headers = new Headers(options.headers);
  const bearer = get(token).trim();
  if (bearer) headers.set('Authorization', `Bearer ${bearer}`);
  if (options.body !== undefined) headers.set('Content-Type', 'application/json');
  if (options.mutating || options.method && options.method !== 'GET') headers.set('Idempotency-Key', idempotencyKey());

  const response = await fetchWithTransientNetworkRetry(`${API_BASE}${path}`, {
    ...options,
    headers,
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
  });
  const text = await response.text();
  let envelope: ApiEnvelope<T> | null = null;
  try {
    envelope = text ? JSON.parse(text) as ApiEnvelope<T> : null;
  } catch {
    throw new ApiError(text || response.statusText, 'invalid_json', response.status);
  }
  if (!response.ok || envelope?.error) {
    if (isAuthenticationFailure(response.status)) token.set('');
    throw new ApiError(
      envelope?.error?.message ?? response.statusText,
      envelope?.error?.code ?? 'request_failed',
      response.status,
    );
  }
  if (!envelope || envelope.data === null) {
    throw new ApiError('Response did not include data.', 'missing_data', response.status);
  }
  return envelope.data;
}

export async function listAgentProfiles(includeArchived = false, options: ReadRequestOptions = {}): Promise<AgentProfileView[]> {
  const query = includeArchived ? '?include_archived=true' : '';
  return (await request<{ agent_profiles: AgentProfileView[] }>(`/agent-profiles${query}`, options)).agent_profiles;
}

export async function getAgentProfile(profileId: string): Promise<AgentProfileView> {
  return (await request<{ agent_profile: AgentProfileView }>(`/agent-profiles/${encodeURIComponent(profileId)}`)).agent_profile;
}

export async function createAgentProfile(input: UpsertAgentProfileInput): Promise<AgentProfileView> {
  return (await request<{ agent_profile: AgentProfileView }>('/agent-profiles', { method: 'POST', body: input, mutating: true })).agent_profile;
}

export async function deleteAgentProfile(profileId: string): Promise<{ profile_id: string; archived_versions: number }> {
  return request<{ profile_id: string; archived_versions: number }>(`/agent-profiles/${encodeURIComponent(profileId)}`, { method: 'DELETE', mutating: true });
}

export async function listAgentProfileVersions(profileId: string, includeArchived = false, options: ReadRequestOptions = {}): Promise<AgentProfileView[]> {
  const query = includeArchived ? '?include_archived=true' : '';
  return (await request<{ agent_profile_versions: AgentProfileView[] }>(`/agent-profiles/${encodeURIComponent(profileId)}/versions${query}`, options)).agent_profile_versions;
}

export async function createAgentProfileVersion(profileId: string, input: UpsertAgentProfileInput): Promise<AgentProfileView> {
  return (await request<{ agent_profile: AgentProfileView }>(`/agent-profiles/${encodeURIComponent(profileId)}/versions`, { method: 'POST', body: input, mutating: true })).agent_profile;
}

export async function getAgentProfileVersion(profileId: string, version: string): Promise<AgentProfileView> {
  return (await request<{ agent_profile: AgentProfileView }>(`/agent-profiles/${encodeURIComponent(profileId)}/versions/${encodeURIComponent(version)}`)).agent_profile;
}

export async function updateAgentProfileVersion(profileId: string, version: string, input: UpsertAgentProfileInput): Promise<AgentProfileView> {
  return (await request<{ agent_profile: AgentProfileView }>(`/agent-profiles/${encodeURIComponent(profileId)}/versions/${encodeURIComponent(version)}`, { method: 'PUT', body: input, mutating: true })).agent_profile;
}

export async function deleteAgentProfileVersion(profileId: string, version: string): Promise<AgentProfileView> {
  return (await request<{ agent_profile: AgentProfileView }>(`/agent-profiles/${encodeURIComponent(profileId)}/versions/${encodeURIComponent(version)}`, { method: 'DELETE', mutating: true })).agent_profile;
}

export type ListSessionsOptions = {
  includeArchived?: boolean;
  limit?: number;
  includePinned?: boolean;
};

export async function listSessions(options: ListSessionsOptions = {}): Promise<SessionView[]> {
  const query = new URLSearchParams();
  if (options.includeArchived) query.set('include_archived', 'true');
  if (options.limit !== undefined) query.set('limit', String(options.limit));
  if (options.includePinned) query.set('include_pinned', 'true');
  const queryString = query.toString() ? `?${query.toString()}` : '';
  return (await request<{ sessions: SessionView[] }>(`/sessions${queryString}`)).sessions;
}

export async function listWorkspaces(options: ReadRequestOptions = {}): Promise<WorkspaceView[]> {
  return (await request<{ workspaces: WorkspaceView[] }>('/workspaces', options)).workspaces;
}

export async function getWorkspace(workspaceId: string): Promise<WorkspaceView> {
  return (await request<{ workspace: WorkspaceView }>(`/workspaces/${workspaceId}`)).workspace;
}

export async function registerWorkspace(input: RegisterWorkspaceInput): Promise<WorkspaceView> {
  return (await request<{ workspace: WorkspaceView }>('/workspaces', { method: 'POST', body: input, mutating: true })).workspace;
}

export async function renameWorkspace(workspaceId: string, input: RenameWorkspaceInput): Promise<WorkspaceView> {
  return (await request<{ workspace: WorkspaceView }>(`/workspaces/${encodeURIComponent(workspaceId)}`, { method: 'PATCH', body: input, mutating: true })).workspace;
}

export async function deleteWorkspace(workspaceId: string): Promise<WorkspaceView> {
  return (await request<{ workspace: WorkspaceView }>(`/workspaces/${encodeURIComponent(workspaceId)}`, { method: 'DELETE', mutating: true })).workspace;
}

export async function getWorkspaceGitStatus(workspaceId: string, options: ReadRequestOptions = {}): Promise<WorkspaceGitStatusView> {
  return (await request<{ git_status: WorkspaceGitStatusView }>(`/workspaces/${encodeURIComponent(workspaceId)}/git-status`, options)).git_status;
}

export async function refreshWorkspaceGitStatus(workspaceId: string): Promise<WorkspaceGitStatusView> {
  return (await request<{ git_status: WorkspaceGitStatusView }>(`/workspaces/${encodeURIComponent(workspaceId)}/git-status/refresh`, { method: 'POST', mutating: true })).git_status;
}

export async function listWorkspaceRoots(options: ReadRequestOptions = {}): Promise<WorkspaceRootView[]> {
  return (await request<{ roots: WorkspaceRootView[] }>('/workspace-roots', options)).roots;
}

export async function listWorkspaceRootEntries(rootId: string, path = '', options: ReadRequestOptions = {}): Promise<WorkspaceDirectoryListingView> {
  const query = path ? `?path=${encodeURIComponent(path)}` : '';
  return request<WorkspaceDirectoryListingView>(`/workspace-roots/${encodeURIComponent(rootId)}/entries${query}`, options);
}

export async function listWorkspaceFilePickerEntries(
  workspaceId: string,
  query = '',
  options: ReadRequestOptions & { limit?: number } = {},
): Promise<FilePickerResultView> {
  const params = new URLSearchParams();
  if (query) params.set('query', query);
  if (options.limit !== undefined) params.set('limit', String(options.limit));
  const path = `/workspaces/${encodeURIComponent(workspaceId)}/file-picker${params.toString() ? `?${params.toString()}` : ''}`;
  return request<FilePickerResultView>(path, { signal: options.signal });
}

export async function listTasks(): Promise<TaskView[]> {
  return (await request<{ tasks: TaskView[] }>('/tasks')).tasks;
}

export async function getTask(taskId: string): Promise<TaskView> {
  return (await request<{ task: TaskView }>(`/tasks/${taskId}`)).task;
}

export async function listTaskEvents(taskId: string): Promise<TaskEventView[]> {
  return (await request<{ events: TaskEventView[] }>(`/tasks/${taskId}/events`)).events;
}

export async function interruptTask(taskId: string): Promise<TaskView> {
  return (await request<{ task: TaskView }>(`/tasks/${taskId}/interrupt`, { method: 'POST', mutating: true })).task;
}

export async function cancelTask(taskId: string): Promise<TaskView> {
  return (await request<{ task: TaskView }>(`/tasks/${taskId}/cancel`, { method: 'POST', mutating: true })).task;
}

export async function createSession(input: CreateSessionInput): Promise<CreateSessionResult> {
  return request<CreateSessionResult>('/sessions', { method: 'POST', body: input, mutating: true });
}

export async function updateSession(sessionId: string, input: UpdateSessionInput): Promise<SessionView> {
  return (await request<{ session: SessionView }>(`/sessions/${encodeURIComponent(sessionId)}`, { method: 'PATCH', body: input, mutating: true })).session;
}

export async function pinSession(sessionId: string): Promise<SessionView> {
  return (await request<{ session: SessionView }>(`/sessions/${encodeURIComponent(sessionId)}/pin`, { method: 'POST', mutating: true })).session;
}

export async function unpinSession(sessionId: string): Promise<SessionView> {
  return (await request<{ session: SessionView }>(`/sessions/${encodeURIComponent(sessionId)}/unpin`, { method: 'POST', mutating: true })).session;
}

export async function archiveSession(sessionId: string): Promise<SessionView> {
  return (await request<{ session: SessionView }>(`/sessions/${encodeURIComponent(sessionId)}/archive`, { method: 'POST', mutating: true })).session;
}

export async function getSession(sessionId: string): Promise<SessionView> {
  return (await request<{ session: SessionView }>(`/sessions/${sessionId}`)).session;
}

// GET /sessions/:id/turns is read-only turn history. WebUI dispatch must use
// submitInboxMessage(); hook/internal events remain authoritative for turn lifecycle facts.
export async function listTurns(sessionId: string): Promise<TurnView[]> {
  return (await request<{ turns: TurnView[] }>(`/sessions/${sessionId}/turns`)).turns;
}

export async function listInboxMessages(sessionId: string): Promise<InboxMessageView[]> {
  return (await request<{ inbox_messages: InboxMessageView[] }>(`/sessions/${sessionId}/inbox/messages`)).inbox_messages;
}

export async function submitInboxMessage(sessionId: string, input: SubmitInboxMessageInput): Promise<InboxMessageView> {
  return (await request<{ inbox_message: InboxMessageView }>(`/sessions/${sessionId}/inbox/messages`, { method: 'POST', body: input, mutating: true })).inbox_message;
}

export async function cancelInboxMessage(sessionId: string, messageId: string): Promise<InboxMessageView> {
  return (await request<{ inbox_message: InboxMessageView }>(`/sessions/${encodeURIComponent(sessionId)}/inbox/messages/${encodeURIComponent(messageId)}/cancel`, { method: 'POST', mutating: true })).inbox_message;
}

export async function dismissInboxMessage(sessionId: string, messageId: string): Promise<InboxMessageView> {
  return (await request<{ inbox_message: InboxMessageView }>(`/sessions/${encodeURIComponent(sessionId)}/inbox/messages/${encodeURIComponent(messageId)}/dismiss`, { method: 'POST', mutating: true })).inbox_message;
}

export async function listEvents(sessionId: string): Promise<EventView[]> {
  return (await request<{ events: EventView[] }>(`/sessions/${sessionId}/events`)).events;
}

export async function getTurnTimeline(
  sessionId: string,
  options: { direction: TurnTimelineDirection; turnId?: string | null; limit?: number; signal?: AbortSignal },
): Promise<TurnTimelinePage> {
  const params = new URLSearchParams({ direction: options.direction });
  if (options.turnId) params.set('turn_id', options.turnId);
  if (options.limit !== undefined) params.set('limit', String(options.limit));
  return request<TurnTimelinePage>(
    `/sessions/${encodeURIComponent(sessionId)}/turns/timeline?${params.toString()}`,
    { signal: options.signal },
  );
}

export async function interruptSession(sessionId: string): Promise<unknown> {
  return request(`/sessions/${sessionId}/interrupt`, { method: 'POST', mutating: true });
}

export async function restartSession(sessionId: string): Promise<unknown> {
  return request(`/sessions/${sessionId}/restart`, { method: 'POST', mutating: true });
}

export async function resumeSession(sessionId: string): Promise<unknown> {
  return request(`/sessions/${sessionId}/resume`, { method: 'POST', mutating: true });
}

export async function terminateSession(sessionId: string): Promise<unknown> {
  return request(`/sessions/${sessionId}`, { method: 'DELETE', mutating: true });
}
