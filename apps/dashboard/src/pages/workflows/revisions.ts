import { writable } from "svelte/store";
import { getWorkflowRevision } from "../../api/client";
import type { WorkflowGraphRevisionView } from "../../api/types";

export function revisionSelection(raw: string | null, current: number): number | null {
  if (raw === null) return current;
  if (!/^[1-9]\d*$/.test(raw)) return null;
  const revision = Number(raw);
  return Number.isSafeInteger(revision) && revision <= current ? revision : null;
}

// Storage starts at 1 and resolve_patch increments in the same transaction only
// for applied graph changes. Rejected/blocked patches do not create revisions.
export function workflowRevisions(current: number): number[] {
  return Array.from({ length: current }, (_, index) => current - index);
}

export function createRevisionReader() {
  const state = writable<{
    loading: boolean;
    error: string | null;
    detail: WorkflowGraphRevisionView | null;
  }>({ loading: false, error: null, detail: null });
  let controller: AbortController | null = null;
  return {
    subscribe: state.subscribe,
    async load(workflowId: string, revision: number) {
      controller?.abort();
      const request = new AbortController();
      controller = request;
      state.set({ loading: true, error: null, detail: null });
      try {
        const detail = await getWorkflowRevision(workflowId, revision, { signal: request.signal });
        if (!request.signal.aborted) state.set({ loading: false, error: null, detail });
      } catch (error) {
        if (!request.signal.aborted)
          state.set({
            loading: false,
            error: error instanceof Error ? error.message : String(error),
            detail: null,
          });
      }
    },
    cancel() {
      controller?.abort();
    },
  };
}
