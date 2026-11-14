import { afterEach, beforeEach, expect, test, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(),
  restore: vi.fn(),
  start: vi.fn(),
  sessionFree: vi.fn(),
  handshakeFree: vi.fn(),
  identityFree: vi.fn(),
  content: vi.fn((bytes: Uint8Array) => bytes),
}));

vi.mock("../../src/e2e-wasm/pontia_e2e", () => ({
  default: vi.fn(async () => undefined),
  Session: { restore: (...args: unknown[]) => mocks.restore(...args) },
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
  createRemoteConnection,
  retainSavedConnections,
} from "../../src/modes/public/remote-access";
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
      admission_ticket: "admission-ticket",
    }),
    { headers: { "Content-Type": "application/json" } },
  );

const admissionResponse = (token = "admission-token") => Response.json({ token });

beforeEach(() => {
  vi.clearAllMocks();
  sessionStorage.clear();
  mocks.restore.mockImplementation(() => mocks.confirm());
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId);
  mocks.start.mockReturnValue({
    bytes: () => new Uint8Array([1]),
    confirm: mocks.confirm,
    free: mocks.handshakeFree,
  });
  mocks.confirm.mockImplementation(() => ({
    free: mocks.sessionFree,
    export: () => new Uint8Array([1, 2, 3]),
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
    .mockResolvedValueOnce(admissionResponse())
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
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(3));
  expect(completed).toBe(false);
  expect(fetchMock.mock.calls[0][0]).toBe(
    `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
  );
  expect(fetchMock.mock.calls[2][0]).toBe(`${edge}/devices/${deviceId}/e2e/v1/sessions`);
  resolveHandshake(new Response(new Uint8Array([4])));
  await expect(connecting).resolves.toEqual({ handle, deviceId, edgeApiOrigin: edge });
  expect(mocks.confirm).toHaveBeenCalledWith(new Uint8Array([4]));
  await remote.connect();
  expect(fetchMock).toHaveBeenCalledTimes(3);
});

test("shares one connection establishment across concurrent callers", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  const targets = await Promise.all([remote.connect(), remote.connect()]);
  expect(targets[0]).toEqual(targets[1]);
  expect(fetchMock).toHaveBeenCalledTimes(3);
});

test("a failed handshake can be retried with fresh authorization", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response(null, { status: 503 }))
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  await expect(remote.connect()).rejects.toThrow();
  expect(mocks.confirm).not.toHaveBeenCalled();
  await expect(remote.connect()).resolves.toMatchObject({ deviceId });
  expect(fetchMock).toHaveBeenCalledTimes(6);
});

test("a cancelled handshake cannot install a stale session", async () => {
  let resolveHandshake!: (response: Response) => void;
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
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
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(3));
  controller.abort();
  resolveHandshake(new Response("old confirmation"));
  await rejected;
  expect(mocks.confirm).not.toHaveBeenCalled();
  fetchMock
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("new confirmation"));
  await expect(remote.connect()).resolves.toMatchObject({ deviceId });
  expect(fetchMock).toHaveBeenCalledTimes(6);
});

test.each([409])(
  "reauthorizes rejected encrypted requests through connect (%i)",
  async (status) => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(admissionResponse())
      .mockResolvedValueOnce(new Response("confirmation"))
      .mockResolvedValueOnce(new Response(null, { status }))
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(admissionResponse())
      .mockResolvedValueOnce(new Response("confirmation"))
      .mockResolvedValueOnce(new Response("ciphertext"));
    vi.stubGlobal("fetch", fetchMock);
    await remote.connect();
    const response = await remote.e2eFetch("/api/v1/workspaces");
    expect(await response.text()).toBe("ok");
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
      `${edge}/browser-admissions`,
      `${edge}/devices/${deviceId}/e2e/v1/sessions`,
      `${edge}/devices/${deviceId}/e2e/v1/requests`,
      `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
      `${edge}/browser-admissions`,
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
      .mockResolvedValueOnce(admissionResponse())
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
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(3));
    controllers[cancelled].abort();
    await rejected;
    expect(fetchMock.mock.calls[2][1].signal.aborted).toBe(false);
    resolveHandshake(new Response("confirmation"));
    await expect(connections[1 - cancelled]).resolves.toMatchObject({ deviceId });
    expect(fetchMock).toHaveBeenCalledTimes(3);
  },
);

test("staggered concurrent session rejections share one replacement handshake", async () => {
  let rejectSecond!: (response: Response) => void;
  let confirmReplacement!: (response: Response) => void;
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockResolvedValueOnce(new Response(null, { status: 409 }))
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          rejectSecond = resolve;
        }),
    )
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          confirmReplacement = resolve;
        }),
    )
    .mockImplementation(async () => new Response("ciphertext"));
  vi.stubGlobal("fetch", fetchMock);
  await remote.connect();
  const requests = [
    remote.e2eFetch("/api/v1/workspaces"),
    remote.e2eFetch("/api/v1/sessions/overview"),
  ];
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(8));
  rejectSecond(new Response(null, { status: 409 }));
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
    .mockResolvedValueOnce(admissionResponse())
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
  const other = createRemoteConnection(
    { handle: otherHandle, deviceId: otherId, edgeId: deviceId },
    deviceId,
  );
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
    if (url.endsWith("/browser-admissions")) return admissionResponse(url);
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
      expect(remote.e2eFetch("/api/v1/sessions/same")).rejects.toThrow(),
      other.e2eFetch("/api/v1/sessions/same").then((response) => response.text()),
    ]);
    const urls = fetchMock.mock.calls.map(([url]) => url);
    expect(
      urls.filter((url) => url === `${edge}/devices/${deviceId}/e2e/v1/requests`),
    ).toHaveLength(1);
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
    remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId, expired);
    const fetchMock = vi.fn();
    if (phase === "renewal")
      fetchMock
        .mockResolvedValueOnce(cloudResponse())
        .mockResolvedValueOnce(admissionResponse())
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

