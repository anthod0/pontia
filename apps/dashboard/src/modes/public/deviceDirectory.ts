import { getContext, setContext } from "svelte";
import type { ConnectedDevice } from "./deviceConnections";

const directoryContext = Symbol("public-device-directory");
type DeviceDirectory = {
  devices: ConnectedDevice[];
  handle: string | undefined;
  rememberDevice: (handle: string) => void;
};

export function provideDeviceDirectory(directory: DeviceDirectory): void {
  setContext(directoryContext, directory);
}

export function useDeviceDirectory(): DeviceDirectory {
  return getContext<DeviceDirectory>(directoryContext);
}

export const deviceStateLabel = {
  connecting: "Connecting",
  online: "Online",
  offline: "Offline",
  "sign-in-required": "Sign in required",
};
