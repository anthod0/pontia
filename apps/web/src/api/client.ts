import { get } from 'svelte/store';
import { token } from '../stores/auth';
import { ApiError } from './errors';
import type {
  ApiEnvelope,
  ArtifactContent,
  ArtifactView,
  CreateSessionInput,
  CreateSessionResult,
  EventView,
  SessionView,
  SubmitTurnInput,
  TurnView,
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

export async function listSessions(): Promise<SessionView[]> {
  return (await request<{ sessions: SessionView[] }>('/sessions')).sessions;
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

export async function submitTurn(sessionId: string, input: SubmitTurnInput): Promise<TurnView> {
  return (await request<{ turn: TurnView }>(`/sessions/${sessionId}/turns`, { method: 'POST', body: input, mutating: true })).turn;
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
  const bytes = await response.arrayBuffer();
  if (!response.ok) throw new ApiError(response.statusText, 'request_failed', response.status);
  const contentType = response.headers.get('content-type') ?? 'application/octet-stream';
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
