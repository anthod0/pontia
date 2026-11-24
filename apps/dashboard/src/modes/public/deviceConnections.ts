import { toast } from "svelte-sonner";
import { translate } from "$lib/i18n";
import { get, writable } from "svelte/store";
import type { LocalhostPreviewTarget } from "$lib/localhostPreview";
import { DeviceRuntime } from "../../device/runtime";
import { createApiAccess } from "./apiAccess";
import type { RemoteAccess } from "./remoteAccessLoader.svelte";
type PublicDevice = Awaited<ReturnType<RemoteAccess["listPublicDevices"]>>["devices"][number];

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
  let userId: string | null = null;
  let refreshRetryTimer: ReturnType<typeof setTimeout> | undefined;
  let refreshRetryDelay = 5000;

  function publish(): void {
    devices.set(
      [...entries.values()].map(({ handle, deviceId, edgeId, name, state, runtime }) => ({
        handle,
        deviceId,
        edgeId,
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
    clearTimeout(refreshRetryTimer);
    remote.retainSavedConnections(null, []);
    lifetime.abort();
    for (const entry of entries.values()) {
      stop(entry);
      entry.state = "sign-in-required";
    }
    error.set(translate("devices.signInToContinue"));
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

  async function openPreview(
    entry: Entry,
    target: LocalhostPreviewTarget,
    signal: AbortSignal,
    frame?: HTMLIFrameElement,
  ) {
    try {
      const submit = await remote.launchLocalhostPreview(
        entry,
        target,
        signal,
        translate("preview.loading"),
        frame,
      );
      if (submit) {
        toast.info(translate("preview.popupBlocked"), {
          action: { label: translate("preview.open"), onClick: submit },
        });
      }
    } catch (failure) {
      if (signal.aborted) return;
      if (failure instanceof remote.CloudRequestError && failure.status === 401) {
        requireSignIn();
        return;
      }
      toast.error(
        translate(
          failure instanceof remote.CloudRequestError && failure.status === 409
            ? "preview.unavailable"
            : "preview.failed",
        ),
      );
    }
  }

  async function connect(entry: Entry): Promise<void> {
    if (lifetime.signal.aborted || userId === null) return;
    stop(entry);
    entry.state = "connecting";
    const connection = remote.createRemoteConnection(entry, userId, (failure) => {
      if (failure instanceof remote.CloudRequestError && failure.status === 401) requireSignIn();
      else offline(entry);
    });
    entry.connection = connection;
    publish();
    try {
      await connection.connect();
      const id = await connection.uploadStorageScope();
      if (connection.signal.aborted) return;
      const access = createApiAccess(connection, (target, frame) =>
        openPreview(entry, target, connection.signal, frame),
      );
      const runtime = new DeviceRuntime(id, access, entry.name);
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

  function reconcile(listing: Awaited<ReturnType<RemoteAccess["listPublicDevices"]>>): void {
    if (userId !== listing.userId) {
      for (const entry of entries.values()) stop(entry);
      entries.clear();
    }
    userId = listing.userId;
    const listed = listing.devices;
    const handles = new Set(listed.map((device) => device.handle));
    for (const [handle, entry] of entries) {
      if (!handles.has(handle)) {
        stop(entry);
        entries.delete(handle);
      }
    }
    for (const device of listed) {
      const existing = entries.get(device.handle);
      if (existing && existing.deviceId === device.deviceId && existing.edgeId === device.edgeId)
        existing.name = device.name;
      else {
        if (existing) stop(existing);
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
    publish();
  }

  function refreshDevices(): Promise<void> {
    if (lifetime.signal.aborted) return Promise.resolve();
    if (refresh) return refresh;
    clearTimeout(refreshRetryTimer);
    loading.set(userId === null);
    refresh = (async () => {
      try {
        const listing = await remote.listPublicDevices(lifetime.signal);
        if (lifetime.signal.aborted) return;
        remote.retainSavedConnections(listing.userId, listing.devices);
        remote.deviceListCache.save(listing);
        reconcile(listing);
        refreshRetryDelay = 5000;
        error.set(null);
      } catch (failure) {
        if (lifetime.signal.aborted) return;
        if (failure instanceof remote.CloudRequestError && failure.status === 401) requireSignIn();
        else {
          error.set(translate("devices.unreachable"));
          refreshRetryTimer = setTimeout(() => void refreshDevices(), refreshRetryDelay);
          refreshRetryDelay = Math.min(refreshRetryDelay * 2, 60_000);
        }
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
    void refreshDevices().then(() => {
      if (!get(error)) for (const entry of entries.values()) retry(entry.handle);
    });
  }

  const cached = remote.deviceListCache.read();
  if (cached) {
    reconcile(cached);
    loading.set(false);
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
      clearTimeout(refreshRetryTimer);
      lifetime.abort();
      for (const entry of entries.values()) stop(entry);
      entries.clear();
    },
  };
}
