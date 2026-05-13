import { writable } from 'svelte/store';
import { loadArtifacts } from './artifacts';
import { loadEvents, showCachedEvents } from './events';
import { loadInboxMessages } from './inbox';
import { refreshSession } from './sessionDetail';
import { loadTurns } from './turns';

export const selectedSessionId = writable<string | null>(null);
export const selectionLoading = writable(false);
export const selectionError = writable<string | null>(null);

export async function selectSession(sessionId: string): Promise<void> {
  selectedSessionId.set(sessionId);
  showCachedEvents(sessionId);
  selectionLoading.set(true);
  selectionError.set(null);
  try {
    await Promise.all([
      refreshSession(sessionId),
      loadTurns(sessionId),
      loadEvents(sessionId),
      loadArtifacts(sessionId),
      loadInboxMessages(sessionId),
    ]);
  } catch (error) {
    selectionError.set(error instanceof Error ? error.message : String(error));
  } finally {
    selectionLoading.set(false);
  }
}
