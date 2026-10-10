import {
  createInfiniteQuery,
  infiniteQueryOptions,
  type InfiniteData,
} from "@tanstack/svelte-query";
import { getSessionOverview } from "../api/client";
import type { SessionOverviewView, SessionView } from "../api/types";
import { queryClient } from "./queryClient";

const listLimit = 50;
export const sessionOverviewKeys = {
  all: ["sessions", "overview"] as const,
};
type PageParam = string | null;

export type SessionOverviewSnapshot = {
  pinned: SessionView[];
  active: SessionView[];
  list: SessionView[];
  archived: SessionView[];
  nextCursor: string | null;
};

function sessionOverviewOptions(includeArchived: boolean) {
  return infiniteQueryOptions({
    queryKey: [...sessionOverviewKeys.all, { includeArchived }] as const,
    queryFn: ({ pageParam, signal }) =>
      getSessionOverview(
        {
          sections: pageParam
            ? ["list"]
            : includeArchived
              ? ["pinned", "active", "list", "archived"]
              : ["pinned", "active", "list"],
          limit: listLimit,
          ...(pageParam ? { cursor: pageParam } : {}),
        },
        { signal },
      ),
    initialPageParam: null as PageParam,
    getNextPageParam: (lastPage) => lastPage.groups.list?.next_cursor ?? undefined,
  });
}

export function createSessionOverviewQuery(includeArchived = false) {
  return createInfiniteQuery(
    () => sessionOverviewOptions(includeArchived),
    () => queryClient,
  );
}

export function invalidateSessionOverview(): Promise<void> {
  return queryClient.invalidateQueries({ queryKey: sessionOverviewKeys.all });
}

export function clearSessionOverviewQuery(): void {
  void queryClient.cancelQueries({ queryKey: sessionOverviewKeys.all });
  queryClient.removeQueries({ queryKey: sessionOverviewKeys.all });
}

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