test("an admission rejection reconnects without replaying the business request", async () => {
  const failed = vi.fn();
  remote.dispose();
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId, failed);
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("first-token"))
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockResolvedValueOnce(Response.json({ error: "invalid_admission_token" }, { status: 401 }))
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("second-token"))
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockResolvedValueOnce(new Response("ciphertext"));
  vi.stubGlobal("fetch", fetchMock);
  await expect(
    remote.e2eFetch("/api/v1/sessions", { method: "POST", body: "business action" }),
  ).rejects.toThrow();
  expect(fetchMock.mock.calls.filter(([url]) => url.endsWith("/requests"))).toHaveLength(1);
  expect(fetchMock.mock.calls[2][1].headers.Authorization).toBe("Bearer first-token");
  expect(fetchMock.mock.calls[3][1].headers.Authorization).toBe("Bearer first-token");
  expect(fetchMock.mock.calls[6][1].headers.Authorization).toBe("Bearer second-token");
  expect(await (await remote.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  expect(fetchMock.mock.calls[7][1].headers.Authorization).toBe("Bearer second-token");
  expect(failed).not.toHaveBeenCalled();
});

test("a failed renewal reports the device connection failure", async () => {
  const failed = vi.fn();
  remote.dispose();
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId, failed);
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(admissionResponse())
      .mockResolvedValueOnce(new Response("confirmation"))
      .mockResolvedValueOnce(new Response(null, { status: 409 }))
      .mockResolvedValueOnce(cloudResponse())
      .mockResolvedValueOnce(admissionResponse())
      .mockResolvedValueOnce(new Response(null, { status: 503 })),
  );
  await remote.connect();
  await expect(remote.e2eFetch("/api/v1/workspaces")).rejects.toThrow();
  expect(failed).toHaveBeenCalledOnce();
});

test("failed ticket exchange starts again with central authorization", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(new Response(null, { status: 401 }))
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  await expect(remote.connect()).rejects.toThrow();
  expect(mocks.start).not.toHaveBeenCalled();
  await expect(remote.connect()).resolves.toMatchObject({ deviceId });
  expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
    `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
    `${edge}/browser-admissions`,
    `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
    `${edge}/browser-admissions`,
    `${edge}/devices/${deviceId}/e2e/v1/sessions`,
  ]);
  expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toEqual({ ticket: "admission-ticket" });
});

