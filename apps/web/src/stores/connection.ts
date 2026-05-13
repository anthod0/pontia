import { writable } from 'svelte/store';

export type SseStatus = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'closed' | 'error';

export const sseStatus = writable<SseStatus>('idle');
export const reconnectCount = writable(0);
export const lastConnectionError = writable<string | null>(null);
export const streamedSessionId = writable<string | null>(null);
export const dashboardStreamCursor = writable<string | null>(null);

export function resetConnectionState(): void {
  sseStatus.set('idle');
  reconnectCount.set(0);
  lastConnectionError.set(null);
  streamedSessionId.set(null);
  dashboardStreamCursor.set(null);
}
