import { get } from 'svelte/store';
import { token } from '../stores/auth';
import { ApiError } from './errors';
import type {
  AgentProfileView,
  ApiEnvelope,
  ArtifactContent,
  ArtifactView,
  CreateDagTaskInput,
  CreateDagTaskResult,
  CreateSessionInput,
  CreateSessionResult,
  EventView,
  DagProposalView,
  HumanSignalInput,
  InboxMessageView,
  RegisterWorkspaceInput,
  RenameWorkspaceInput,
  SessionView,
  SubmitInboxMessageInput,
  TimelineItemDetail,
  TimelinePage,
  TimelineUpdatesPage,
  UpsertAgentProfileInput,
  TaskDagView,
  TaskEventView,
  TaskView,
  DagSignalView,
  TurnView,
  UpdateSessionInput,
  WorkspaceDirectoryListingView,
  WorkspaceGitStatusView,
  WorkspaceRootView,
  WorkspaceView,
} from './types';

const API_BASE = '/external/v1';

type RequestOptions = Omit<RequestInit, 'body'> & { body?: unknown; mutating?: boolean };
export type ReadRequestOptions = Pick<RequestOptions, 'signal'>;

function idempotencyKey(): string {
  return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export async function validateExternalApiToken(candidateToken: string): Promise<void> {
  const headers = new Headers();
  headers.set('Authorization', `Bearer ${candidateToken}`);
  const response = await fetch(`${API_BASE}/auth/validate`, { headers });
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

  const response = await fetch(`${API_BASE}${path}`, {
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

export async function listSessions(): Promise<SessionView[]> {
  return (await request<{ sessions: SessionView[] }>('/sessions')).sessions;
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

export async function listTasks(): Promise<TaskView[]> {
  return (await request<{ tasks: TaskView[] }>('/tasks')).tasks;
}

export async function createDagTask(input: CreateDagTaskInput): Promise<CreateDagTaskResult> {
  return request<CreateDagTaskResult>('/dag-tasks', { method: 'POST', body: input, mutating: true });
}

export async function getTask(taskId: string): Promise<TaskView> {
  return (await request<{ task: TaskView }>(`/tasks/${taskId}`)).task;
}

export async function listTaskEvents(taskId: string): Promise<TaskEventView[]> {
  return (await request<{ events: TaskEventView[] }>(`/tasks/${taskId}/events`)).events;
}

export async function listTaskProposals(taskId: string): Promise<DagProposalView[]> {
  return (await request<{ proposals: DagProposalView[] }>(`/tasks/${taskId}/proposals`)).proposals;
}

export async function getTaskDag(taskId: string): Promise<TaskDagView> {
  return (await request<{ dag: TaskDagView }>(`/tasks/${taskId}/dag`)).dag;
}

export async function pauseTask(taskId: string): Promise<TaskView> {
  return (await request<{ task: TaskView }>(`/tasks/${taskId}/pause`, { method: 'POST', mutating: true })).task;
}

export async function resumeTask(taskId: string): Promise<{ task: TaskView; scheduler: unknown }> {
  return request<{ task: TaskView; scheduler: unknown }>(`/tasks/${taskId}/resume`, { method: 'POST', mutating: true });
}

export async function createHumanSignal(taskId: string, input: HumanSignalInput): Promise<DagSignalView> {
  return (await request<{ signal: DagSignalView }>(`/tasks/${taskId}/signals`, { method: 'POST', body: input, mutating: true })).signal;
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

export async function getSession(sessionId: string): Promise<SessionView> {
  return (await request<{ session: SessionView }>(`/sessions/${sessionId}`)).session;
}

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

export async function getSessionTimeline(
  sessionId: string,
  options: { olderCursor?: string | null; limit?: number; signal?: AbortSignal } = {},
): Promise<TimelinePage> {
  const params = new URLSearchParams();
  if (options.olderCursor) params.set('older_cursor', options.olderCursor);
  if (options.limit !== undefined) params.set('limit', String(options.limit));
  const query = params.toString() ? `?${params.toString()}` : '';
  return request<TimelinePage>(`/sessions/${encodeURIComponent(sessionId)}/timeline${query}`, { signal: options.signal });
}

export async function getSessionTimelineUpdates(
  sessionId: string,
  options: { afterItemId: string; signal?: AbortSignal },
): Promise<TimelineUpdatesPage> {
  const query = `?after_item_id=${encodeURIComponent(options.afterItemId)}`;
  return request<TimelineUpdatesPage>(`/sessions/${encodeURIComponent(sessionId)}/timeline/updates${query}`, { signal: options.signal });
}

export async function getTimelineItemDetail(
  sessionId: string,
  contentRef: string,
  options: ReadRequestOptions = {},
): Promise<TimelineItemDetail> {
  return request<TimelineItemDetail>(
    `/sessions/${encodeURIComponent(sessionId)}/timeline/detail?ref=${encodeURIComponent(contentRef)}`,
    options,
  );
}

export async function listArtifacts(sessionId: string): Promise<ArtifactView[]> {
  return (await request<{ artifacts: ArtifactView[] }>(`/sessions/${sessionId}/artifacts`)).artifacts;
}

export async function discoverArtifacts(sessionId: string): Promise<ArtifactView[]> {
  return (await request<{ artifacts: ArtifactView[] }>(`/sessions/${sessionId}/artifacts/discover`, { method: 'POST', mutating: true })).artifacts;
}

export async function getArtifactContent(artifactId: string): Promise<ArtifactContent> {
  const headers = new Headers();
  const bearer = get(token).trim();
  if (bearer) headers.set('Authorization', `Bearer ${bearer}`);
  const response = await fetch(`${API_BASE}/artifacts/${artifactId}/content`, { headers });
  const contentType = response.headers.get('content-type') ?? 'application/octet-stream';
  const bytes = await response.arrayBuffer();
  if (!response.ok) {
    const text = new TextDecoder().decode(bytes);
    try {
      const envelope = JSON.parse(text) as ApiEnvelope<unknown>;
      throw new ApiError(
        envelope.error?.message ?? response.statusText,
        envelope.error?.code ?? 'request_failed',
        response.status,
      );
    } catch (error) {
      if (error instanceof ApiError) throw error;
      throw new ApiError(text || response.statusText, 'request_failed', response.status);
    }
  }
  return { artifactId, contentType, bytes, text: new TextDecoder().decode(bytes) };
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
