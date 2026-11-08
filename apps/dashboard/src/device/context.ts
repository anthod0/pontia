import { getContext, setContext } from "svelte";
import type { DeviceRuntime } from "./runtime";

export const deviceContext = Symbol("dashboard-device");

export function provideDeviceRuntime(device: DeviceRuntime): void {
  setContext(deviceContext, device);
}

export function useDeviceRuntime(): DeviceRuntime {
  const device = getContext<DeviceRuntime>(deviceContext);
  if (!device) throw new Error("This component requires a Dashboard device.");
  return device;
}
