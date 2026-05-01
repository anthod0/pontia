import { writable } from 'svelte/store';
import { listSessions } from '../api/client';
import type { SessionView } from '../api/types';

export const sessions = writable<SessionView[]>([]);
export const sessionsLoading = writable(false);
export const sessionsError = writable<string | null>(null);

export async function loadSessions(): Promise<void> {
  sessionsLoading.set(true);
  sessionsError.set(null);
  try {
    sessions.set(await listSessions());
  } catch (error) {
    sessionsError.set(error instanceof Error ? error.message : String(error));
  } finally {
    sessionsLoading.set(false);
  }
}
