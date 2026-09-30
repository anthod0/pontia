import { afterEach, describe, expect, test, vi } from "vitest";
import {
  dashboardBootstrapUrl,
  listPublicDevices,
  resolvePublicDeviceTarget,
} from "../src/modes/public/remoteAccess";
import {
  clearPublicApiTarget,
  publicApiSignal,
  publicApiUrl,
  setPublicApiTarget,
} from "../src/modes/public/apiTarget";

const handle = "office-mac";
const deviceId = "01234567-89ab-cdef-0123-456789abcdef";
const edgeApiOrigin = "https://brave-silver-atlas.edge.pontia.dev";

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
  await expect(listPublicDevices()).rejects.toThrow("invalid device list");
});

test("resolves a matching immutable target with browser credentials", async () => {
  const fetchMock = vi.fn(async () =>
    json({ device_handle: handle, device_id: deviceId, edge_api_origin: edgeApiOrigin }),
  );
  vi.stubGlobal("fetch", fetchMock);

  await expect(resolvePublicDeviceTarget(handle)).resolves.toEqual({
    handle,
    deviceId,
    edgeApiOrigin,
  });
  expect(fetchMock).toHaveBeenCalledWith(
    `https://pontia.dev/api/dashboard/devices/${handle}/target`,
    { credentials: "include", signal: undefined },
  );
});

describe("target validation", () => {
  test.each([
    [
      "a different handle",
      { device_handle: "other-device", device_id: deviceId, edge_api_origin: edgeApiOrigin },
    ],
    [
      "a non-canonical UUID",
      { device_handle: handle, device_id: deviceId.toUpperCase(), edge_api_origin: edgeApiOrigin },
    ],
    [
      "an external origin",
      { device_handle: handle, device_id: deviceId, edge_api_origin: "https://attacker.example" },
    ],
    [
      "a nested edge hostname",
      {
        device_handle: handle,
        device_id: deviceId,
        edge_api_origin: "https://one.two.edge.pontia.dev",
      },
    ],
    [
      "a non-default port",
      { device_handle: handle, device_id: deviceId, edge_api_origin: `${edgeApiOrigin}:8443` },
    ],
    [
      "a path",
      { device_handle: handle, device_id: deviceId, edge_api_origin: `${edgeApiOrigin}/api` },
    ],
    [
      "credentials",
      {
        device_handle: handle,
        device_id: deviceId,
        edge_api_origin: "https://user@brave-silver-atlas.edge.pontia.dev",
      },
    ],
    [
      "unknown fields",
      {
        device_handle: handle,
        device_id: deviceId,
        edge_api_origin: edgeApiOrigin,
        redirect: "https://attacker.example",
      },
    ],
  ])("rejects %s", async (_name, body) => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => json(body)),
    );
    await expect(resolvePublicDeviceTarget(handle)).rejects.toThrow("invalid device target");
  });
});

test("constructs fixed bootstrap and edge proxy paths without bearer authentication", () => {
  setPublicApiTarget({ handle, deviceId, edgeApiOrigin });

  expect(dashboardBootstrapUrl(handle)).toBe(
    `https://pontia.dev/api/dashboard/devices/${handle}/bootstrap`,
  );
  expect(publicApiUrl("/api/v1/workspaces?limit=5")).toBe(
    `${edgeApiOrigin}/devices/${deviceId}/api/v1/workspaces?limit=5`,
  );
  expect(() => publicApiUrl("https://attacker.example/api/v1/workspaces")).toThrow();
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
