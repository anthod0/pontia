import type { DeviceRuntime } from "../device/runtime";
import type { BotMessage } from "../api/types";
import { readSse } from "./readSse";

export type BotUpdate =
  | { type: "typing" | "changed" | "reset" }
  | { type: "message"; message: BotMessage };

export function openBotChatStream(
  device: DeviceRuntime,
  id: string,
  onUpdate: (update: BotUpdate) => void,
  onDisconnect: () => void,
): () => void {
  const controller = new AbortController();
  const signal = device.access.apiSignal(controller.signal);
  let retry: ReturnType<typeof setTimeout>;

  async function connect(): Promise<void> {
    const headers = new Headers();
    if (signal.aborted || !device.access.applyApiAuthentication(headers)) return;
    try {
      const response = await device.access.apiFetch(
        device.access.apiUrl(`/api/v1/bots/${encodeURIComponent(id)}/stream?timeline=true`),
        { headers, credentials: device.access.apiCredentials, signal },
      );
      if (response.status === 401 || response.status === 403) {
        device.access.handleApiAuthenticationFailure();
        return;
      }
      if (response.status === 404) return;
      if (response.ok && response.body) {
        await readSse(response.body, (data) => {
          if (!signal.aborted) onUpdate(JSON.parse(data) as BotUpdate);
        });
      }
    } catch {
      // The next subscription starts with a chat refresh.
    }
    if (!signal.aborted) {
      onDisconnect();
      retry = setTimeout(() => void connect(), 1000);
    }
  }

  void connect();
  return () => {
    clearTimeout(retry);
    controller.abort();
  };
}
