import { get } from "svelte/store";

import type { DashboardStreamEvent } from "../api/types";

import { createDashboardRefreshScheduler } from "./dashboardRefreshScheduler";
import { isAuthenticationFailure } from "../api/client";
import type { DeviceRuntime } from "../device/runtime";

type DashboardEventListener = (event: DashboardStreamEvent) => void;
export function createEvents(device: DeviceRuntime) {
  const API_BASE = "/api/v1";

  const dashboardEventListeners = new Set<DashboardEventListener>();

  function subscribeDashboardEvents(listener: DashboardEventListener): () => void {
    dashboardEventListeners.add(listener);
    return () => dashboardEventListeners.delete(listener);
  }

  const refreshScheduler = createDashboardRefreshScheduler({
    getSelectedSessionId: () => get(device.sessions.selectedSessionId),
    getSelectedWorkflowId: () => get(device.workflows.selectedWorkflowId),
    getSelectedWorkflowSessionIds: device.workflows.selectedWorkflowSessionIds,
    loadWorkflows: device.workflowQueries.invalidateWorkflowLists,
    refreshSession: (sessionId) =>
      device.sessions.loadSessionDetail(sessionId, { showLoading: false }),
    refreshWorkflow: device.workflowQueries.invalidateWorkflow,
  });

  let controller: AbortController | null = null;

  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  let generation = 0;

  let started = false;

  let onDisconnected: (() => void) | undefined;

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
    device.connection.reconnectCount.update((count) => count + 1);
    device.connection.sseStatus.set("reconnecting");
    void connect(nextGeneration);
  }

  function stopEventStream(): void {
    generation += 1;
    started = false;
    lifecycleReconnectQueued = false;
    detachLifecycleListeners();
    clearReconnectTimer();
    controller?.abort();
    controller = null;
    device.connection.streamedSessionId.set(null);
    refreshScheduler.reset();
    device.connection.sseStatus.set("closed");
  }

  function startEventStream(disconnected?: () => void): void {
    if (started) return;
    started = true;
    onDisconnected = disconnected;
    device.connection.reconnectCount.set(0);
    device.connection.lastConnectionError.set(null);
    device.connection.streamedSessionId.set("dashboard");
    if (!onDisconnected) attachLifecycleListeners();
    void connect(generation);
  }

  async function connect(streamGeneration: number): Promise<void> {
    if (!started || streamGeneration !== generation) return;
    const headers = new Headers();
    if (!device.access.applyApiAuthentication(headers)) {
      device.connection.sseStatus.set("idle");
      device.connection.lastConnectionError.set(device.access.apiStreamUnavailableMessage);
      started = false;
      detachLifecycleListeners();
      device.connection.streamedSessionId.set(null);
      return;
    }

    const localController = new AbortController();
    controller = localController;
    device.connection.sseStatus.set(
      get(device.connection.reconnectCount) > 0 ? "reconnecting" : "connecting",
    );
    device.connection.lastConnectionError.set(null);

    try {
      const after = get(device.connection.dashboardStreamCursor);
      const query = after ? `?after=${encodeURIComponent(after)}` : "";
      const response = await device.access.apiFetch(
        device.access.apiUrl(`${API_BASE}/dashboard/events/stream${query}`),
        {
          headers,
          credentials: device.access.apiCredentials,
          signal: device.access.apiSignal(localController.signal),
        },
      );

      if (localController.signal.aborted || streamGeneration !== generation || !started) return;

      if (!response.ok || !response.body) {
        if (isAuthenticationFailure(response.status)) {
          device.access.handleApiAuthenticationFailure();
          stopEventStream();
          onDisconnected?.();
          return;
        }
        if (after && (response.status === 409 || response.status === 410)) {
          device.connection.dashboardStreamCursor.set(null);
          await device.snapshotRefresh.refreshDashboardSnapshot({ reason: "sse_fallback" });
        }
        throw new Error(`Dashboard event stream failed: ${response.status} ${response.statusText}`);
      }

      device.connection.sseStatus.set("open");
      void device.snapshotRefresh.refreshDashboardSnapshot({ reason: "sse_open" });
      await readSse(response.body, (event, cursor) => {
        if (streamGeneration === generation) handleDashboardEvent(event, cursor);
      });

      if (streamGeneration === generation && started) scheduleReconnect(streamGeneration);
    } catch (error) {
      if (localController.signal.aborted || streamGeneration !== generation) return;
      device.connection.lastConnectionError.set(
        error instanceof Error ? error.message : String(error),
      );
      device.connection.sseStatus.set("error");
      scheduleReconnect(streamGeneration);
    }
  }

  function scheduleReconnect(streamGeneration: number): void {
    if (!started || streamGeneration !== generation) return;
    if (onDisconnected) {
      onDisconnected();
      return;
    }
    device.connection.reconnectCount.update((count) => count + 1);
    const delay = Math.min(1000 + get(device.connection.reconnectCount) * 500, 5000);
    device.connection.sseStatus.set("reconnecting");
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
      device.connection.lastConnectionError.set(
        error instanceof Error ? error.message : String(error),
      );
    }
  }

  function handleDashboardEvent(streamEvent: DashboardStreamEvent, cursor: string | null): void {
    if (cursor) device.connection.dashboardStreamCursor.set(cursor);
    device.inboxQueries.reconcileInboxSubmissionForDashboardEvent(streamEvent);
    void device.invalidation.invalidateQueriesForDashboardEvent(streamEvent);
    refreshScheduler.handleEvent(streamEvent);
    for (const listener of dashboardEventListeners) listener(streamEvent);
  }
  return { subscribeDashboardEvents, stopEventStream, startEventStream };
}
