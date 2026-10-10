import { createQuery, queryOptions } from "@tanstack/svelte-query";
import { getSession, listSessionModels } from "../api/client";
import { queryClient } from "../lib/queryClient";

export const sessionKeys = {
  all: ["sessions"] as const,
  detail: (sessionId: string) => [...sessionKeys.all, sessionId] as const,
  models: (sessionId: string) => [...sessionKeys.all, sessionId, "models"] as const,
};

function sessionOptions(sessionId: string, enabled: boolean) {
  return queryOptions({
    queryKey: sessionKeys.detail(sessionId),
    enabled: enabled && sessionId.length > 0,
    queryFn: ({ signal }) => getSession(sessionId, { signal }),
  });
}

function sessionModelsOptions(sessionId: string) {
  return queryOptions({
    queryKey: sessionKeys.models(sessionId),
    enabled: sessionId.length > 0,
    queryFn: ({ signal }) => listSessionModels(sessionId, { signal }),
  });
}

export function createSessionQuery(sessionId: () => string, enabled: () => boolean = () => true) {
  return createQuery(
    () => sessionOptions(sessionId(), enabled()),
    () => queryClient,
  );
}

export function createSessionModelsQuery(sessionId: () => string) {
  return createQuery(
    () => sessionModelsOptions(sessionId()),
    () => queryClient,
  );
}
