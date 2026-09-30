import { and, eq, exists, sql } from "drizzle-orm";
import { consumeEdgeTicket, issueEdgeTicket, type EdgeTicketDependencies } from "../edge-tickets";
import type { Database } from "../db";
import { devices } from "../db/schema";
import { isUuidV7 } from "../uuid";
import { findDashboardDeviceTarget } from "./device-discovery";

const TICKET_LIFETIME_MS = 60_000;
const CAPABILITY_LIFETIME_MS = 30 * 24 * 60 * 60 * 1_000;

export type DashboardAccessDependencies = EdgeTicketDependencies;

type DashboardAccessPayload = {
  device_id: string;
  expires_at: string;
};

export type IssuedDashboardAccess = {
  ticket: string;
  bootstrapUrl: string;
};

export type IssueDashboardAccessResult =
  | { status: "issued"; value: IssuedDashboardAccess }
  | { status: "device_not_found" };

export type RedeemedDashboardAccess = {
  deviceId: string;
  deviceHandle: string;
  expiresAt: string;
};

export function decodeDashboardAccessPayload(value: unknown): DashboardAccessPayload | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record).sort();
  if (keys.length !== 2 || keys[0] !== "device_id" || keys[1] !== "expires_at") return null;
  const deviceId = record.device_id;
  const expiresAt = record.expires_at;
  if (
    typeof deviceId !== "string" ||
    deviceId !== deviceId.toLowerCase() ||
    !isUuidV7(deviceId) ||
    typeof expiresAt !== "string"
  ) {
    return null;
  }
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(expiresAt)) return null;
  const parsedExpiry = new Date(expiresAt);
  if (!Number.isFinite(parsedExpiry.getTime()) || parsedExpiry.toISOString() !== expiresAt) {
    return null;
  }
  return { device_id: deviceId, expires_at: expiresAt };
}

export async function issueDashboardAccess(
  db: Database,
  userId: string,
  deviceHandle: string,
  dependencies?: DashboardAccessDependencies,
): Promise<IssueDashboardAccessResult> {
  const target = await findDashboardDeviceTarget(db, userId, deviceHandle);
  if (!target) return { status: "device_not_found" };

  const now = dependencies?.now() ?? new Date();
  const capabilityExpiresAt = new Date(now.getTime() + CAPABILITY_LIFETIME_MS);
  const ticket = await issueEdgeTicket(
    db,
    {
      purpose: "dashboard_access",
      userId,
      expectedEdgeId: target.edgeId,
      payload: {
        device_id: target.deviceId,
        expires_at: capabilityExpiresAt.toISOString(),
      },
      expiresAt: new Date(now.getTime() + TICKET_LIFETIME_MS),
      createdAt: now,
      decodePayload: decodeDashboardAccessPayload,
    },
    dependencies,
  );

  return {
    status: "issued",
    value: {
      ticket,
      bootstrapUrl: `${target.edgeApiOrigin}/dashboard/bootstrap`,
    },
  };
}

export async function redeemDashboardAccess(
  db: Database,
  edgeId: string,
  ticket: string,
): Promise<RedeemedDashboardAccess | null> {
  const consumed = await consumeEdgeTicket(db, {
    ticket,
    purpose: "dashboard_access",
    expectedEdgeId: edgeId,
    decodePayload: decodeDashboardAccessPayload,
    additionalCondition: ({ userId, expectedEdgeId, payload, createdAt }) => {
      const issuedAt = new Date(createdAt);
      if (
        !Number.isFinite(issuedAt.getTime()) ||
        issuedAt.toISOString() !== createdAt ||
        new Date(issuedAt.getTime() + CAPABILITY_LIFETIME_MS).toISOString() !== payload.expires_at
      ) {
        return sql`0`;
      }
      return exists(
        db
          .select({ value: devices.id })
          .from(devices)
          .where(
            and(
              eq(devices.id, payload.device_id),
              eq(devices.userId, userId),
              eq(devices.edgeId, expectedEdgeId),
            ),
          ),
      );
    },
  });
  if (!consumed) return null;

  const device = await db
    .select({ deviceHandle: devices.handle })
    .from(devices)
    .where(
      and(
        eq(devices.id, consumed.payload.device_id),
        eq(devices.userId, consumed.userId),
        eq(devices.edgeId, consumed.expectedEdgeId),
      ),
    )
    .get();
  if (!device) return null;
  return {
    deviceId: consumed.payload.device_id,
    deviceHandle: device.deviceHandle,
    expiresAt: consumed.payload.expires_at,
  };
}
