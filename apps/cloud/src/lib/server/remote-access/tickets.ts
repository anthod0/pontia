import { and, eq, exists } from "drizzle-orm";
import { consumeEdgeTicket, issueEdgeTicket, type EdgeTicketDependencies } from "../edge-tickets";
import type { Database } from "../db";
import { devices } from "../db/schema";
import { isUuidV7 } from "../uuid";
import { findOwnedDeviceTarget } from "./resources";

const TICKET_LIFETIME_MS = 60_000;

export type IssuedTunnelTicket = {
  ticket: string;
  tunnelUrl: string;
  expiresAt: string;
};

export type IssueTunnelTicketResult =
  | { status: "issued"; value: IssuedTunnelTicket }
  | { status: "device_not_found" };

export type TunnelTicketDependencies = EdgeTicketDependencies;

type DeviceTunnelPayload = { device_id: string };

export function decodeDeviceTunnelPayload(value: unknown): DeviceTunnelPayload | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const keys = Object.keys(value);
  if (keys.length !== 1 || keys[0] !== "device_id") return null;
  const deviceId = (value as Record<string, unknown>).device_id;
  if (typeof deviceId !== "string" || deviceId !== deviceId.toLowerCase() || !isUuidV7(deviceId)) {
    return null;
  }
  return { device_id: deviceId };
}

export async function issueTunnelTicket(
  db: Database,
  userId: string,
  deviceId: string,
  dependencies?: TunnelTicketDependencies,
): Promise<IssueTunnelTicketResult> {
  const target = await findOwnedDeviceTarget(db, userId, deviceId);
  if (!target) return { status: "device_not_found" };

  const now = dependencies?.now() ?? new Date();
  const expiresAt = new Date(now.getTime() + TICKET_LIFETIME_MS);
  const ticket = await issueEdgeTicket(
    db,
    {
      purpose: "device_tunnel",
      userId,
      expectedEdgeId: target.edgeId,
      payload: { device_id: target.deviceId },
      expiresAt,
      decodePayload: decodeDeviceTunnelPayload,
    },
    dependencies,
  );

  return {
    status: "issued",
    value: {
      ticket,
      tunnelUrl: target.tunnelUrl,
      expiresAt: expiresAt.toISOString(),
    },
  };
}

export async function redeemTunnelTicket(
  db: Database,
  edgeId: string,
  ticket: string,
): Promise<{ deviceId: string } | null> {
  const consumed = await consumeEdgeTicket(db, {
    ticket,
    purpose: "device_tunnel",
    expectedEdgeId: edgeId,
    decodePayload: decodeDeviceTunnelPayload,
    additionalCondition: ({ userId, expectedEdgeId, payload }) =>
      exists(
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
      ),
  });
  return consumed ? { deviceId: consumed.payload.device_id } : null;
}
