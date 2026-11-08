import { QueryObserver } from "@tanstack/svelte-query";

import { readable } from "svelte/store";
import type { DeviceRuntime } from "../device/runtime";

export function createSessionCreation(device: DeviceRuntime) {
  function createSessionCreationOptionsStore() {
    const observer = new QueryObserver(device.queryClient, {
      queryKey: [device.id, "session-creation-options"],
      queryFn: ({ signal }) => device.api.listSessionCreationOptions({ signal }),
    });
    return readable(observer.getCurrentResult(), (set) => {
      set(observer.getCurrentResult());
      return observer.subscribe(set);
    });
  }
  return { createSessionCreationOptionsStore };
}
