import {
  createInfiniteQuery,
  createMutation,
  createQuery,
  keepPreviousData,
} from "@tanstack/svelte-query";
import type { ReplaceUserFileInput } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createMemoryQueries(device: DeviceRuntime) {
  const memoryKeys = {
    all: [device.id, "memory"] as const,
    list: (query: string) => [...memoryKeys.all, "list", query] as const,
    user: () => [...memoryKeys.all, "user"] as const,
  };

  function createMemoriesQuery(query: () => string) {
    return createInfiniteQuery(
      () => ({
        queryKey: memoryKeys.list(query()),
        queryFn: ({ pageParam, signal }) =>
          device.api.listMemories({ q: query() || undefined, cursor: pageParam }, { signal }),
        initialPageParam: undefined as string | undefined,
        getNextPageParam: (page) => page.next_cursor ?? undefined,
        placeholderData: keepPreviousData,
      }),
      () => device.queryClient,
    );
  }

  function createUserFileQuery() {
    return createQuery(
      () => ({
        queryKey: memoryKeys.user(),
        queryFn: ({ signal }) => device.api.getUserFile({ signal }),
      }),
      () => device.queryClient,
    );
  }

  function createReplaceUserFileMutation() {
    return createMutation(
      () => ({
        mutationFn: (input: ReplaceUserFileInput) => device.api.replaceUserFile(input),
        onSuccess: (user) => device.queryClient.setQueryData(memoryKeys.user(), user),
      }),
      () => device.queryClient,
    );
  }

  return { memoryKeys, createMemoriesQuery, createUserFileQuery, createReplaceUserFileMutation };
}
