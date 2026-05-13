import { get } from 'svelte/store';
import type { DashboardStreamEvent, EventView, TaskEventView } from '../api/types';
import { token } from '../stores/auth';
import {
  dashboardStreamCursor,
  lastConnectionError,
  reconnectCount,
  sseStatus,
  streamedSessionId,
} from '../stores/connection';
import { appendEvent } from '../stores/events';
import { selectedSessionId } from '../stores/selection';
import { selectedTaskId } from '../stores/tasks';
import {
  refreshArtifacts,
  refreshSelectedSession,
  refreshSelectedTask,
  refreshSessionList,
  refreshTaskList,
  refreshTurns,
} from './refreshCoordinator';

const API_BASE = '/external/v1';
const TERMINAL_TURN_EVENTS = new Set(['turn.completed', 'turn.failed', 'turn.interrupted', 'turn.cancelled']);
const SESSION_STATE_EVENTS = new Set(['session.ready', 'session.started', 'session.exited', 'session.error']);

let controller: AbortController | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let generation = 0;
let started = false;

function clearReconnectTimer(): void {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
}

export function stopEventStream(): void {
  generation += 1;
  started = false;
  clearReconnectTimer();
  controller?.abort();
  controller = null;
  streamedSessionId.set(null);
  sseStatus.set('closed');
}

export function startEventStream(): void {
  if (started) return;
  started = true;
  reconnectCount.set(0);
  lastConnectionError.set(null);
  streamedSessionId.set('dashboard');
  void connect(generation);
}

async function connect(streamGeneration: number): Promise<void> {
  const bearer = get(token).trim();
  if (!bearer) {
    sseStatus.set('idle');
    lastConnectionError.set('Set an API token to open the dashboard event stream.');
    return;
  }

  controller = new AbortController();
  sseStatus.set(get(reconnectCount) > 0 ? 'reconnecting' : 'connecting');
  lastConnectionError.set(null);

  try {
    const after = get(dashboardStreamCursor);
    const query = after ? `?after=${encodeURIComponent(after)}` : '';
    const response = await fetch(`${API_BASE}/dashboard/events/stream${query}`, {
      headers: { Authorization: `Bearer ${bearer}` },
      signal: controller.signal,
    });

    if (!response.ok || !response.body) throw new Error(`Dashboard event stream failed: ${response.status} ${response.statusText}`);
    sseStatus.set('open');
    await readSse(response.body, handleDashboardEvent);

    if (streamGeneration === generation && started) scheduleReconnect(streamGeneration);
  } catch (error) {
    if (controller?.signal.aborted || streamGeneration !== generation) return;
    lastConnectionError.set(error instanceof Error ? error.message : String(error));
    sseStatus.set('error');
    scheduleReconnect(streamGeneration);
  }
}

function scheduleReconnect(streamGeneration: number): void {
  if (!started || streamGeneration !== generation) return;
  reconnectCount.update((count) => count + 1);
  const delay = Math.min(1000 + get(reconnectCount) * 500, 5000);
  sseStatus.set('reconnecting');
  clearReconnectTimer();
  reconnectTimer = setTimeout(() => connect(streamGeneration), delay);
}

async function readSse(body: ReadableStream<Uint8Array>, onEvent: (event: DashboardStreamEvent, id: string | null) => void): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let boundary = buffer.search(/\r?\n\r?\n/);
    while (boundary !== -1) {
      const frame = buffer.slice(0, boundary);
      buffer = buffer.slice(buffer[boundary] === '\r' ? boundary + 4 : boundary + 2);
      parseFrame(frame, onEvent);
      boundary = buffer.search(/\r?\n\r?\n/);
    }
  }
}

function parseFrame(frame: string, onEvent: (event: DashboardStreamEvent, id: string | null) => void): void {
  const dataLines: string[] = [];
  let id: string | null = null;
  for (const line of frame.split(/\r?\n/)) {
    if (line.startsWith('id:')) id = line.slice(3).trimStart();
    if (line.startsWith('data:')) dataLines.push(line.slice(5).trimStart());
  }
  if (!dataLines.length) return;
  try {
    onEvent(JSON.parse(dataLines.join('\n')) as DashboardStreamEvent, id);
  } catch (error) {
    lastConnectionError.set(error instanceof Error ? error.message : String(error));
  }
}

function handleDashboardEvent(streamEvent: DashboardStreamEvent, cursor: string | null): void {
  if (cursor) dashboardStreamCursor.set(cursor);
  if (streamEvent.kind === 'session_event') {
    handleSessionEvent(streamEvent.event);
  } else {
    handleTaskEvent(streamEvent.event);
  }
}

function handleSessionEvent(event: EventView): void {
  const isSelectedSession = event.session_id === get(selectedSessionId);
  if (isSelectedSession && !appendEvent(event)) return;

  if (event.type === 'turn.output') {
    if (isSelectedSession) refreshTurns();
  } else if (TERMINAL_TURN_EVENTS.has(event.type)) {
    if (isSelectedSession) {
      refreshSelectedSession();
      refreshTurns();
    }
  } else if (SESSION_STATE_EVENTS.has(event.type)) {
    if (isSelectedSession) refreshSelectedSession();
    refreshSessionList();
  } else if (event.type.startsWith('artifact.')) {
    if (isSelectedSession) refreshArtifacts();
  }
}

function handleTaskEvent(event: TaskEventView): void {
  refreshTaskList();
  if (event.task_id === get(selectedTaskId)) refreshSelectedTask();
}
