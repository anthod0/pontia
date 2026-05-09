import { derived, writable } from 'svelte/store';
import { listTurns } from '../api/client';
import type { TurnView } from '../api/types';

export const turns = writable<TurnView[]>([]);
export const turnsLoading = writable(false);
export const turnsError = writable<string | null>(null);

export const activeTurn = derived(turns, ($turns) => $turns.find((turn) => turn.state === 'running' || turn.state === 'queued') ?? null);
export const latestOutput = derived(turns, ($turns) => [...$turns].reverse().find((turn) => turn.output?.summary)?.output?.summary ?? 'No turn output yet.');

export async function loadTurns(sessionId: string): Promise<void> {
  turnsLoading.set(true);
  turnsError.set(null);
  try {
    turns.set(await listTurns(sessionId));
  } catch (error) {
    turnsError.set(error instanceof Error ? error.message : String(error));
  } finally {
    turnsLoading.set(false);
  }
}
