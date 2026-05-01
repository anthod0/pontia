export type JsonObject = Record<string, unknown>;

export type SessionState = 'created' | 'starting' | 'idle' | 'busy' | 'interrupted' | 'exited' | 'error';
export type TurnState = 'queued' | 'running' | 'completed' | 'failed' | 'interrupted' | 'cancelled';

export interface SessionCapabilities {
  accept_task?: boolean;
  interrupt?: boolean;
  stream_output?: boolean;
  heartbeat?: boolean;
  artifact_sources?: boolean;
  [key: string]: unknown;
}

export interface SessionView {
  session_id: string;
  client_type: string;
  state: SessionState | string;
  current_turn_id: string | null;
  workspace: string;
  capabilities: SessionCapabilities;
  created_at: string;
  updated_at: string;
  metadata: JsonObject;
}

export interface TurnView {
  turn_id: string;
  session_id: string;
  state: TurnState | string;
  input: { summary?: string; artifact_id?: string | null; [key: string]: unknown } | null;
  output: { summary?: string; artifact_ids?: string[]; [key: string]: unknown } | null;
  failure: unknown | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  metadata: JsonObject;
}

export interface EventView {
  event_id: string;
  session_id: string;
  turn_id: string | null;
  source: string;
  type: string;
  time: string;
  payload: JsonObject;
}

export interface ArtifactView {
  artifact_id: string;
  session_id: string;
  turn_id: string | null;
  kind: string;
  name: string;
  size_bytes: number | null;
  preview: string | null;
  created_at: string;
  metadata: JsonObject;
}

export interface CreateSessionInput {
  client_type: string;
  workspace: string;
  metadata?: JsonObject;
  initial_task?: { input: string; metadata?: JsonObject } | null;
}

export interface CreateSessionResult {
  session: SessionView;
  initial_turn: TurnView | null;
}

export interface SubmitTurnInput {
  input: string;
  metadata?: JsonObject;
}

export interface ArtifactContent {
  artifactId: string;
  contentType: string;
  text: string;
  bytes: ArrayBuffer;
}

export interface ApiEnvelope<T> {
  data: T | null;
  meta?: JsonObject;
  error?: { code: string; message: string } | null;
}
