import { onlineManager } from "@tanstack/svelte-query";
import { afterEach, expect, test, vi } from "vitest";
import { get } from "svelte/store";
import { File } from "node:buffer";
import { DeviceRuntime } from "../src/device/runtime";
import type { ApiAccess } from "../src/device/apiAccess";
import { writeCachedTimeline } from "../src/lib/session-chat/timelineCache";

const devices: DeviceRuntime[] = [];
function device(id: string, request: ApiAccess["apiFetch"]) {
  const controller = new AbortController();
  const runtime = new DeviceRuntime(id, {
    apiCredentials: "include",
    apiStreamUnavailableMessage: "Unavailable",
    apiUrl: (path) => `https://same-edge.test/devices/${id}${path}`,
    apiFetch: request,
    apiSignal: (signal) =>
      signal ? AbortSignal.any([controller.signal, signal]) : controller.signal,
    applyApiAuthentication: () => true,
    handleApiAuthenticationFailure() {},
    uploadStorageScope: async () => id,
    dispose: () => controller.abort(),
  });
  devices.push(runtime);
  return runtime;
}
function response(data: unknown) {
  return new Response(JSON.stringify({ data }));
}
afterEach(() => {
  for (const runtime of devices.splice(0)) runtime.dispose();
  localStorage.clear();
  onlineManager.setOnline(true);
});

test("same Workspace and Session IDs keep their requests, caches and selections on their device", async () => {
  const request = vi.fn(async (url: string) =>
    response({ workspaces: [{ workspace_id: "same", name: url.includes("/a/") ? "A" : "B" }] }),
  );
  const a = device("a", request);
  const b = device("b", request);
  const [first, second] = await Promise.all([
    a.workspaceQueries.fetchWorkspaces(),
    b.workspaceQueries.fetchWorkspaces(),
  ]);
  expect(first[0].name).toBe("A");
  expect(second[0].name).toBe("B");
  expect(a.queryClient.getQueryData(b.workspaceQueries.workspaceKeys.list())).toBeUndefined();
  a.sessions.selectSession("same");
  b.sessions.selectSession("same");
  a.chatDraft.setChatDraft("Draft A");
  b.chatDraft.setChatDraft("Draft B");
  a.sessions.selectSession("another");
  expect(get(b.sessions.selectedSessionId)).toBe("same");
  expect(get(a.chatDraft.chatDraft)).toBe("Draft A");
  expect(get(b.chatDraft.chatDraft)).toBe("Draft B");
});

test("a delayed mutation and its follow-up reads stay with the initiating device", async () => {
  let finish!: (value: Response) => void;
  const requestA = vi.fn((url: string) =>
    url.endsWith("/interrupt")
      ? new Promise<Response>((resolve) => {
          finish = resolve;
        })
      : Promise.resolve(
          response(
            url.endsWith("/turns")
              ? { turns: [] }
              : url.endsWith("/events")
                ? { events: [] }
                : url.endsWith("/inbox/messages")
                  ? { inbox_messages: [] }
                  : { session: { session_id: "same", title: "A", state: "idle" } },
          ),
        ),
  );
  const a = device("a", requestA);
  a.sessions.selectSession("same");
  const pending = a.sessions.interruptSession("same");
  const requestB = vi.fn(async () => response({ session: { session_id: "same", title: "B" } }));
  const b = device("b", requestB);
  b.sessions.selectSession("same");
  await b.queryClient.fetchQuery(b.sessionQueries.sessionOptions("same"));
  finish(response({ accepted: true }));
  await pending;
  expect(requestA.mock.calls.map(([url]) => new URL(url).pathname)).toEqual(
    expect.arrayContaining([
      "/devices/a/api/v1/sessions/same/interrupt",
      "/devices/a/api/v1/sessions/same/turns",
      "/devices/a/api/v1/sessions/same/events",
    ]),
  );
  expect(requestB).toHaveBeenCalledTimes(1);
  expect(a.queryClient.getQueryData(a.sessionQueries.sessionKeys.detail("same"))).toMatchObject({
    title: "A",
  });
  expect(b.queryClient.getQueryData(b.sessionQueries.sessionKeys.detail("same"))).toMatchObject({
    title: "B",
  });
});

