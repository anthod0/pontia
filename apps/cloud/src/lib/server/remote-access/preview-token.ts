import { and, eq } from "drizzle-orm";
import { SignJWT } from "jose";
import { edgeAuthority } from "../../edge-port";
import type { Database } from "../db";
import { devices, edges } from "../db/schema";
import { edgeIsAccessibleTo } from "./resources";
import { isValidDeviceHandle } from "./device-handle";
import { capabilitySigningKey } from "../edge-auth";

// Fetch Standard bad ports, also enforced at the device's loopback dialer.
const blockedPorts = new Set([
  0, 1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
  103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
  512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
  995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668,
  6669, 6679, 6697, 10080,
]);

export function validPreviewPort(port: unknown): port is number {
  return (
    typeof port === "number" &&
    Number.isInteger(port) &&
    port >= 1 &&
    port <= 65535 &&
    !blockedPorts.has(port)
  );
}

export async function issuePreviewToken(
  db: Database,
  userId: string,
  handle: string,
  port: number,
  env: App.Platform["env"],
) {
  const device =
    isValidDeviceHandle(handle) &&
    (await db
      .select({
        id: devices.id,
        edge: edges.id,
        label: edges.dnsLabel,
        tunnel: edges.tunnelUrl,
        enabled: edges.previewEnabled,
      })
      .from(devices)
      .innerJoin(edges, eq(devices.edgeId, edges.id))
      .where(
        and(eq(devices.userId, userId), eq(devices.handle, handle), edgeIsAccessibleTo(userId)),
      )
      .get());
  if (!device) return { status: 404, body: { error: "device_not_found" } };
  if (!device.enabled || !device.label || !env.PREVIEW_ZONE)
    return { status: 409, body: { error: "edge_preview_unavailable" } };
  const hostname = `${port}-${device.id.replaceAll("-", "")}.${device.label}.${env.PREVIEW_ZONE}`;
  const now = Math.floor(Date.now() / 1000);
  const expires = now + 4 * 3600;
  const token = await new SignJWT({ purpose: "localhost_preview", did: device.id, port })
    .setProtectedHeader({ alg: "EdDSA", typ: "pontia-preview+jwt" })
    .setAudience(device.edge)
    .setSubject(userId)
    .setIssuedAt(now)
    .setExpirationTime(expires)
    .sign(await capabilitySigningKey(env.E2E_CAPABILITY_SIGNING_KEY));
  return {
    status: 200,
    body: {
      preview_origin: `https://${edgeAuthority(hostname, Number(new URL(device.tunnel).port || 443))}`,
      token,
      expires_at: expires,
    },
  };
}
