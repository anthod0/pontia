import { get } from 'svelte/store';
import { token } from '../stores/auth';
import { ApiError } from './errors';
import type {
  AgentProfileView,
  ApiEnvelope,
  ArtifactContent,
  ArtifactView,
  ConfirmTaskWorkspaceInput,
  CreateSessionInput,
  CreateSessionResult,
  CreateTaskInput,
  EventView,
  InboxMessageView,
  RegisterWorkspaceInput,
  SessionView,
  SubmitInboxMessageInput,
  SubmitPlannerInput,
  TaskEventView,
  TaskView,
  TurnView,
  WorkspaceDirectoryListingView,
  WorkspaceRootView,
  WorkspaceView,
} from './types';

const API_BASE = '/external/v1';

type RequestOptions = Omit<RequestInit, 'body'> & { body?: unknown; mutating?: boolean };

function idempotencyKey(): string {
  return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
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

export async function listAgentProfiles(): Promise<AgentProfileView[]> {
  return (await request<{ agent_profiles: AgentProfileView[] }>('/agent-profiles')).agent_profiles;
}

export async function getAgentProfile(profileId: string): Promise<AgentProfileView> {
  return (await request<{ agent_profile: AgentProfileView }>(`/agent-profiles/${encodeURIComponent(profileId)}`)).agent_profile;
}

export async function listSessions(): Promise<SessionView[]> {
  return (await request<{ sessions: SessionView[] }>('/sessions')).sessions;
}

export async function listWorkspaces(): Promise<WorkspaceView[]> {
  return (await request<{ workspaces: WorkspaceView[] }>('/workspaces')).workspaces;
}

export async function getWorkspace(workspaceId: string): Promise<WorkspaceView> {
  return (await request<{ workspace: WorkspaceView }>(`/workspaces/${workspaceId}`)).workspace;
}

export async function registerWorkspace(input: RegisterWorkspaceInput): Promise<WorkspaceView> {
  return (await request<{ workspace: WorkspaceView }>('/workspaces', { method: 'POST', body: input, mutating: true })).workspace;
}

export async function listWorkspaceRoots(): Promise<WorkspaceRootView[]> {
  return (await request<{ roots: WorkspaceRootView[] }>('/workspace-roots')).roots;
}

export async function listWorkspaceRootEntries(rootId: string, path = ''): Promise<WorkspaceDirectoryListingView> {
  const query = path ? `?path=${encodeURIComponent(path)}` : '';
  return request<WorkspaceDirectoryListingView>(`/workspace-roots/${encodeURIComponent(rootId)}/entries${query}`);
}

export async function listTasks(): Promise<TaskView[]> {
  return (await request<{ tasks: TaskView[] }>('/tasks')).tasks;
}

export async function createTask(input: CreateTaskInput): Promise<TaskView> {
  return (await request<{ task: TaskView }>('/tasks', { method: 'POST', body: input, mutating: true })).task;
}

export async function getTask(taskId: string): Promise<TaskView> {
  return (await request<{ task: TaskView }>(`/tasks/${taskId}`)).task;
}

export async function confirmTaskWorkspace(taskId: string, input: ConfirmTaskWorkspaceInput): Promise<TaskView> {
  return (await request<{ task: TaskView }>(`/tasks/${taskId}/confirm-workspace`, { method: 'POST', body: input, mutating: true })).task;
}

export async function submitPlannerInput(taskId: string, input: SubmitPlannerInput): Promise<TaskView> {
  return (await request<{ task: TaskView }>(`/tasks/${taskId}/planner-input`, { method: 'POST', body: input, mutating: true })).task;
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

export async function listEvents(sessionId: string): Promise<EventView[]> {
  return (await request<{ events: EventView[] }>(`/sessions/${sessionId}/events`)).events;
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

export async function terminateSession(sessionId: string): Promise<unknown> {
  return request(`/sessions/${sessionId}`, { method: 'DELETE', mutating: true });
}
