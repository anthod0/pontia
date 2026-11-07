import { readable, type Readable } from "svelte/store";
import { listSessionFiles } from "../api/sessionFiles";
import type { SharedFile } from "../api/types";
import { subscribeDashboardEvents } from "../services/eventStream";

export type SessionFilesState = { files: SharedFile[]; loading: boolean; error: string | null };
export type SessionFilesStore = Readable<SessionFilesState> & { retry: () => void };

export function createSessionFilesStore(sessionId: string): SessionFilesStore {
  let retry = () => {};
  const store = readable<SessionFilesState>({ files: [], loading: true, error: null }, (set) => {
    const controller = new AbortController();
    let state: SessionFilesState = { files: [], loading: true, error: null };
    const update = (next: Partial<SessionFilesState>) => set((state = { ...state, ...next }));
    const merge = (incoming: SharedFile[]) => {
      const entries = new Map(state.files.map((file) => [file.file_id, file]));
      for (const file of incoming) entries.set(file.file_id, file);
      update({
        files: [...entries.values()].sort(
          (a, b) => b.created_at.localeCompare(a.created_at) || b.file_id.localeCompare(a.file_id),
        ),
      });
    };
    const load = async () => {
      update({ loading: true, error: null });
      try {
        let cursor: string | undefined;
        do {
          const page = await listSessionFiles(sessionId, cursor, controller.signal);
          merge(page.files);
          cursor = page.next_cursor ?? undefined;
        } while (cursor);
      } catch (cause) {
        if (!controller.signal.aborted)
          update({ error: cause instanceof Error ? cause.message : String(cause) });
      } finally {
        if (!controller.signal.aborted) update({ loading: false });
      }
    };
    retry = () => void load();
    const unsubscribe = subscribeDashboardEvents((event) => {
      if (
        event.kind === "session_event" &&
        event.event.session_id === sessionId &&
        event.event.file
      )
        merge([event.event.file]);
    });
    void load();
    return () => {
      controller.abort();
      unsubscribe();
      retry = () => {};
    };
  });
  return { subscribe: store.subscribe, retry: () => retry() };
}
