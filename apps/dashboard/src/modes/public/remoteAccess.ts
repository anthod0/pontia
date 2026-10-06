import { isValidDeviceHandle } from "$lib/remoteDashboard";

const CLOUD_ORIGIN = "https://pontia.dev";
const PUBLIC_DASHBOARD_ORIGIN = "https://app.pontia.dev";
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const EDGE_HOST = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.edge\.pontia\.dev$/;

export type PublicDevice = {
  handle: string;
  name: string;
};

export type PublicDeviceTarget = {
  handle: string;
  deviceId: string;
  edgeApiOrigin: string;
};

export class CloudRequestError extends Error {
  constructor(
    message: string,
    readonly status?: number,
  ) {
    super(message);
    this.name = "CloudRequestError";
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasOnlyKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value).sort();
  return (
    actual.length === keys.length && actual.every((key, index) => key === [...keys].sort()[index])
  );
}

function parseDevice(value: unknown): PublicDevice {
  if (!isRecord(value) || !hasOnlyKeys(value, ["device_handle", "name"])) {
    throw new CloudRequestError("Cloud returned an invalid device list.");
  }
  const handle = value.device_handle;
  const name = value.name;
  if (
    typeof handle !== "string" ||
    !isValidDeviceHandle(handle) ||
    typeof name !== "string" ||
    name.length === 0 ||
    name.length > 255 ||
    name.trim() !== name
  ) {
    throw new CloudRequestError("Cloud returned an invalid device list.");
  }
  return { handle, name };
}

function parseEdgeOrigin(value: unknown): string {
  if (typeof value !== "string") {
    throw new CloudRequestError("Cloud returned an invalid device target.");
  }
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new CloudRequestError("Cloud returned an invalid device target.");
  }
  if (
    value !== url.origin ||
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.pathname !== "/" ||
    url.search ||
    url.hash ||
    !EDGE_HOST.test(url.hostname)
  ) {
    throw new CloudRequestError("Cloud returned an invalid device target.");
  }
  return value;
}

async function cloudJson(url: string, signal?: AbortSignal): Promise<unknown> {
  const response = await fetch(url, { credentials: "include", signal });
  if (!response.ok) {
    throw new CloudRequestError("Cloud request failed.", response.status);
  }
  try {
    return await response.json();
  } catch {
    throw new CloudRequestError("Cloud returned an invalid response.", response.status);
  }
}

export async function listPublicDevices(signal?: AbortSignal): Promise<PublicDevice[]> {
  const value = await cloudJson(`${CLOUD_ORIGIN}/api/dashboard/devices`, signal);
  if (!Array.isArray(value)) throw new CloudRequestError("Cloud returned an invalid device list.");
  const devices = value.map(parseDevice);
  if (new Set(devices.map((device) => device.handle)).size !== devices.length) {
    throw new CloudRequestError("Cloud returned an invalid device list.");
  }
  return devices;
}

export type PublicDeviceConnection = {
  target: PublicDeviceTarget;
  devicePublicKey: string;
  capability: string;
};

function isEncodedBytes(value: unknown, length: number): value is string {
  if (typeof value !== "string" || !/^[A-Za-z0-9_-]+$/.test(value)) return false;
  try {
    const bytes = atob(value.replaceAll("-", "+").replaceAll("_", "/"));
    return (
      bytes.length === length &&
      btoa(bytes).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "") === value
    );
  } catch {
    return false;
  }
}

export async function requestPublicDeviceConnection(
  handle: string,
  browserPublicKey: string,
  signal?: AbortSignal,
): Promise<PublicDeviceConnection> {
  if (!isValidDeviceHandle(handle)) {
    throw new CloudRequestError("Invalid device handle.");
  }
  const response = await fetch(
    `${CLOUD_ORIGIN}/api/dashboard/devices/${encodeURIComponent(handle)}/connect`,
    {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ browser_public_key: browserPublicKey }),
      signal,
    },
  );
  if (!response.ok) throw new CloudRequestError("Cloud request failed.", response.status);
  let value: unknown;
  try {
    value = await response.json();
  } catch {
    throw new CloudRequestError("Cloud returned an invalid response.", response.status);
  }
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, [
      "device_handle",
      "device_id",
      "edge_api_origin",
      "device_public_key",
      "device_key_version",
      "capability",
    ]) ||
    !isEncodedBytes(value.device_public_key, 32) ||
    !isEncodedBytes(value.capability, 169) ||
    !Number.isSafeInteger(value.device_key_version) ||
    (value.device_key_version as number) <= 0 ||
    value.device_handle !== handle ||
    typeof value.device_id !== "string" ||
    !UUID.test(value.device_id)
  ) {
    throw new CloudRequestError("Cloud returned an invalid device target.");
  }
  return {
    target: {
      handle,
      deviceId: value.device_id,
      edgeApiOrigin: parseEdgeOrigin(value.edge_api_origin),
    },
    devicePublicKey: value.device_public_key,
    capability: value.capability,
  };
}

export function dashboardSignInUrl(returnPath: string): string {
  const destination = new URL(returnPath, PUBLIC_DASHBOARD_ORIGIN);
  if (destination.origin !== PUBLIC_DASHBOARD_ORIGIN) {
    throw new Error("Invalid dashboard return path.");
  }
  const login = new URL("/login", CLOUD_ORIGIN);
  login.searchParams.set("return_to", destination.href);
  return login.href;
}

export function dashboardBootstrapUrl(handle: string): string {
  if (!isValidDeviceHandle(handle)) throw new Error("Invalid device handle.");
  return `${PUBLIC_DASHBOARD_ORIGIN}/${encodeURIComponent(handle)}`;
}
