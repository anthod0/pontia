import { createQuery, queryOptions } from "@tanstack/svelte-query";
import { queryClient } from "../../queries/queryClient";
import { loadRemoteAccess } from "./remoteAccessLoader.svelte";

export const publicDeviceKeys = {
  all: ["public-devices"] as const,
  list: () => [...publicDeviceKeys.all, "list"] as const,
};

function publicDevicesOptions(enabled: boolean) {
  return queryOptions({
    queryKey: publicDeviceKeys.list(),
    enabled,
    queryFn: async ({ signal }) => (await loadRemoteAccess()).listPublicDevices(signal),
  });
}

export function createPublicDevicesQuery(enabled: () => boolean = () => true) {
  return createQuery(
    () => publicDevicesOptions(enabled()),
    () => queryClient,
  );
}
