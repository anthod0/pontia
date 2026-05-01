import { writable } from 'svelte/store';
import { getSession } from '../api/client';
import type { SessionView } from '../api/types';

export const session = writable<SessionView | null>(null);
export const sessionLoading = writable(false);
export const sessionError = writable<string | null>(null);

export async function refreshSession(sessionId: string): Promise<void> {
  sessionLoading.set(true);
  sessionError.set(null);
  try {
    session.set(await getSession(sessionId));
  } catch (error) {
    sessionError.set(error instanceof Error ? error.message : String(error));
  } finally {
    sessionLoading.set(false);
  }
}
