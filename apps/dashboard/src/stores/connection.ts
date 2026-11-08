import { writable } from "svelte/store";
import type { DeviceRuntime } from "../device/runtime";

export type SseStatus = "idle" | "connecting" | "open" | "reconnecting" | "closed" | "error";
export function createConnection(device: DeviceRuntime) {
  const sseStatus = writable<SseStatus>("idle");

  const reconnectCount = writable(0);

  const lastConnectionError = writable<string | null>(null);

  const streamedSessionId = writable<string | null>(null);

  const dashboardStreamCursor = writable<string | null>(null);

  function resetConnectionState(): void {
    sseStatus.set("idle");
    reconnectCount.set(0);
    lastConnectionError.set(null);
    streamedSessionId.set(null);
    dashboardStreamCursor.set(null);
  }
  return {
    sseStatus,
    reconnectCount,
    lastConnectionError,
    streamedSessionId,
    dashboardStreamCursor,
    resetConnectionState,
  };
}
