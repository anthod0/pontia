import { and, eq } from "drizzle-orm";
import { base64url } from "jose";
import type { Database } from "../db";
import { devices, edges } from "../db/schema";
import { edgeApiOrigin } from "../edge-network";
import { isValidDeviceHandle } from "./device-handle";
import { issueEdgeTicket } from "../edge-tickets";
import { decodeDeviceTunnelPayload } from "./tickets";

const LABEL = new TextEncoder().encode("pontia-e2e-capability-v1\0");

function buffer(value: Uint8Array): ArrayBuffer {
  return value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength) as ArrayBuffer;
}

function uuidBytes(value: string): Uint8Array {
  const hex = value.replaceAll("-", "");
  if (!/^[0-9a-f]{32}$/.test(hex)) throw new Error("invalid device id");
  return Uint8Array.from(hex.match(/../g)!, (pair) => Number.parseInt(pair, 16));
}

function writeU64(output: Uint8Array, offset: number, value: number) {
  new DataView(output.buffer, output.byteOffset, output.byteLength).setBigUint64(
    offset,
    BigInt(value),
  );
}

export async function connectDashboardDevice(
  db: Database,
  userId: string,
  deviceHandle: string,
  browserPublicKey: string,
  signingKeyPkcs8: string,
  now = new Date(),
) {
  if (!isValidDeviceHandle(deviceHandle)) return null;
  let browserKey: Uint8Array;
  try {
    browserKey = base64url.decode(browserPublicKey);
  } catch {
    throw new InvalidBrowserKey();
  }
  if (browserKey.length !== 32 || base64url.encode(browserKey) !== browserPublicKey)
    throw new InvalidBrowserKey();
  const device = await db
    .select({
      id: devices.id,
      edgeId: edges.id,
      handle: devices.handle,
      tunnelUrl: edges.tunnelUrl,
      publicKey: devices.e2ePublicKey,
      keyVersion: devices.e2eKeyVersion,
    })
    .from(devices)
    .innerJoin(edges, eq(devices.edgeId, edges.id))
    .where(and(eq(devices.userId, userId), eq(devices.handle, deviceHandle)))
    .get();
  if (!device?.publicKey || !device.keyVersion) return null;
  const apiOrigin = edgeApiOrigin(device.tunnelUrl);
  if (!apiOrigin) return null;
  const issuedAt = Math.floor(now.getTime() / 1000);
  const authorizationId = crypto.getRandomValues(new Uint8Array(32));
  const unsigned = new Uint8Array(105);
  unsigned[0] = 1;
  unsigned.set(uuidBytes(device.id), 1);
  writeU64(unsigned, 17, device.keyVersion);
  unsigned.set(browserKey, 25);
  unsigned.set(authorizationId, 57);
  writeU64(unsigned, 89, issuedAt);
  writeU64(unsigned, 97, issuedAt + 60);
  const signingInput = new Uint8Array(LABEL.length + unsigned.length);
  signingInput.set(LABEL);
  signingInput.set(unsigned, LABEL.length);
  const key = await crypto.subtle.importKey(
    "pkcs8",
    buffer(base64url.decode(signingKeyPkcs8)),
    { name: "Ed25519" },
    false,
    ["sign"],
  );
  const signature = new Uint8Array(await crypto.subtle.sign("Ed25519", key, buffer(signingInput)));
  const capability = new Uint8Array(169);
  capability.set(unsigned);
  capability.set(signature, unsigned.length);
  const admissionTicket = await issueEdgeTicket(db, {
    purpose: "browser_admission",
    userId,
    expectedEdgeId: device.edgeId,
    payload: { device_id: device.id },
    expiresAt: new Date(now.getTime() + 60_000),
    createdAt: now,
    decodePayload: decodeDeviceTunnelPayload,
  });
  return {
    user_id: userId,
    device_handle: device.handle,
    edge_api_origin: apiOrigin,
    device_id: device.id,
    device_public_key: device.publicKey,
    device_key_version: device.keyVersion,
    capability: base64url.encode(capability),
    admission_ticket: admissionTicket,
  };
}

export class InvalidBrowserKey extends Error {}
