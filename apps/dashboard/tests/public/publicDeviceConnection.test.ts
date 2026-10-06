import { afterEach, beforeEach, expect, test, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(),
  start: vi.fn(),
  sessionFree: vi.fn(),
  handshakeFree: vi.fn(),
  identityFree: vi.fn(),
}));

vi.mock("../../src/e2e-wasm/pontia_e2e", () => ({
  default: vi.fn(async () => undefined),
  Identity: class {
    public_key() {
      return new Uint8Array(32).fill(9);
    }
    start(...args: unknown[]) {
      return mocks.start(...args);
    }
    free() {
      mocks.identityFree();
    }
  },
}));

import {
  clearE2eSession,
  connectPublicDevice,
  e2eFetch,
} from "../../src/modes/public/e2eTransport";
import { clearPublicApiTarget, setPublicApiTarget } from "../../src/modes/public/apiTarget";

const handle = "office-mac";
const deviceId = "01234567-89ab-cdef-0123-456789abcdef";
const edge = "https://brave-atlas.edge.pontia.dev";
const encoded = (length: number) =>
  btoa(String.fromCharCode(...new Uint8Array(length).fill(9))).replace(/=+$/, "");
const cloudResponse = () =>
  new Response(
    JSON.stringify({
      device_handle: handle,
      device_id: deviceId,
      edge_api_origin: edge,
      device_public_key: encoded(32),
      device_key_version: 1,
      capability: encoded(169),
    }),
    { headers: { "Content-Type": "application/json" } },
  );

beforeEach(() => {
  vi.clearAllMocks();
  mocks.start.mockReturnValue({
    bytes: () => new Uint8Array([1]),
    confirm: mocks.confirm,
    free: mocks.handshakeFree,
  });
  mocks.confirm.mockImplementation(() => ({
    free: mocks.sessionFree,
    request: () => {
      const events = [[0, 200, []], [1, new Uint8Array([111, 107])], [2]];
      return {
        first_bytes: () => new Uint8Array([2]),
        finish_upload: () => new Uint8Array([3]),
        next_event: () => events.shift() ?? null,
        receive: (bytes: Uint8Array) => bytes.length,
        finish_response: () => undefined,
        free: vi.fn(),
      };
    },
  }));
});
afterEach(() => {
  clearE2eSession();
  clearPublicApiTarget();
  vi.unstubAllGlobals();
});

test("does not complete connection until the device confirms the handshake", async () => {
  let resolveHandshake!: (response: Response) => void;
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          resolveHandshake = resolve;
        }),
    );
  vi.stubGlobal("fetch", fetchMock);
  let completed = false;
  const connecting = connectPublicDevice(handle).then((target) => {
    completed = true;
    return target;
  });
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  expect(completed).toBe(false);
  expect(fetchMock.mock.calls[0][0]).toBe(
    `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
  );
  expect(fetchMock.mock.calls[1][0]).toBe(`${edge}/devices/${deviceId}/e2e/v1/sessions`);
  resolveHandshake(new Response(new Uint8Array([4])));
  await expect(connecting).resolves.toEqual({ handle, deviceId, edgeApiOrigin: edge });
  expect(mocks.confirm).toHaveBeenCalledWith(new Uint8Array([4]));
  await connectPublicDevice(handle);
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

test("shares one connection establishment across concurrent callers", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  const targets = await Promise.all([connectPublicDevice(handle), connectPublicDevice(handle)]);
  expect(targets[0]).toEqual(targets[1]);
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

test("a failed handshake can be retried with fresh authorization", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response(null, { status: 503 }))
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  await expect(connectPublicDevice(handle)).rejects.toThrow();
  expect(mocks.confirm).not.toHaveBeenCalled();
  await expect(connectPublicDevice(handle)).resolves.toMatchObject({ deviceId });
  expect(fetchMock).toHaveBeenCalledTimes(4);
});

test("a cancelled handshake cannot install a stale session", async () => {
  let resolveHandshake!: (response: Response) => void;
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          resolveHandshake = resolve;
        }),
    );
  vi.stubGlobal("fetch", fetchMock);
  const controller = new AbortController();
  const connecting = connectPublicDevice(handle, controller.signal);
  const rejected = expect(connecting).rejects.toMatchObject({ name: "AbortError" });
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  controller.abort();
  clearE2eSession();
  resolveHandshake(new Response("old confirmation"));
  await rejected;
  expect(mocks.confirm).not.toHaveBeenCalled();
  fetchMock
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response("new confirmation"));
  await expect(connectPublicDevice(handle)).resolves.toMatchObject({ deviceId });
  expect(fetchMock).toHaveBeenCalledTimes(4);
});

test.each([401, 409])(
  "reauthorizes rejected encrypted requests through connect (%i)",
  async (status) => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(new Response("confirmation"))
      .mockResolvedValueOnce(new Response(null, { status }))
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(new Response("confirmation"))
      .mockResolvedValueOnce(new Response("ciphertext"));
    vi.stubGlobal("fetch", fetchMock);
    setPublicApiTarget(await connectPublicDevice(handle));
    const response = await e2eFetch("/api/v1/workspaces");
    expect(await response.text()).toBe("ok");
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
      `${edge}/devices/${deviceId}/e2e/v1/sessions`,
      `${edge}/devices/${deviceId}/e2e/v1/requests`,
      `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
      `${edge}/devices/${deviceId}/e2e/v1/sessions`,
      `${edge}/devices/${deviceId}/e2e/v1/requests`,
    ]);
  },
);

test.each([0, 1])(
  "cancelling waiter %i does not cancel another caller's handshake",
  async (cancelled) => {
    let resolveHandshake!: (response: Response) => void;
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(cloudResponse())
      .mockImplementationOnce(
        () =>
          new Promise<Response>((resolve) => {
            resolveHandshake = resolve;
          }),
      );
    vi.stubGlobal("fetch", fetchMock);
    const controllers = [new AbortController(), new AbortController()];
    const connections = controllers.map((controller) =>
      connectPublicDevice(handle, controller.signal),
    );
    const rejected = expect(connections[cancelled]).rejects.toMatchObject({ name: "AbortError" });
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    controllers[cancelled].abort();
    await rejected;
    expect(fetchMock.mock.calls[1][1].signal.aborted).toBe(false);
    resolveHandshake(new Response("confirmation"));
    await expect(connections[1 - cancelled]).resolves.toMatchObject({ deviceId });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  },
);

test("staggered concurrent session rejections share one replacement handshake", async () => {
  let rejectSecond!: (response: Response) => void;
  let confirmReplacement!: (response: Response) => void;
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockResolvedValueOnce(new Response(null, { status: 409 }))
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          rejectSecond = resolve;
        }),
    )
    .mockResolvedValueOnce(cloudResponse())
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          confirmReplacement = resolve;
        }),
    )
    .mockImplementation(async () => new Response("ciphertext"));
  vi.stubGlobal("fetch", fetchMock);
  setPublicApiTarget(await connectPublicDevice(handle));
  const requests = [e2eFetch("/api/v1/workspaces"), e2eFetch("/api/v1/tasks")];
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(6));
  rejectSecond(new Response(null, { status: 401 }));
  await Promise.resolve();
  confirmReplacement(new Response("new confirmation"));
  const responses = await Promise.all(requests);
  expect(await Promise.all(responses.map((response) => response.text()))).toEqual(["ok", "ok"]);
  expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/connect"))).toHaveLength(2);
  expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/sessions"))).toHaveLength(2);
});
