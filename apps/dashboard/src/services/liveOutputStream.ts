import { get } from 'svelte/store';
import type { LiveOutputEvent } from '../lib/session-chat/liveOutput';
import { token } from '../stores/auth';
import { isAuthenticationFailure } from '../api/client';

const API_BASE = '/api/v1';
const RECONNECT_DELAY_MS = 1_000;

export interface LiveOutputStreamHandlers {
  onEvent: (event: LiveOutputEvent) => void;
  onDisconnected: () => void;
}

export function openLiveOutputStream(
  sessionId: string,
  handlers: LiveOutputStreamHandlers,
): () => void {
  let stopped = false;
  let controller: AbortController | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  const connect = async () => {
    const bearer = get(token).trim();
    if (stopped || !bearer) return;
    controller = new AbortController();
    try {
      const response = await fetch(
        `${API_BASE}/sessions/${encodeURIComponent(sessionId)}/live-output/stream`,
        {
          headers: { Authorization: `Bearer ${bearer}` },
          signal: controller.signal,
        },
      );
      if (stopped || controller.signal.aborted) return;
      if (!response.ok || !response.body) {
        if (isAuthenticationFailure(response.status)) {
          token.set('');
          return;
        }
        throw new Error(`Live output stream failed: ${response.status} ${response.statusText}`);
      }
      await readSse(response.body, handlers.onEvent);
      if (!stopped) scheduleReconnect();
    } catch (error) {
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

async function readSse(
  body: ReadableStream<Uint8Array>,
  onEvent: (event: LiveOutputEvent) => void,
): Promise<void> {
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

function parseFrame(frame: string, onEvent: (event: LiveOutputEvent) => void): void {
  const data = frame
    .split(/\r?\n/)
    .filter((line) => line.startsWith('data:'))
    .map((line) => line.slice(5).trimStart())
    .join('\n');
  if (!data) return;
  try {
    const event = JSON.parse(data) as LiveOutputEvent;
    if (event.type === 'snapshot' || event.type === 'updates' || event.type === 'closed') onEvent(event);
  } catch {
    // A reconnecting snapshot will recover from malformed or truncated frames.
  }
}