test("refresh restores the saved admission and session without another handshake", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("first-token"))
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockImplementation(async () => new Response("ciphertext"));
  vi.stubGlobal("fetch", fetchMock);
  await remote.connect();
  remote.dispose();
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId);
  expect(await (await remote.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  expect(fetchMock).toHaveBeenCalledTimes(4);
  expect(fetchMock.mock.calls[3][0]).toBe(`${edge}/devices/${deviceId}/e2e/v1/requests`);
  expect(fetchMock.mock.calls[3][1].headers.Authorization).toBe("Bearer first-token");
  expect(mocks.restore).toHaveBeenCalledWith(new Uint8Array([1, 2, 3]));
});

test("admission rejection aborts the previous connection's streams", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockResolvedValueOnce(new Response("ciphertext"))
    .mockResolvedValueOnce(new Response(null, { status: 401 }))
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("replacement"))
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  const response = await remote.e2eFetch("/api/v1/events");
  const streamSignal = fetchMock.mock.calls[3][1].signal as AbortSignal;
  expect(streamSignal.aborted).toBe(false);
  await expect(remote.e2eFetch("/api/v1/workspaces")).rejects.toThrow();
  expect(streamSignal.aborted).toBe(true);
  await response.body?.cancel();
});

test("delayed admission rejections share a replacement without replaying either request", async () => {
  let rejectSecond!: (response: Response) => void;
  let confirmReplacement!: (response: Response) => void;
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockResolvedValueOnce(new Response(null, { status: 401 }))
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          rejectSecond = resolve;
        }),
    )
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("replacement"))
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          confirmReplacement = resolve;
        }),
    );
  vi.stubGlobal("fetch", fetchMock);
  await remote.connect();
  const requests = Promise.allSettled([
    remote.e2eFetch("/api/v1/workspaces"),
    remote.e2eFetch("/api/v1/sessions/overview"),
  ]);
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(8));
  rejectSecond(new Response(null, { status: 401 }));
  confirmReplacement(new Response("confirmation"));
  expect((await requests).map((r) => r.status)).toEqual(["rejected", "rejected"]);
  expect(fetchMock.mock.calls.filter(([url]) => url.endsWith("/requests"))).toHaveLength(2);
  expect(fetchMock.mock.calls.filter(([url]) => url.endsWith("/sessions"))).toHaveLength(2);
});

function successfulConnectionFetch() {
  return vi
    .fn()
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
}

test("a network interruption preserves the session for the replacement connection", async () => {
  const fetchMock = successfulConnectionFetch()
    .mockRejectedValueOnce(new TypeError("Failed to fetch"))
    .mockResolvedValueOnce(new Response("ciphertext"));
  vi.stubGlobal("fetch", fetchMock);
  await remote.connect();
  await expect(
    remote.e2eFetch("/api/v1/sessions", { method: "POST", body: "action" }),
  ).rejects.toThrow();
  remote.dispose();
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId);
  expect(await (await remote.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  expect(fetchMock.mock.calls.filter(([url]) => url.endsWith("/sessions"))).toHaveLength(1);
  expect(fetchMock.mock.calls.filter(([url]) => url.endsWith("/requests"))).toHaveLength(2);
});

test.each(["json", "version", "target", "session"])("replaces a corrupt %s cache", async (kind) => {
  const fetchMock = successfulConnectionFetch();
  vi.stubGlobal("fetch", fetchMock);
  await remote.connect();
  remote.dispose();
  const key = sessionStorage.key(0)!;
  const saved = JSON.parse(sessionStorage.getItem(key)!);
  if (kind === "version") saved.version = 99;
  if (kind === "target") saved.target.edgeApiOrigin = "https://attacker.example";
  if (kind === "session")
    mocks.restore.mockImplementationOnce(() => {
      throw new Error("invalid snapshot");
    });
  sessionStorage.setItem(key, kind === "json" ? "broken json" : JSON.stringify(saved));
  fetchMock
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("replacement"))
    .mockResolvedValueOnce(new Response("confirmation"));
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId);
  await remote.connect();
  expect(fetchMock).toHaveBeenCalledTimes(6);
  expect(JSON.parse(sessionStorage.getItem(key)!).token).toBe("replacement");
});

