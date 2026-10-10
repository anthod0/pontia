import { get } from "svelte/store";
import type { DashboardStreamEvent } from "../api/types";
import {
  apiCredentials,
  apiFetch,
  apiSignal,
  apiStreamUnavailableMessage,
  apiUrl,
  applyApiAuthentication,
  handleApiAuthenticationFailure,
} from "$dashboard-mode/apiAccess";
import {
  dashboardStreamCursor,
  lastConnectionError,
  reconnectCount,
  sseStatus,
  streamedSessionId,
} from "../stores/connection";
import { loadAgentProfiles } from "../stores/agentProfiles";
import { loadTasks, refreshTask, selectedTaskId } from "../stores/tasks";
import { loadSessionDetail, selectedSessionId } from "../stores/sessions";
import { invalidateQueriesForDashboardEvent } from "../queries/dashboardInvalidation";
import { loadWorkspaces } from "../stores/workspaces";
import {
  loadWorkflows,
  refreshWorkflow,
  selectedWorkflowId,
  selectedWorkflowSessionIds,
} from "../stores/workflows";
import { createDashboardRefreshScheduler } from "./dashboardRefreshScheduler";
import { isAuthenticationFailure } from "../api/client";
import { refreshDashboardSnapshot } from "./dashboardSnapshotRefresh";

const API_BASE = "/api/v1";

type DashboardEventListener = (event: DashboardStreamEvent) => void;
const dashboardEventListeners = new Set<DashboardEventListener>();

export function subscribeDashboardEvents(listener: DashboardEventListener): () => void {
  dashboardEventListeners.add(listener);
  return () => dashboardEventListeners.delete(listener);
}

const refreshScheduler = createDashboardRefreshScheduler({
  getSelectedTaskId: () => get(selectedTaskId),
  getSelectedSessionId: () => get(selectedSessionId),
  getSelectedWorkflowId: () => get(selectedWorkflowId),
  getSelectedWorkflowSessionIds: selectedWorkflowSessionIds,
  loadTasks,
  loadWorkspaces,
  loadAgentProfiles,
  loadWorkflows: () => loadWorkflows({ showLoading: false }),
  refreshTask,
  refreshSession: (sessionId) => loadSessionDetail(sessionId, { showLoading: false }),
  refreshWorkflow: (workflowId) => refreshWorkflow(workflowId, { showLoading: false }),
});

let controller: AbortController | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let generation = 0;
let started = false;
let lifecycleListenersAttached = false;
let lifecycleReconnectQueued = false;

function clearReconnectTimer(): void {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
}

function attachLifecycleListeners(): void {
  if (lifecycleListenersAttached || typeof window === "undefined") return;
  document.addEventListener("visibilitychange", handleVisibilityChange);
  window.addEventListener("online", queueLifecycleReconnect);
  window.addEventListener("pageshow", queueLifecycleReconnect);
  lifecycleListenersAttached = true;
}

function detachLifecycleListeners(): void {
  if (!lifecycleListenersAttached || typeof window === "undefined") return;
  document.removeEventListener("visibilitychange", handleVisibilityChange);
  window.removeEventListener("online", queueLifecycleReconnect);
  window.removeEventListener("pageshow", queueLifecycleReconnect);
  lifecycleListenersAttached = false;
}

function handleVisibilityChange(): void {
  if (document.visibilityState === "visible") queueLifecycleReconnect();
}

function queueLifecycleReconnect(): void {
  if (!started || lifecycleReconnectQueued) return;
  lifecycleReconnectQueued = true;
  const requestedGeneration = generation;
  queueMicrotask(() => {
    lifecycleReconnectQueued = false;
    if (!started || generation !== requestedGeneration) return;
    forceReconnect();
  });
}

function forceReconnect(): void {
  if (!started) return;
  const nextGeneration = ++generation;
  clearReconnectTimer();
  const previousController = controller;
  controller = null;
  previousController?.abort();
  reconnectCount.update((count) => count + 1);
  sseStatus.set("reconnecting");
  void connect(nextGeneration);
}

export function stopEventStream(): void {
  generation += 1;
  started = false;
  lifecycleReconnectQueued = false;
  detachLifecycleListeners();
  clearReconnectTimer();
  controller?.abort();
  controller = null;
  streamedSessionId.set(null);
  refreshScheduler.reset();
  sseStatus.set("closed");
}

