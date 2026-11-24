import { parseDeviceTarget, type PublicDevice, type PublicDeviceTarget } from "./remoteAccess";
import { isValidDeviceHandle } from "../../../lib/deviceHandle";

const PREFIX = "pontia.remote-connection:";
const DEVICE_LIST_KEY = "pontia.remote-device-list";
type DeviceList = { userId: string; devices: PublicDevice[] };

function accessStorage<T>(action: (storage: Storage) => T, unavailable: T): T {
  try {
    return action(window.localStorage);
  } catch (error) {
    // Browser policy or storage quota can disable persistence; live access still works.
    if (error instanceof DOMException) return unavailable;
    throw error;
  }
}

type Device = Pick<PublicDevice, "handle" | "deviceId" | "edgeId">;

export const deviceListCache = {
  read(): DeviceList | null {
    const serialized = accessStorage((storage) => storage.getItem(DEVICE_LIST_KEY), null);
    if (serialized === null) return null;
    try {
      const value = JSON.parse(serialized);
      if (
        value?.version !== 1 ||
        typeof value.userId !== "string" ||
        !value.userId ||
        !Array.isArray(value.devices) ||
        !value.devices.every(
          (device: PublicDevice) =>
            device &&
            typeof device.handle === "string" &&
            isValidDeviceHandle(device.handle) &&
            typeof device.deviceId === "string" &&
            device.deviceId &&
            typeof device.edgeId === "string" &&
            device.edgeId &&
            typeof device.name === "string" &&
            device.name,
        ) ||
        new Set(value.devices.map((device: PublicDevice) => device.handle)).size !==
          value.devices.length
      )
        throw new Error("Invalid saved device list");
      return { userId: value.userId, devices: value.devices };
    } catch {
      accessStorage((storage) => storage.removeItem(DEVICE_LIST_KEY), undefined);
      return null;
    }
  },
  save(listing: DeviceList): void {
    accessStorage(
      (storage) => storage.setItem(DEVICE_LIST_KEY, JSON.stringify({ version: 1, ...listing })),
      undefined,
    );
  },
};

function key(userId: string, device: Device): string {
  return PREFIX + JSON.stringify([userId, device.deviceId, device.edgeId, device.handle]);
}

export function retainSavedConnections(userId: string | null, devices: Device[]): void {
  const retained = new Set(userId ? devices.map((device) => key(userId, device)) : []);
  accessStorage((storage) => {
    if (userId === null) storage.removeItem(DEVICE_LIST_KEY);
    for (let index = storage.length - 1; index >= 0; index--) {
      const name = storage.key(index)!;
      if (name.startsWith(PREFIX) && !retained.has(name)) storage.removeItem(name);
    }
  }, undefined);
}

export function connectionCache(userId: string, device: Device) {
  const name = key(userId, device);

  function remove(expected: string): void {
    accessStorage((storage) => {
      if (storage.getItem(name) === expected) storage.removeItem(name);
    }, undefined);
  }

  function read() {
    const serialized = accessStorage((storage) => storage.getItem(name), null);
    if (serialized === null) return null;
    try {
      const value = JSON.parse(serialized);
      if (
        !value ||
        value.version !== 2 ||
        typeof value.token !== "string" ||
        !value.token ||
        !Number.isSafeInteger(value.refreshAt) ||
        typeof value.session !== "string" ||
        !/^[A-Za-z0-9_-]+$/.test(value.session)
      )
        throw new Error("Invalid saved connection");
      const target = parseDeviceTarget(value.target);
      if (target.handle !== device.handle || target.deviceId !== device.deviceId)
        throw new Error("Saved device does not match");
      return {
        serialized,
        target,
        token: value.token as string,
        refreshAt: value.refreshAt as number,
        session: value.session as string,
      };
    } catch {
      remove(serialized);
      return null;
    }
  }

  function save(
    target: PublicDeviceTarget,
    token: string,
    session: string,
    refreshAt: number,
  ): string {
    const serialized = JSON.stringify({ version: 2, target, token, session, refreshAt });
    accessStorage((storage) => {
      storage.setItem(name, serialized);
    }, undefined);
    return serialized;
  }

  return {
    read,
    save,
    remove,
    refresh(target: PublicDeviceTarget, token: string, session: string, refreshAt: number) {
      const current = read();
      if (
        !current ||
        current.session !== session ||
        current.target.edgeApiOrigin !== target.edgeApiOrigin
      )
        return null;
      if (current.refreshAt > refreshAt) return current.serialized;
      return save(target, token, session, refreshAt);
    },
  };
}
