import type * as RemoteAccessModule from "./remote-access";

export type RemoteAccess = typeof RemoteAccessModule;

// Built separately by vite.remote-access.config.ts and served from the site root.
const REMOTE_ACCESS_URL = "/_remote-access/remote-access.js";

// Module namespaces are not stored in reactive state; `ready` signals that loading finished.
let loaded: RemoteAccess | null = null;
let ready = $state(false);
let loading: Promise<RemoteAccess> | null = null;

function importRemoteAccess(): Promise<RemoteAccess> {
  if (import.meta.env.DEV) return import("./remote-access");
  return import(/* @vite-ignore */ REMOTE_ACCESS_URL);
}

export function loadRemoteAccess(): Promise<RemoteAccess> {
  loading ??= importRemoteAccess().then(
    (module) => {
      loaded = module;
      ready = true;
      return module;
    },
    (error: unknown) => {
      loading = null;
      throw error;
    },
  );
  return loading;
}

/** The loaded remote access module, or null before loading completes. Reactive. */
export function loadedRemoteAccess(): RemoteAccess | null {
  return ready ? loaded : null;
}
