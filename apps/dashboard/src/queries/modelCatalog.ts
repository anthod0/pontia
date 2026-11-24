import type { QueryFunctionContext } from "@tanstack/svelte-query";
import type { AgentClientModels, AgentClientView } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";
import { readStoredWith, scopedKey, writeStored } from "../lib/storage/webStorage";

const catalogLifetime = 15 * 60 * 1000;

export function modelCatalogQuery(
  device: DeviceRuntime,
  client: Pick<AgentClientView, "client_type" | "client_key" | "version">,
  workspaceId?: string,
) {
  const { client_type, client_key, version } = client;
  const storageKey = scopedKey("pontia.modelCatalog", device.id, client_type, client_key);
  const queryKey = ["agent-client-models", client_type, client_key, version];
  function read(): AgentClientModels | undefined {
    return (
      readStoredWith(storageKey, (saved) => {
        const entry = JSON.parse(saved);
        if (Array.isArray(entry?.models) && typeof entry.fetched_at === "number") return entry;
      }) ?? undefined
    );
  }

  function remember(entry: AgentClientModels) {
    if ((read()?.fetched_at ?? 0) <= entry.fetched_at) {
      writeStored(storageKey, JSON.stringify(entry));
    }
  }

  function fresh(entry: AgentClientModels) {
    return entry.version === version && Date.now() - entry.fetched_at < catalogLifetime;
  }

  return {
    queryKey,
    initialData: read,
    queryFn: async ({ signal }: QueryFunctionContext) => {
      const local = read();
      if (local && fresh(local)) return local;
      const saved = await device.api.listAgentClientModels(
        client_type,
        client_key,
        signal,
        workspaceId,
        true,
      );
      signal.throwIfAborted();
      remember(saved);
      if (fresh(saved)) return saved;
      if (!local || saved.fetched_at >= local.fetched_at) {
        device.queryClient.setQueryData(queryKey, saved);
      }
      const latest = await device.api.listAgentClientModels(
        client_type,
        client_key,
        signal,
        workspaceId,
        false,
      );
      signal.throwIfAborted();
      remember(latest);
      return latest;
    },
    retry: false,
    refetchOnWindowFocus: false,
  };
}
