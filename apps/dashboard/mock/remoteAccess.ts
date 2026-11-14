// Development-only stand-in for src/modes/public/remote-access. The mock Vite plugin
// resolves the remote-access entry point to this module, so it must keep the same exports.
import { isValidDeviceHandle } from "../src/lib/deviceHandle.ts";
import type {
  CloudProfile,
  PublicDevice,
  PublicDeviceConnection,
  PublicDeviceTarget,
} from "../src/modes/public/remote-access/index.ts";
import { mockCloudDevices, mockDeviceHeader } from "./devices.ts";

export type { CloudProfile, PublicDevice, PublicDeviceConnection, PublicDeviceTarget };

export class CloudRequestError extends Error {
  readonly status?: number;

  constructor(message: string, status?: number) {
    super(message);
    this.name = "CloudRequestError";
    this.status = status;
  }
}

const CONNECT_DELAY_MS = 600;
const scenarioKey = "pontia.mockCloudScenario";
const scenarios = ["default", "signed-out", "no-devices", "cloud-error"] as const;
type Scenario = (typeof scenarios)[number];

// `?mock-cloud=<scenario>` switches the scenario for the rest of the browser tab.
function readScenario(): Scenario {
  const requested = new URL(location.href).searchParams.get("mock-cloud");
  if (scenarios.includes(requested as Scenario)) sessionStorage.setItem(scenarioKey, requested!);
  const stored = sessionStorage.getItem(scenarioKey);
  return scenarios.includes(stored as Scenario) ? (stored as Scenario) : "default";
}

function wait(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        reject(signal.reason);
      },
      { once: true },
    );
  });
}

export async function listPublicDevices(
  signal?: AbortSignal,
): Promise<{ userId: string; devices: PublicDevice[] }> {
  await wait(300, signal);
  const scenario = readScenario();
  if (scenario === "signed-out") throw new CloudRequestError("Cloud request failed.", 401);
  if (scenario === "cloud-error") throw new CloudRequestError("Cloud request failed.", 503);
  return {
    userId: "mock-user",
    devices:
      scenario === "no-devices"
        ? []
        : mockCloudDevices.map(({ handle, name }) => ({
            handle,
            deviceId: handle,
            edgeId: "mock-edge",
            name,
          })),
  };
}

export async function getCloudProfile(signal?: AbortSignal): Promise<CloudProfile> {
  await wait(200, signal);
  if (readScenario() === "signed-out") throw new CloudRequestError("Cloud request failed.", 401);
  return { displayName: "Ada Lovelace", avatarUrl: null };
}

export function cloudAccountUrl(): string {
  return "https://pontia.dev/settings/account";
}

export function cloudDevicesUrl(): string {
  return "https://pontia.dev/settings/devices";
}

export async function requestPublicDeviceConnection(): Promise<PublicDeviceConnection> {
  throw new CloudRequestError("The mock cloud does not issue device connections.");
}

export function createRemoteConnection(
  { handle }: Pick<PublicDevice, "handle" | "deviceId" | "edgeId">,
  _userId: string,
  _onConnectionFailure?: (error: unknown) => void,
) {
  const lifetime = new AbortController();
  const connection = mockCloudDevices.find((device) => device.handle === handle)?.connection;

  async function connect(signal?: AbortSignal): Promise<void> {
    const scoped = signal ? AbortSignal.any([lifetime.signal, signal]) : lifetime.signal;
    if (connection === "connecting")
      return new Promise((_, reject) => {
        scoped.addEventListener("abort", () => reject(scoped.reason), { once: true });
      });
    await wait(CONNECT_DELAY_MS, scoped);
    if (connection !== "online") throw new CloudRequestError("Device is offline.", 503);
  }

  function e2eFetch(path: string, init: RequestInit = {}): Promise<Response> {
    const headers = new Headers(init.headers);
    headers.set(mockDeviceHeader, handle);
    const signal = init.signal ? AbortSignal.any([lifetime.signal, init.signal]) : lifetime.signal;
    return fetch(path, { ...init, headers, signal });
  }

  return {
    connect,
    e2eFetch,
    uploadStorageScope: async () => JSON.stringify([location.origin, "mock", handle]),
    signal: lifetime.signal,
    dispose: () => lifetime.abort(),
  };
}

export function dashboardSignInUrl(returnPath: string): string {
  const destination = new URL(returnPath, location.origin);
  destination.searchParams.set("mock-cloud", "default");
  return `${destination.pathname}${destination.search}${destination.hash}`;
}

export function dashboardBootstrapUrl(handle: string): string {
  if (!isValidDeviceHandle(handle)) throw new Error("Invalid device handle.");
  return `${location.origin}/${encodeURIComponent(handle)}`;
}

export function retainSavedConnections(_userId: string | null, _devices: PublicDevice[]): void {}