test("events update only their device cursor and cache, and stopping one stream leaves the other open", async () => {
  const streams = new Map<string, ReadableStreamDefaultController<Uint8Array>>();
  const request = vi.fn(async (url: string, init: RequestInit) => {
    const id = new URL(url).pathname.split("/")[2];
    if (!url.includes("/events/stream")) return response({ workflows: [] });
    return new Response(
      new ReadableStream<Uint8Array>({
        start(controller) {
          streams.set(id, controller);
          init.signal!.addEventListener("abort", () => controller.close(), { once: true });
        },
      }),
    );
  });
  const a = device("a", request);
  const b = device("b", request);
  a.events.startEventStream();
  b.events.startEventStream();
  await vi.waitFor(() => expect(get(b.connection.sseStatus)).toBe("open"));
  a.queryClient.setQueryData(a.sessionQueries.sessionKeys.detail("same"), { title: "A" });
  b.queryClient.setQueryData(b.sessionQueries.sessionKeys.detail("same"), { title: "B" });
  const event = {
    kind: "session_event",
    id: "event",
    occurred_at: "now",
    event: { session_id: "same", type: "session.updated", payload: {} },
  };
  streams
    .get("a")!
    .enqueue(new TextEncoder().encode(`id: cursor-a\ndata: ${JSON.stringify(event)}\n\n`));
  await vi.waitFor(() => expect(get(a.connection.dashboardStreamCursor)).toBe("cursor-a"));
  expect(get(b.connection.dashboardStreamCursor)).toBeNull();
  expect(
    a.queryClient.getQueryState(a.sessionQueries.sessionKeys.detail("same"))?.isInvalidated,
  ).toBe(true);
  expect(
    b.queryClient.getQueryState(b.sessionQueries.sessionKeys.detail("same"))?.isInvalidated,
  ).toBe(false);
  a.events.stopEventStream();
  expect(get(b.connection.sseStatus)).toBe("open");
});

test("Inbox recovery and persisted timelines do not cross devices after reload", async () => {
  const a = device("a", vi.fn());
  const b = device("b", vi.fn());
  const first = { sessionId: "same", messageId: "same", input: { input: "A" } };
  const second = { ...first, input: { input: "B" } };
  a.inboxRecovery.rememberSubmission(first);
  b.inboxRecovery.rememberSubmission(second);
  const reloadedA = device("a", vi.fn());
  const reloadedB = device("b", vi.fn());
  expect(get(reloadedA.inboxRecovery.unconfirmedSubmissions)).toEqual([first]);
  expect(get(reloadedB.inboxRecovery.unconfirmedSubmissions)).toEqual([second]);
  reloadedA.inboxRecovery.forgetSubmission("same");
  expect(get(reloadedB.inboxRecovery.unconfirmedSubmissions)).toEqual([second]);
  await writeCachedTimeline({
    deviceId: "a",
    sessionId: "same",
    mode: "linear",
    groups: [],
    items: [],
    nextOlderTurnId: null,
    latestTurnId: "a-turn",
    hasMore: false,
  });
  await writeCachedTimeline({
    deviceId: "b",
    sessionId: "same",
    mode: "linear",
    groups: [],
    items: [],
    nextOlderTurnId: null,
    latestTurnId: "b-turn",
    hasMore: false,
  });
  expect(await reloadedA.timeline.restoreSessionTimeline("same")).toBe(true);
  expect(await reloadedB.timeline.restoreSessionTimeline("same")).toBe(true);
  expect(get(reloadedA.timeline.timelineState).latestTurnId).toBe("a-turn");
  expect(get(reloadedB.timeline.timelineState).latestTurnId).toBe("b-turn");
});

test("an upload that finishes hashing after another device opens sends every chunk to its original device", async () => {
  const urls: string[] = [];
  let offset = 0;
  const a = device("upload-a", async (url, init) => {
    urls.push(url);
    if (init.method === "PATCH") offset += (init.body as ArrayBuffer).byteLength;
    return response({
      upload: {
        upload_id: "same",
        name: "x",
        size: 3,
        sha256: "hash",
        offset,
        completed: url.endsWith("/complete"),
        path: url.endsWith("/complete") ? "/a/x" : null,
      },
    });
  });
  const draft = a.uploadDrafts.createUploadDraft(crypto.randomUUID());
  await draft.initialized;
  await draft.add([new File(["abc"], "x") as unknown as globalThis.File]);
  const b = device("upload-b", vi.fn());
  await vi.waitFor(() => expect(get(draft).items[0].status).toBe("complete"));
  expect(urls).toHaveLength(3);
  expect(
    urls.every((url) => new URL(url).pathname.startsWith("/devices/upload-a/api/v1/uploads")),
  ).toBe(true);
  expect(b.access.apiFetch).not.toHaveBeenCalled();
  expect(get(draft).items[0].upload?.path).toBe("/a/x");
  await draft.clear();
  draft.destroy();
});

test("a device resumes its paused mutation when the browser comes online", async () => {
  const request = vi.fn(async () =>
    response({ session: { session_id: "same", title: "Updated" } }),
  );
  const runtime = device("offline-device", request);
  onlineManager.setOnline(false);
  const pending = runtime.sessionQueries.mutateUpdateSession("same", { title: "Updated" });
  await vi.waitFor(() =>
    expect(runtime.queryClient.getMutationCache().getAll()[0]?.state.isPaused).toBe(true),
  );
  expect(request).not.toHaveBeenCalled();
  onlineManager.setOnline(true);
  await expect(pending).resolves.toMatchObject({ title: "Updated" });
  expect(
    runtime.queryClient.getQueryData(runtime.sessionQueries.sessionKeys.detail("same")),
  ).toMatchObject({ title: "Updated" });
});
