import type { DeviceRuntime } from "../../device/runtime";
import type { SessionView } from "../../api/types";
import { recentSessions, snapshotSessionOverview } from "../../queries/sessionOverview";
export type SessionScope = "active" | "all" | "pinned" | "archived";
export function createSessionList(
  device: DeviceRuntime,
  includeArchived = true,
  workspace: () => string = () => "",
  scope: () => SessionScope = () => "all",
) {
  const query = device.sessionOverview.createSessionOverviewQuery(
    includeArchived,
    () => workspace() || undefined,
  );
  const overview = $derived(snapshotSessionOverview(query.data));
  const recent = $derived(
    recentSessions(overview, workspace(), scope() === "active" ? "active" : "all"),
  );
  const hasClosed = $derived(
    scope() === "active" &&
      (!!overview.nextCursor ||
        recentSessions(overview, workspace(), "all").length > recent.length),
  );
  const pinned = $derived(
    overview.pinned.filter(
      (s) =>
        (!workspace() || s.workspace_id === workspace()) &&
        (scope() !== "active" || (s.state !== "exited" && s.state !== "error")),
    ),
  );
  const list = $derived(
    scope() === "archived"
      ? overview.archived.filter((s) => !workspace() || s.workspace_id === workspace())
      : scope() === "pinned"
        ? pinned
        : [...pinned, ...recent],
  );
  const api = device.sessions;
  let busyId = $state<string | null>(null);
  async function manage(id: string, action: () => Promise<unknown>) {
    busyId = id;
    try {
      await action();
    } finally {
      if (busyId === id) busyId = null;
    }
  }
  return {
    query,
    get overview() {
      return overview;
    },
    get list() {
      return list;
    },
    get recent() {
      return recent;
    },
    get hasClosed() {
      return hasClosed;
    },
    get busyId() {
      return busyId;
    },
    loadNextPage() {
      if (scope() === "all" && overview.nextCursor && !query.isFetchingNextPage)
        void query.fetchNextPage();
    },
    pin(s: SessionView) {
      return manage(s.session_id, () =>
        (s.pinned_at ? api.unpinSession : api.pinSession)(s.session_id),
      );
    },
    archive(s: SessionView) {
      return manage(s.session_id, () =>
        (s.archived_at ? api.unarchiveSession : api.archiveSession)(s.session_id),
      );
    },
    exit(s: SessionView) {
      return manage(s.session_id, () => api.terminateSession(s.session_id));
    },
    rename(s: SessionView, title: string | null) {
      return manage(s.session_id, () => api.updateSessionTitle(s.session_id, title));
    },
  };
}
export type SessionListController = ReturnType<typeof createSessionList>;
