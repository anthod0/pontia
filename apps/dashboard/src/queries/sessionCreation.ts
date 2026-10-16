import { QueryObserver } from "@tanstack/svelte-query";
import { readable } from "svelte/store";
import { listSessionCreationOptions } from "../api/client";
import { queryClient } from "./queryClient";

export function createSessionCreationOptionsStore() {
  const observer = new QueryObserver(queryClient, {
    queryKey: ["session-creation-options"],
    queryFn: ({ signal }) => listSessionCreationOptions({ signal }),
  });
  return readable(observer.getCurrentResult(), (set) => {
    set(observer.getCurrentResult());
    return observer.subscribe(set);
  });
}
