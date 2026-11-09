import { get, writable } from "svelte/store";
import { DeviceRuntime } from "../../device/runtime";
import { createApiAccess } from "./apiAccess";
import type { RemoteAccess } from "./remoteAccessLoader.svelte";
type PublicDevice = Awaited<ReturnType<RemoteAccess["listPublicDevices"]>>[number];

export type DeviceConnectionState = "connecting" | "online" | "offline" | "sign-in-required";
export type ConnectedDevice = PublicDevice & {
  state: DeviceConnectionState;
  runtime: DeviceRuntime | null;
};

type Entry = ConnectedDevice & {
  connection: ReturnType<RemoteAccess["createRemoteConnection"]> | null;
  retryTimer?: ReturnType<typeof setTimeout>;
  retryDelay: number;
  unsubscribe?: () => void;
};

export function createDeviceConnections(remote: RemoteAccess) {
  const devices = writable<ConnectedDevice[]>([]);
  const loading = writable(true);
  const error = writable<string | null>(null);
  const signInRequired = writable(false);
  const entries = new Map<string, Entry>();
  const lifetime = new AbortController();
  let refresh: Promise<void> | null = null;

  function publish(): void {
    devices.set(
      [...entries.values()].map(({ handle, name, state, runtime }) => ({
        handle,
        name,
        state,
        runtime,
      })),
    );
  }

  function stop(entry: Entry): void {
    clearTimeout(entry.retryTimer);
    entry.retryTimer = undefined;
    entry.unsubscribe?.();
    entry.unsubscribe = undefined;
    entry.connection?.dispose();
    entry.connection = null;
    entry.runtime?.dispose();
    entry.runtime = null;
  }

  function requireSignIn(): void {
    if (get(signInRequired) || lifetime.signal.aborted) return;
    signInRequired.set(true);
    lifetime.abort();
    for (const entry of entries.values()) {
      stop(entry);
      entry.state = "sign-in-required";
    }
    error.set("Sign in to Pontia to continue.");
    loading.set(false);
    publish();
  }

  function offline(entry: Entry): void {
    stop(entry);
    entry.state = "offline";
    entry.retryTimer = setTimeout(() => void connect(entry), entry.retryDelay);
    entry.retryDelay = Math.min(entry.retryDelay * 2, 60_000);
    publish();
  }

  async function connect(entry: Entry): Promise<void> {
    if (lifetime.signal.aborted) return;
    stop(entry);
    entry.state = "connecting";
    const connection = remote.createRemoteConnection(entry.handle, (failure) => {
      if (failure instanceof remote.CloudRequestError && failure.status === 401) requireSignIn();
      else offline(entry);
    });
    entry.connection = connection;
    publish();
    try {
      await connection.connect();
      const id = await connection.uploadStorageScope();
      if (connection.signal.aborted) return;
      const runtime = new DeviceRuntime(id, createApiAccess(connection));
      entry.runtime = runtime;
      entry.unsubscribe = runtime.connection.sseStatus.subscribe((status) => {
        if (status !== "open") return;
        entry.state = "online";
        entry.retryDelay = 5000;
        publish();
      });
      runtime.lifecycle.startDashboardRuntime(() => offline(entry));
    } catch (failure) {
      if (connection.signal.aborted) return;
      if (failure instanceof remote.CloudRequestError && failure.status === 401) requireSignIn();
      else offline(entry);
    }
  }

  function refreshDevices(): Promise<void> {
    if (lifetime.signal.aborted) return Promise.resolve();
    if (refresh) return refresh;
    loading.set(true);
    refresh = (async () => {
      try {
        const listed = await remote.listPublicDevices(lifetime.signal);
        if (lifetime.signal.aborted) return;
        const handles = new Set(listed.map((device) => device.handle));
        for (const [handle, entry] of entries) {
          if (!handles.has(handle)) {
            stop(entry);
            entries.delete(handle);
          }
        }
        for (const device of listed) {
          const existing = entries.get(device.handle);
          if (existing) existing.name = device.name;
          else {
            const entry: Entry = {
              ...device,
              state: "connecting",
              runtime: null,
              connection: null,
              retryDelay: 5000,
            };
            entries.set(device.handle, entry);
            void connect(entry);
          }
        }
        error.set(null);
        publish();
      } catch (failure) {
        if (lifetime.signal.aborted) return;
        if (failure instanceof remote.CloudRequestError && failure.status === 401) requireSignIn();
        else error.set("Could not reach Pontia to load devices.");
      } finally {
        loading.set(false);
        refresh = null;
      }
    })();
    return refresh;
  }

  function retry(handle: string): void {
    const entry = entries.get(handle);
    if (entry?.state === "offline") void connect(entry);
  }

  function focus(): void {
    void refreshDevices();
    for (const entry of entries.values()) retry(entry.handle);
  }

  return {
    devices,
    loading,
    error,
    signInRequired,
    refreshDevices,
    retry,
    focus,
    dispose() {
      lifetime.abort();
      for (const entry of entries.values()) stop(entry);
      entries.clear();
    },
  };
}
