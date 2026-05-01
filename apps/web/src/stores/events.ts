import { writable } from 'svelte/store';
import { listEvents } from '../api/client';
import type { EventView } from '../api/types';

export const events = writable<EventView[]>([]);
export const eventsLoading = writable(false);
export const eventsError = writable<string | null>(null);

export async function loadEvents(sessionId: string): Promise<void> {
  eventsLoading.set(true);
  eventsError.set(null);
  try {
    events.set(await listEvents(sessionId));
  } catch (error) {
    eventsError.set(error instanceof Error ? error.message : String(error));
  } finally {
    eventsLoading.set(false);
  }
}