test("another user cannot restore a saved connection for the same handle", async () => {
  vi.stubGlobal("fetch", successfulConnectionFetch());
  await remote.connect();
  remote.dispose();
  const otherUser = "12345678-9abc-def0-1234-56789abcdef0";
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, otherUser);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(null, { status: 401 })),
  );
  await expect(remote.connect()).rejects.toMatchObject({ status: 401 });
  expect(mocks.restore).not.toHaveBeenCalled();
});

test("a user change during authorization cannot install credentials under the previous user", async () => {
  const body = await cloudResponse().json();
  const fetchMock = vi.fn(async () =>
    Response.json({ ...body, user_id: "12345678-9abc-def0-1234-56789abcdef0" }),
  );
  vi.stubGlobal("fetch", fetchMock);
  await expect(remote.connect()).rejects.toMatchObject({ status: 401 });
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(sessionStorage.length).toBe(0);
});

test("reconciles saved connections on user changes, removal and logout without clearing unrelated storage", async () => {
  vi.stubGlobal("fetch", successfulConnectionFetch());
  await remote.connect();
  sessionStorage.setItem("unrelated", "keep");
  const key = [...Array(sessionStorage.length)]
    .map((_, i) => sessionStorage.key(i)!)
    .find((key) => key !== "unrelated")!;
  const saved = sessionStorage.getItem(key)!;
  retainSavedConnections(deviceId, [{ handle, deviceId, edgeId: deviceId }]);
  expect(sessionStorage.getItem(key)).toBe(saved);
  for (const [user, handles] of [
    [deviceId, []],
    ["other-user", [handle]],
    [null, []],
  ] as const) {
    sessionStorage.setItem(key, saved);
    retainSavedConnections(
      user,
      handles.map((handle) => ({ handle, deviceId, edgeId: deviceId })),
    );
    expect(sessionStorage.getItem(key)).toBeNull();
    expect(sessionStorage.getItem("unrelated")).toBe("keep");
  }
});

test("a late rejection from a disposed connection cannot erase its replacement cache", async () => {
  let rejectOld!: (response: Response) => void;
  const fetchMock = successfulConnectionFetch()
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          rejectOld = resolve;
        }),
    )
    .mockResolvedValueOnce(new Response(null, { status: 409 }))
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("replacement"))
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockImplementation(async () => new Response("ciphertext"));
  vi.stubGlobal("fetch", fetchMock);
  await remote.connect();
  const pending = remote.e2eFetch("/api/v1/workspaces");
  const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(4));
  remote.dispose();
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId);
  expect(await (await remote.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  rejectOld(new Response(null, { status: 401 }));
  await rejected;
  remote.dispose();
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId);
  expect(await (await remote.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  expect(fetchMock.mock.calls.filter(([url]) => url.endsWith("/sessions"))).toHaveLength(2);
  expect(fetchMock.mock.calls.at(-1)![1].headers.Authorization).toBe("Bearer replacement");
});

test("a device capacity rejection discards the saved session without replaying the request", async () => {
  const fetchMock = successfulConnectionFetch().mockResolvedValueOnce(
    new Response(null, { status: 429 }),
  );
  vi.stubGlobal("fetch", fetchMock);
  await expect(
    remote.e2eFetch("/api/v1/sessions", { method: "POST", body: "action" }),
  ).rejects.toThrow();
  expect(sessionStorage.length).toBe(0);
  remote.dispose();
  fetchMock
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId);
  await remote.connect();
  expect(fetchMock.mock.calls.filter(([url]) => url.endsWith("/requests"))).toHaveLength(1);
  expect(fetchMock.mock.calls.filter(([url]) => url.endsWith("/sessions"))).toHaveLength(2);
});

