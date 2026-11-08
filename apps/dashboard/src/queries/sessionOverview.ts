import {
  createInfiniteQuery,
  infiniteQueryOptions,
  type InfiniteData,
} from "@tanstack/svelte-query";

import type { SessionOverviewView, SessionView } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

type PageParam = string | null;

export type SessionOverviewSnapshot = {
  pinned: SessionView[];
  active: SessionView[];
  list: SessionView[];
  archived: SessionView[];
  nextCursor: string | null;
};

export function snapshotSessionOverview(
  data: InfiniteData<SessionOverviewView, unknown> | undefined,
): SessionOverviewSnapshot {
  const first = data?.pages[0];
  const listById = new Map<string, SessionView>();
  for (const page of data?.pages ?? []) {
    for (const session of page.groups.list?.sessions ?? []) {
      listById.set(session.session_id, session);
    }
  }
  return {
    pinned: first?.groups.pinned?.sessions ?? [],
    active: first?.groups.active?.sessions ?? [],
    list: [...listById.values()],
    archived: first?.groups.archived?.sessions ?? [],
    nextCursor: data?.pages.at(-1)?.groups.list?.next_cursor ?? null,
  };
}
export function createSessionOverview(device: DeviceRuntime) {
  const listLimit = 50;

  const sessionOverviewKeys = {
    all: [device.id, "sessions", "overview"] as const,
  };

  function sessionOverviewOptions(includeArchived: boolean, workspaceId: string | undefined) {
    return infiniteQueryOptions({
      queryKey: [
        ...sessionOverviewKeys.all,
        { includeArchived, workspaceId: workspaceId ?? null },
      ] as const,
      queryFn: ({ pageParam, signal }) =>
        device.api.getSessionOverview(
          {
            sections: pageParam
              ? ["list"]
              : includeArchived
                ? ["pinned", "active", "list", "archived"]
                : ["pinned", "active", "list"],
            limit: listLimit,
            workspaceId,
            ...(pageParam ? { cursor: pageParam } : {}),
          },
          { signal },
        ),
      initialPageParam: null as PageParam,
      getNextPageParam: (lastPage) => lastPage.groups.list?.next_cursor ?? undefined,
    });
  }

  function createSessionOverviewQuery(
    includeArchived = false,
    workspaceId: () => string | undefined = () => undefined,
  ) {
    return createInfiniteQuery(
      () => sessionOverviewOptions(includeArchived, workspaceId()),
      () => device.queryClient,
    );
  }

  function invalidateSessionOverview(): Promise<void> {
    return device.queryClient.invalidateQueries({ queryKey: sessionOverviewKeys.all });
  }

  function clearSessionOverviewQuery(): void {
    void device.queryClient.cancelQueries({ queryKey: sessionOverviewKeys.all });
    device.queryClient.removeQueries({ queryKey: sessionOverviewKeys.all });
  }
  return {
    sessionOverviewKeys,
    createSessionOverviewQuery,
    invalidateSessionOverview,
    clearSessionOverviewQuery,
  };
}
