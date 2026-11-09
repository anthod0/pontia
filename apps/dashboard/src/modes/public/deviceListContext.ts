import { getContext, setContext } from "svelte";
import type { ConnectedDevice } from "./deviceConnections";

export type DeviceList = {
  devices: () => ConnectedDevice[];
  retry: (handle: string) => void;
};

const deviceListContext = Symbol("public-device-list");

export function provideDeviceList(list: DeviceList): void {
  setContext(deviceListContext, list);
}

export function useDeviceList(): DeviceList {
  const list = getContext<DeviceList>(deviceListContext);
  if (!list) throw new Error("This component requires the Public device list.");
  return list;
}
