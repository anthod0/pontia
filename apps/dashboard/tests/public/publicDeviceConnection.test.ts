import { afterEach, beforeEach, expect, test, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(),
  start: vi.fn(),
  sessionFree: vi.fn(),
  handshakeFree: vi.fn(),
  identityFree: vi.fn(),
  content: vi.fn((bytes: Uint8Array) => bytes),
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

import { createRemoteConnection } from "../../src/modes/public/remote-access";
let remote: ReturnType<typeof createRemoteConnection>;

const handle = "office-mac";
const deviceId = "01234567-89ab-cdef-0123-456789abcdef";
const edge = "https://brave-atlas.edge.pontia.dev";
const encoded = (length: number) =>
  btoa(String.fromCharCode(...new Uint8Array(length).fill(9))).replace(/=+$/, "");
const cloudResponse = () =>
  new Response(
    JSON.stringify({
      user_id: deviceId,
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
  remote = createRemoteConnection(handle);
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
        content: mocks.content,
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
  remote.dispose();
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
  const connecting = remote.connect().then((target) => {
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
  await remote.connect();
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

test("shares one connection establishment across concurrent callers", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  const targets = await Promise.all([remote.connect(), remote.connect()]);
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
  await expect(remote.connect()).rejects.toThrow();
  expect(mocks.confirm).not.toHaveBeenCalled();
  await expect(remote.connect()).resolves.toMatchObject({ deviceId });
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
  const connecting = remote.connect(controller.signal);
  const rejected = expect(connecting).rejects.toMatchObject({ name: "AbortError" });
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  controller.abort();
  resolveHandshake(new Response("old confirmation"));
  await rejected;
  expect(mocks.confirm).not.toHaveBeenCalled();
  fetchMock
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response("new confirmation"));
  await expect(remote.connect()).resolves.toMatchObject({ deviceId });
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
    await remote.connect();
    const response = await remote.e2eFetch("/api/v1/workspaces");
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
    const connections = controllers.map((controller) => remote.connect(controller.signal));
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
  await remote.connect();
  const requests = [remote.e2eFetch("/api/v1/workspaces"), remote.e2eFetch("/api/v1/tasks")];
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(6));
  rejectSecond(new Response(null, { status: 401 }));
  await Promise.resolve();
  confirmReplacement(new Response("new confirmation"));
  const responses = await Promise.all(requests);
  expect(await Promise.all(responses.map((response) => response.text()))).toEqual(["ok", "ok"]);
  expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/connect"))).toHaveLength(2);
  expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/sessions"))).toHaveLength(2);
});

test("encrypts binary request bytes without a text conversion", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response(new Uint8Array([1])))
    .mockResolvedValueOnce(new Response(new Uint8Array([2])));
  vi.stubGlobal("fetch", fetchMock);
  const bytes = new Uint8Array([0, 255, 128, 10]);
  await (
    await remote.e2eFetch("/api/v1/uploads/id?offset=0", { method: "PATCH", body: bytes })
  ).text();
  expect(mocks.content).toHaveBeenCalledWith(bytes);
});

test("devices on the same edge keep independent encrypted sessions and reauthorization", async () => {
  const otherHandle = "travel-laptop";
  const otherId = "12345678-9abc-def0-1234-56789abcdef0";
  const other = createRemoteConnection(otherHandle);
  let rejectFirstDevice = false;
  const fetchMock = vi.fn(async (url: string) => {
    if (url.includes("/connect")) {
      if (url.includes(otherHandle)) {
        const body = await cloudResponse().json();
        return new Response(
          JSON.stringify({ ...body, device_handle: otherHandle, device_id: otherId }),
        );
      }
      return cloudResponse();
    }
    if (url.endsWith("/sessions")) return new Response("confirmation");
    if (rejectFirstDevice && url.includes(deviceId)) {
      rejectFirstDevice = false;
      return new Response(null, { status: 401 });
    }
    return new Response("ciphertext");
  });
  vi.stubGlobal("fetch", fetchMock);
  try {
    await Promise.all([remote.connect(), other.connect()]);
    rejectFirstDevice = true;
    await Promise.all([
      remote.e2eFetch("/api/v1/sessions/same").then((response) => response.text()),
      other.e2eFetch("/api/v1/sessions/same").then((response) => response.text()),
    ]);
    const urls = fetchMock.mock.calls.map(([url]) => url);
    expect(
      urls.filter((url) => url === `${edge}/devices/${deviceId}/e2e/v1/requests`),
    ).toHaveLength(2);
    expect(urls.filter((url) => url === `${edge}/devices/${otherId}/e2e/v1/requests`)).toHaveLength(
      1,
    );
    expect(urls.filter((url) => url.endsWith(`/${handle}/connect`))).toHaveLength(2);
    expect(urls.filter((url) => url.endsWith(`/${otherHandle}/connect`))).toHaveLength(1);
    remote.dispose();
    expect(await (await other.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  } finally {
    other.dispose();
  }
});

test.each(["initial", "renewal"])(
  "reports central 401 during %s authorization so the tab can terminate all access",
  async (phase) => {
    const expired = vi.fn(() => remote.dispose());
    remote.dispose();
    remote = createRemoteConnection(handle, expired);
    const fetchMock = vi.fn();
    if (phase === "renewal")
      fetchMock
        .mockResolvedValueOnce(cloudResponse())
        .mockResolvedValueOnce(new Response("confirmation"))
        .mockResolvedValueOnce(new Response(null, { status: 409 }));
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 401 }));
    vi.stubGlobal("fetch", fetchMock);
    const request = phase === "initial" ? remote.connect() : remote.e2eFetch("/api/v1/workspaces");
    await expect(request).rejects.toBeInstanceOf(Error);
    expect(expired).toHaveBeenCalledOnce();
    expect(remote.signal.aborted).toBe(true);
    const requests = fetchMock.mock.calls.length;
    await expect(remote.e2eFetch("/api/v1/workspaces")).rejects.toMatchObject({
      name: "AbortError",
    });
    expect(fetchMock).toHaveBeenCalledTimes(requests);
  },
);

test("an edge rejection renews only that connection without reporting central login expiry", async () => {
  const expired = vi.fn();
  remote.dispose();
  remote = createRemoteConnection(handle, expired);
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(new Response("confirmation"))
      .mockResolvedValueOnce(new Response(null, { status: 401 }))
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(new Response("confirmation"))
      .mockResolvedValueOnce(new Response("ciphertext")),
  );
  expect(await (await remote.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  expect(expired).not.toHaveBeenCalled();
});

test("a failed renewal reports the device connection failure", async () => {
  const failed = vi.fn();
  remote.dispose();
  remote = createRemoteConnection(handle, failed);
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(new Response("confirmation"))
      .mockResolvedValueOnce(new Response(null, { status: 409 }))
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(new Response(null, { status: 503 })),
  );
  await remote.connect();
  await expect(remote.e2eFetch("/api/v1/workspaces")).rejects.toThrow();
  expect(failed).toHaveBeenCalledOnce();
});
