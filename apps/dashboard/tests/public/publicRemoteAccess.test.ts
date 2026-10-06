import { afterEach, expect, test, vi } from "vitest";
import {
  dashboardBootstrapUrl,
  dashboardSignInUrl,
  listPublicDevices,
  requestPublicDeviceConnection,
  CloudRequestError,
} from "../../src/modes/public/remoteAccess";
import {
  clearPublicApiTarget,
  publicApiSignal,
  setPublicApiTarget,
} from "../../src/modes/public/apiTarget";

const handle = "office-mac";
const deviceId = "01234567-89ab-cdef-0123-456789abcdef";
const edgeApiOrigin = "https://brave-silver-atlas.edge.pontia.dev";
const browserPublicKey = btoa(String.fromCharCode(...new Uint8Array(32).fill(9))).replace(
  /=+$/,
  "",
);
const connection = {
  device_handle: handle,
  device_id: deviceId,
  edge_api_origin: edgeApiOrigin,
  device_public_key: browserPublicKey,
  device_key_version: 1,
  capability: btoa(String.fromCharCode(...new Uint8Array(169).fill(7))).replace(/=+$/, ""),
};

afterEach(() => {
  clearPublicApiTarget();
  vi.unstubAllGlobals();
});

function json(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

test("loads the strict Cloud device list with browser credentials", async () => {
  const fetchMock = vi.fn(async () =>
    json([
      { device_handle: handle, name: "Office Mac" },
      { device_handle: "travel-laptop", name: "Travel Laptop" },
    ]),
  );
  vi.stubGlobal("fetch", fetchMock);
  await expect(listPublicDevices()).resolves.toEqual([
    { handle, name: "Office Mac" },
    { handle: "travel-laptop", name: "Travel Laptop" },
  ]);
  expect(fetchMock).toHaveBeenCalledWith("https://pontia.dev/api/dashboard/devices", {
    credentials: "include",
    signal: undefined,
  });
});

test.each([
  [{ device_handle: handle, name: "Office Mac", online: true }],
  [{ device_handle: "Invalid", name: "Office Mac" }],
  [{ device_handle: handle, name: " Office Mac" }],
])("rejects an invalid Cloud device list", async (body) => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => json(body)),
  );
  await expect(listPublicDevices()).rejects.toBeInstanceOf(CloudRequestError);
});

test("gets routing and authorization together using the browser key and login", async () => {
  const fetchMock = vi.fn(async () => json(connection));
  vi.stubGlobal("fetch", fetchMock);
  await expect(requestPublicDeviceConnection(handle, browserPublicKey)).resolves.toEqual({
    target: { handle, deviceId, edgeApiOrigin },
    devicePublicKey: browserPublicKey,
    capability: connection.capability,
  });
  expect(fetchMock).toHaveBeenCalledWith(
    `https://pontia.dev/api/dashboard/devices/${handle}/connect`,
    {
      method: "POST",
      credentials: "include",
      signal: undefined,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ browser_public_key: browserPublicKey }),
    },
  );
});

test("custom port survives connection discovery", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => json({ ...connection, edge_api_origin: `${edgeApiOrigin}:8443` })),
  );
  const result = await requestPublicDeviceConnection(handle, browserPublicKey);
  expect(result.target.edgeApiOrigin).toBe(`${edgeApiOrigin}:8443`);
});

test.each([
  { device_handle: "other-device" },
  { device_id: deviceId.toUpperCase() },
  { edge_api_origin: "https://attacker.example" },
  { edge_api_origin: "https://one.two.edge.pontia.dev" },
  { edge_api_origin: `${edgeApiOrigin}/api` },
  { edge_api_origin: "https://user@brave-silver-atlas.edge.pontia.dev" },
  { redirect: "https://attacker.example" },
  { device_public_key: "invalid" },
  { capability: "invalid" },
  { device_key_version: 0 },
  { device_key_version: 1.5 },
])("rejects invalid connection metadata %j", async (override) => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => json({ ...connection, ...override })),
  );
  await expect(requestPublicDeviceConnection(handle, browserPublicKey)).rejects.toBeInstanceOf(
    CloudRequestError,
  );
});

test("preserves Cloud authentication failure status", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => json({ error: "unauthorized" }, 401)),
  );
  await expect(requestPublicDeviceConnection(handle, browserPublicKey)).rejects.toMatchObject({
    status: 401,
  });
});

test("constructs a Cloud sign-in URL that returns only to the public Dashboard", () => {
  expect(dashboardSignInUrl(`/${handle}/workspaces?view=recent#active`)).toBe(
    `https://pontia.dev/login?return_to=${encodeURIComponent(`https://app.pontia.dev/${handle}/workspaces?view=recent#active`)}`,
  );
  expect(() => dashboardSignInUrl("//attacker.example/path")).toThrow();
});

test("constructs a direct public Dashboard device path", () => {
  expect(dashboardBootstrapUrl(handle)).toBe(`https://app.pontia.dev/${handle}`);
});

test("changing target aborts requests bound to the previous target", () => {
  setPublicApiTarget({ handle, deviceId, edgeApiOrigin });
  const oldSignal = publicApiSignal();
  setPublicApiTarget({
    handle: "travel-laptop",
    deviceId: "12345678-9abc-def0-1234-56789abcdef0",
    edgeApiOrigin: "https://calm-blue-arthur.edge.pontia.dev",
  });
  expect(oldSignal.aborted).toBe(true);
  expect(publicApiSignal().aborted).toBe(false);
});
