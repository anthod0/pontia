import { asc, eq } from "drizzle-orm";
import type { Database } from "../db";
import { devices } from "../db/schema";

export type DashboardDevice = {
  deviceHandle: string;
  name: string;
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
