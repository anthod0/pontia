import { writable } from "svelte/store";
import { listWorkflowPatches } from "../../api/client";
import type { WorkflowPatchHistoryView } from "../../api/types";

export function createPatchHistoryReader() {
  const state = writable<{
    workflowId: string | null;
    loading: boolean;
    loaded: boolean;
    error: string | null;
    patches: WorkflowPatchHistoryView[];
  }>({ workflowId: null, loading: false, loaded: false, error: null, patches: [] });
  let workflow: string | null = null;
  let controller: AbortController | null = null;
  let pending: Promise<void> | null = null;
  return {
    subscribe: state.subscribe,
    load(workflowId: string): Promise<void> {
      // Repeated snapshot notifications must not starve a slow request.
      if (workflow === workflowId && pending) return pending;
      if (workflow !== workflowId) {
        controller?.abort();
        workflow = workflowId;
        state.set({ workflowId, loading: true, loaded: false, error: null, patches: [] });
      }
      const request = new AbortController();
      controller = request;
      state.update((value) => ({ ...value, loading: true, error: null }));
      pending = (async () => {
        try {
          const patches = await listWorkflowPatches(workflowId, { signal: request.signal });
          if (!request.signal.aborted)
            state.set({
              workflowId,
              loading: false,
              loaded: true,
              error: null,
              patches: [...patches].sort(
                (a, b) =>
                  Date.parse(b.requested_at) - Date.parse(a.requested_at) ||
                  b.patch_id.localeCompare(a.patch_id),
              ),
            });
        } catch (error) {
          if (!request.signal.aborted)
            state.update((value) => ({
              ...value,
              loading: false,
              error: error instanceof Error ? error.message : String(error),
            }));
        } finally {
          if (controller === request) pending = null;
        }
      })();
      return pending;
    },
    cancel() {
      controller?.abort();
      pending = null;
      workflow = null;
    },
  };
}
