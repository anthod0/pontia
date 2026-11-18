import type { LiveOutputEvent } from "../lib/session-chat/liveOutput";
import { isAuthenticationFailure } from "../api/client";
import type { DeviceRuntime } from "../device/runtime";
import { readSse } from "./readSse";

export interface LiveOutputStreamHandlers {
  onEvent: (event: LiveOutputEvent) => void;
  onDisconnected: () => void;
}
export function createLiveOutput(device: DeviceRuntime) {
  const API_BASE = "/api/v1";

  const RECONNECT_DELAY_MS = 1_000;

  function openLiveOutputStream(sessionId: string, handlers: LiveOutputStreamHandlers): () => void {
    let stopped = false;
    let controller: AbortController | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    const connect = async () => {
      const headers = new Headers();
      if (stopped || !device.access.applyApiAuthentication(headers)) return;
      controller = new AbortController();
      try {
        const response = await device.access.apiFetch(
          device.access.apiUrl(
            `${API_BASE}/sessions/${encodeURIComponent(sessionId)}/live-output/stream`,
          ),
          {
            headers,
            credentials: device.access.apiCredentials,
            signal: device.access.apiSignal(controller.signal),
          },
        );
        if (stopped || controller.signal.aborted) return;
        if (!response.ok || !response.body) {
          if (isAuthenticationFailure(response.status)) {
            device.access.handleApiAuthenticationFailure();
            return;
          }
          throw new Error(`Live output stream failed: ${response.status} ${response.statusText}`);
        }
        await readSse(response.body, (data) => handleFrame(data, handlers.onEvent));
        if (!stopped) scheduleReconnect();
      } catch {
        if (stopped || controller?.signal.aborted) return;
        scheduleReconnect();
      }
    };

    const scheduleReconnect = () => {
      if (stopped || reconnectTimer) return;
      handlers.onDisconnected();
      reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        void connect();
      }, RECONNECT_DELAY_MS);
    };

    void connect();
    return () => {
      stopped = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = null;
      controller?.abort();
      controller = null;
    };
  }

  function handleFrame(data: string, onEvent: (event: LiveOutputEvent) => void): void {
    try {
      const event = JSON.parse(data) as LiveOutputEvent;
      if (event.type === "snapshot" || event.type === "updates" || event.type === "closed")
        onEvent(event);
    } catch {
      // A reconnecting snapshot will recover from malformed or truncated frames.
    }
  }
  return { openLiveOutputStream };
}
