import { parseDeviceTarget, type PublicDevice, type PublicDeviceTarget } from "./remoteAccess";

const PREFIX = "pontia.remote-connection:";

function accessStorage<T>(action: (storage: Storage) => T, unavailable: T): T {
  try {
    return action(window.sessionStorage);
  } catch (error) {
    // Browser policy or storage quota can disable persistence; live access still works.
    if (error instanceof DOMException) return unavailable;
    throw error;
  }
}

type Device = Pick<PublicDevice, "handle" | "deviceId" | "edgeId">;

function key(userId: string, device: Device): string {
  return PREFIX + JSON.stringify([userId, device.deviceId, device.edgeId, device.handle]);
}

export function retainSavedConnections(userId: string | null, devices: Device[]): void {
  const retained = new Set(userId ? devices.map((device) => key(userId, device)) : []);
  accessStorage((storage) => {
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

  return {
    read() {
      const serialized = accessStorage((storage) => storage.getItem(name), null);
      if (serialized === null) return null;
      try {
        const value = JSON.parse(serialized);
        if (
          !value ||
          value.version !== 1 ||
          typeof value.token !== "string" ||
          !value.token ||
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
          session: value.session as string,
        };
      } catch {
        remove(serialized);
        return null;
      }
    },
    save(target: PublicDeviceTarget, token: string, session: string): string {
      const serialized = JSON.stringify({ version: 1, target, token, session });
      accessStorage((storage) => storage.setItem(name, serialized), undefined);
      return serialized;
    },
    remove,
  };
}