test("storage policy denial leaves live connections usable", async () => {
  const storage = vi.spyOn(window, "sessionStorage", "get").mockImplementation(() => {
    throw new DOMException("Storage disabled", "SecurityError");
  });
  try {
    vi.stubGlobal(
      "fetch",
      successfulConnectionFetch().mockResolvedValueOnce(new Response("ciphertext")),
    );
    expect(await (await remote.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  } finally {
    storage.mockRestore();
  }
});

test("a reassigned handle cannot restore the previous device's credentials", async () => {
  vi.stubGlobal("fetch", successfulConnectionFetch());
  await remote.connect();
  remote.dispose();
  const replacementId = "12345678-9abc-def0-1234-56789abcdef0";
  retainSavedConnections(deviceId, [{ handle, deviceId: replacementId, edgeId: deviceId }]);
  expect(sessionStorage.length).toBe(0);
  remote = createRemoteConnection({ handle, deviceId: replacementId, edgeId: deviceId }, deviceId);
  const body = await cloudResponse().json();
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(Response.json({ ...body, device_id: replacementId }))
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  await expect(remote.connect()).resolves.toMatchObject({ deviceId: replacementId });
  expect(mocks.restore).not.toHaveBeenCalled();
  expect(fetchMock.mock.calls[2][0]).toBe(`${edge}/devices/${replacementId}/e2e/v1/sessions`);
});

test("a late rejection of an already retried request leaves a newer session online", async () => {
  const failure = vi.fn();
  remote.dispose();
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId, failure);
  let rejectRetry!: (response: Response) => void;
  const fetchMock = successfulConnectionFetch()
    .mockResolvedValueOnce(new Response(null, { status: 409 }))
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("second"))
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          rejectRetry = resolve;
        }),
    )
    .mockResolvedValueOnce(new Response(null, { status: 401 }))
    .mockResolvedValueOnce(cloudResponse())
    .mockResolvedValueOnce(admissionResponse("third"))
    .mockResolvedValueOnce(new Response("confirmation"))
    .mockResolvedValueOnce(new Response("ciphertext"));
  vi.stubGlobal("fetch", fetchMock);
  const pending = remote.e2eFetch("/api/v1/workspaces");
  const rejected = expect(pending).rejects.toThrow();
  await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(8));
  await expect(remote.e2eFetch("/api/v1/workspaces")).rejects.toThrow();
  rejectRetry(new Response(null, { status: 409 }));
  await rejected;
  expect(failure).not.toHaveBeenCalled();
  remote.dispose();
  remote = createRemoteConnection({ handle, deviceId, edgeId: deviceId }, deviceId, failure);
  expect(await (await remote.e2eFetch("/api/v1/workspaces")).text()).toBe("ok");
  expect(fetchMock).toHaveBeenCalledTimes(13);
  expect(fetchMock.mock.calls[12][1].headers.Authorization).toBe("Bearer third");
});

test("changing edge binding discards credentials routed to the previous edge", async () => {
  vi.stubGlobal("fetch", successfulConnectionFetch());
  await remote.connect();
  remote.dispose();
  const device = { handle, deviceId, edgeId: "12345678-9abc-def0-1234-56789abcdef0" };
  retainSavedConnections(deviceId, [device]);
  expect(sessionStorage.length).toBe(0);
  const body = await cloudResponse().json();
  const newEdge = "https://other.edge.pontia.dev";
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(Response.json({ ...body, edge_api_origin: newEdge }))
    .mockResolvedValueOnce(admissionResponse())
    .mockResolvedValueOnce(new Response("confirmation"));
  vi.stubGlobal("fetch", fetchMock);
  remote = createRemoteConnection(device, deviceId);
  await expect(remote.connect()).resolves.toMatchObject({ edgeApiOrigin: newEdge });
  expect(mocks.restore).not.toHaveBeenCalled();
  expect(fetchMock.mock.calls[2][0]).toBe(`${newEdge}/devices/${deviceId}/e2e/v1/sessions`);
});