export function startEventStream(): void {
  if (started) return;
  started = true;
  reconnectCount.set(0);
  lastConnectionError.set(null);
  streamedSessionId.set("dashboard");
  attachLifecycleListeners();
  void connect(generation);
}

async function connect(streamGeneration: number): Promise<void> {
  if (!started || streamGeneration !== generation) return;
  const headers = new Headers();
  if (!applyApiAuthentication(headers)) {
    sseStatus.set("idle");
    lastConnectionError.set(apiStreamUnavailableMessage);
    started = false;
    detachLifecycleListeners();
    streamedSessionId.set(null);
    return;
  }

  const localController = new AbortController();
  controller = localController;
  sseStatus.set(get(reconnectCount) > 0 ? "reconnecting" : "connecting");
  lastConnectionError.set(null);

  try {
    const after = get(dashboardStreamCursor);
    const query = after ? `?after=${encodeURIComponent(after)}` : "";
    const response = await apiFetch(apiUrl(`${API_BASE}/dashboard/events/stream${query}`), {
      headers,
      credentials: apiCredentials,
      signal: apiSignal(localController.signal),
    });

    if (localController.signal.aborted || streamGeneration !== generation || !started) return;

    if (!response.ok || !response.body) {
      if (isAuthenticationFailure(response.status)) {
        handleApiAuthenticationFailure();
        stopEventStream();
        return;
      }
      if (after && (response.status === 409 || response.status === 410)) {
        dashboardStreamCursor.set(null);
        await refreshDashboardSnapshot({ reason: "sse_fallback" });
      }
      throw new Error(`Dashboard event stream failed: ${response.status} ${response.statusText}`);
    }

    sseStatus.set("open");
    void refreshDashboardSnapshot({ reason: "sse_open" });
    await readSse(response.body, (event, cursor) => {
      if (streamGeneration === generation) handleDashboardEvent(event, cursor);
    });

    if (streamGeneration === generation && started) scheduleReconnect(streamGeneration);
  } catch (error) {
    if (localController.signal.aborted || streamGeneration !== generation) return;
    lastConnectionError.set(error instanceof Error ? error.message : String(error));
    sseStatus.set("error");
    scheduleReconnect(streamGeneration);
  }
}

function scheduleReconnect(streamGeneration: number): void {
  if (!started || streamGeneration !== generation) return;
  reconnectCount.update((count) => count + 1);
  const delay = Math.min(1000 + get(reconnectCount) * 500, 5000);
  sseStatus.set("reconnecting");
  clearReconnectTimer();
  reconnectTimer = setTimeout(() => connect(streamGeneration), delay);
}

async function readSse(
  body: ReadableStream<Uint8Array>,
  onEvent: (event: DashboardStreamEvent, id: string | null) => void,
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let boundary = buffer.search(/\r?\n\r?\n/);
    while (boundary !== -1) {
      const frame = buffer.slice(0, boundary);
      buffer = buffer.slice(buffer[boundary] === "\r" ? boundary + 4 : boundary + 2);
      parseFrame(frame, onEvent);
      boundary = buffer.search(/\r?\n\r?\n/);
    }
  }
}

function parseFrame(
  frame: string,
  onEvent: (event: DashboardStreamEvent, id: string | null) => void,
): void {
  const dataLines: string[] = [];
  let id: string | null = null;
  for (const line of frame.split(/\r?\n/)) {
    if (line.startsWith("id:")) id = line.slice(3).trimStart();
    if (line.startsWith("data:")) dataLines.push(line.slice(5).trimStart());
  }
  if (!dataLines.length) return;
  try {
    onEvent(JSON.parse(dataLines.join("\n")) as DashboardStreamEvent, id);
  } catch (error) {
    lastConnectionError.set(error instanceof Error ? error.message : String(error));
  }
}

function handleDashboardEvent(streamEvent: DashboardStreamEvent, cursor: string | null): void {
  if (cursor) dashboardStreamCursor.set(cursor);
  void invalidateQueriesForDashboardEvent(streamEvent);
  refreshScheduler.handleEvent(streamEvent);
  for (const listener of dashboardEventListeners) listener(streamEvent);
}
