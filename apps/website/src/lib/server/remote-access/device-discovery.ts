import { and, asc, eq } from "drizzle-orm";
import type { Database } from "../db";
import { devices, edges } from "../db/schema";
import { edgeApiOrigin } from "../edge-network";
import { isValidDeviceHandle } from "./device-handle";

export type DashboardDevice = {
  deviceHandle: string;
  name: string | null;
};

export type DashboardDeviceTarget = {
  deviceHandle: string;
  deviceId: string;
  edgeId: string;
  edgeApiOrigin: string;
};

export async function listDashboardDevices(
  db: Database,
  userId: string,
): Promise<DashboardDevice[]> {
  return db
    .select({ deviceHandle: devices.handle, name: devices.name })
    .from(devices)
    .where(eq(devices.userId, userId))
    .orderBy(asc(devices.handle));
}

export async function findDashboardDeviceTarget(
  db: Database,
  userId: string,
  deviceHandle: string,
): Promise<DashboardDeviceTarget | null> {
  if (!isValidDeviceHandle(deviceHandle)) return null;
  const target = await db
    .select({
      deviceHandle: devices.handle,
      deviceId: devices.id,
      edgeId: devices.edgeId,
      tunnelUrl: edges.tunnelUrl,
    })
    .from(devices)
    .innerJoin(edges, eq(devices.edgeId, edges.id))
    .where(and(eq(devices.userId, userId), eq(devices.handle, deviceHandle)))
    .get();
  if (!target) return null;
  const apiOrigin = edgeApiOrigin(target.tunnelUrl);
  if (!apiOrigin) return null;
  return {
    deviceHandle: target.deviceHandle,
    deviceId: target.deviceId,
    edgeId: target.edgeId,
    edgeApiOrigin: apiOrigin,
  };
}
