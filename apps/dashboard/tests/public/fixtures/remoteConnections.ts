import { vi } from "vitest";
import * as cloud from "../../../src/modes/public/remote-access/remoteAccess";
import type { RemoteAccess } from "../../../src/modes/public/remoteAccessLoader.svelte";

export function createRemoteAccessFixture() {
  const listed = [
    { handle: "office-mac", name: "Office Mac" },
    { handle: "travel-laptop", name: "Travel Laptop" },
  ];
  const streams = new Map<string, ReadableStreamDefaultController<Uint8Array>>();
  const signals = new Map<string, AbortSignal[]>();
  const connectionFailures = new Map<string, (error: unknown) => void>();
  const authorizationFailures = new Map<string, () => void>();
  const target = (handle: string) => ({
    handle,
    deviceId: handle,
    edgeApiOrigin: "https://same.edge.pontia.dev",
  });
  const connect = vi.fn(async (handle: string, _signal: AbortSignal) => target(handle));
  const request = vi.fn(async (handle: string, path: string, init: RequestInit) => {
    if (path.includes("/events/stream")) {
      return new Response(
        new ReadableStream<Uint8Array>({
          start(controller) {
            streams.set(handle, controller);
            init.signal!.addEventListener("abort", () => controller.error(init.signal!.reason), {
              once: true,
            });
          },
        }),
      );
    }
    const data = path.endsWith("/workspaces")
      ? { workspaces: [] }
      : path.includes("/overview")
        ? { active: [], pinned: [], list: [], next_cursor: null }
        : { workflows: [] };
    return new Response(JSON.stringify({ data }));
  });
  const remote: RemoteAccess = {
    ...cloud,
    listPublicDevices: vi.fn(async (_signal?: AbortSignal) => [...listed]),
    createRemoteConnection(handle, onConnectionFailure) {
      const controller = new AbortController();
      signals.set(handle, [...(signals.get(handle) ?? []), controller.signal]);
      connectionFailures.set(handle, (error) => onConnectionFailure?.(error));
      authorizationFailures.set(handle, () =>
        onConnectionFailure?.(new cloud.CloudRequestError("unauthorized", 401)),
      );
      return {
        connect: () => connect(handle, controller.signal),
        uploadStorageScope: async () => handle,
        signal: controller.signal,
        e2eFetch: (path, init = {}) => {
          controller.signal.throwIfAborted();
          return request(handle, path, init);
        },
        dispose: () => controller.abort(),
      };
    },
  };
  function event(handle: string, cursor: string) {
    streams.get(handle)!.enqueue(
      new TextEncoder().encode(
        `id: ${cursor}\ndata: ${JSON.stringify({
          kind: "session_event",
          id: cursor,
          occurred_at: "now",
          event: { session_id: "same", type: "session.updated", payload: {} },
        })}\n\n`,
      ),
    );
  }
  return {
    remote,
    listed,
    streams,
    signals,
    authorizationFailures,
    connectionFailures,
    connect,
    request,
    target,
    event,
  };
}
