import { and, asc, eq, sql } from "drizzle-orm";
import type { Database } from "../db";
import { devices, edges } from "../db/schema";
import { isUuidV7 } from "../uuid";
import { deviceHandleCandidates } from "./device-handle";
import { edgeIsAccessibleTo } from "./resources";

export type RegistrationEdge = {
  id: string;
  name: string;
};

export type RegisteredDevice = {
  id: string;
  handle: string;
  name: string;
  edgeId: string;
  edgeName: string;
  e2ePublicKey: string;
  e2eKeyVersion: number;
};

export type DeviceLookup =
  | { status: "found"; device: RegisteredDevice }
  | { status: "not_found" }
  | { status: "conflict" };

export type DeviceRegistration =
  | { status: "created" | "existing"; device: RegisteredDevice }
  | { status: "invalid_request" | "edge_not_found" | "conflict" };

export type DeviceUnregistration = { status: "deleted" | "not_found" | "conflict" };

function validName(value: string) {
  return value.trim() === value && value.length > 0 && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
}

export async function registrationEdges(db: Database, userId: string): Promise<RegistrationEdge[]> {
  return db
    .select({ id: edges.id, name: edges.name })
    .from(edges)
    .where(edgeIsAccessibleTo(userId))
    .orderBy(asc(edges.name), asc(edges.id));
}

type StoredDevice = Omit<RegisteredDevice, "e2ePublicKey" | "e2eKeyVersion"> & {
  userId: string;
  e2ePublicKey: string | null;
  e2eKeyVersion: number | null;
};

function registeredDevice(device: StoredDevice): RegisteredDevice {
  return {
    id: device.id,
    handle: device.handle,
    name: device.name,
    edgeId: device.edgeId,
    edgeName: device.edgeName,
    e2ePublicKey: device.e2ePublicKey ?? "",
    e2eKeyVersion: device.e2eKeyVersion ?? 0,
  };
}

async function storedDevice(db: Database, deviceId: string): Promise<StoredDevice | null> {
  const device = await db
    .select({
      id: devices.id,
      handle: devices.handle,
      name: devices.name,
      userId: devices.userId,
      edgeId: devices.edgeId,
      edgeName: edges.name,
      e2ePublicKey: devices.e2ePublicKey,
      e2eKeyVersion: devices.e2eKeyVersion,
    })
    .from(devices)
    .innerJoin(edges, eq(devices.edgeId, edges.id))
    .where(eq(devices.id, deviceId))
    .get();
  return device ?? null;
}

export async function findRegisteredDevice(
  db: Database,
  userId: string,
  deviceId: string,
): Promise<DeviceLookup> {
  if (!isUuidV7(deviceId)) return { status: "not_found" };
  const device = await storedDevice(db, deviceId);
  if (!device) return { status: "not_found" };
  if (device.userId !== userId) return { status: "conflict" };
  return { status: "found", device: registeredDevice(device) };
}

export async function unregisterDevice(
  db: Database,
  userId: string,
  deviceId: string,
): Promise<DeviceUnregistration> {
  if (!isUuidV7(deviceId)) return { status: "not_found" };
  const deleted = await db
    .delete(devices)
    .where(and(eq(devices.id, deviceId), eq(devices.userId, userId)))
    .returning({ id: devices.id });
  if (deleted.length === 1) return { status: "deleted" };
  return (await storedDevice(db, deviceId)) ? { status: "conflict" } : { status: "not_found" };
}

export async function registerDevice(
  db: Database,
  userId: string,
  deviceId: string,
  name: string,
  edgeId: string,
  e2ePublicKey: string,
  e2eKeyVersion: number,
): Promise<DeviceRegistration> {
  if (
    !isUuidV7(deviceId) ||
    !isUuidV7(edgeId) ||
    !validName(name) ||
    !/^[A-Za-z0-9_-]{43}$/.test(e2ePublicKey) ||
    !Number.isSafeInteger(e2eKeyVersion) ||
    e2eKeyVersion < 1
  )
    return { status: "invalid_request" };
  const existing = await storedDevice(db, deviceId);
  if (existing) {
    if (existing.userId !== userId || existing.edgeId !== edgeId) return { status: "conflict" };
    if (e2eKeyVersion <= (existing.e2eKeyVersion ?? 0)) {
      if (e2eKeyVersion !== existing.e2eKeyVersion || e2ePublicKey !== existing.e2ePublicKey)
        return { status: "conflict" };
      return { status: "existing", device: registeredDevice(existing) };
    }
    const [updated] = await db
      .update(devices)
      .set({ e2ePublicKey, e2eKeyVersion, updatedAt: new Date().toISOString() })
      .where(and(eq(devices.id, deviceId), eq(devices.userId, userId)))
      .returning({ id: devices.id });
    if (!updated) return { status: "conflict" };
    const stored = await storedDevice(db, deviceId);
    return stored
      ? { status: "existing", device: registeredDevice(stored) }
      : { status: "conflict" };
  }

  const now = new Date().toISOString();
  for (const handle of deviceHandleCandidates(name, deviceId)) {
    const created = await db
      .insert(devices)
      .select(
        db
          .select({
            id: sql<string>`${deviceId}`.as("id"),
            userId: sql<string>`${userId}`.as("user_id"),
            edgeId: edges.id,
            handle: sql<string>`${handle}`.as("handle"),
            name: sql<string>`${name}`.as("name"),
            e2ePublicKey: sql<string>`${e2ePublicKey}`.as("e2e_public_key"),
            e2eKeyVersion: sql<number>`${e2eKeyVersion}`.as("e2e_key_version"),
            createdAt: sql<string>`${now}`.as("created_at"),
            updatedAt: sql<string>`${now}`.as("updated_at"),
          })
          .from(edges)
          .where(and(eq(edges.id, edgeId), edgeIsAccessibleTo(userId))),
      )
      .onConflictDoNothing()
      .returning({ id: devices.id });
    const stored = await storedDevice(db, deviceId);
    if (stored) {
      if (stored.userId !== userId || stored.edgeId !== edgeId) return { status: "conflict" };
      return {
        status: created.length === 1 ? "created" : "existing",
        device: registeredDevice(stored),
      };
    }
  }
  return { status: "edge_not_found" };
}
