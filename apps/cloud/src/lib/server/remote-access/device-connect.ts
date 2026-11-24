import { and, eq } from "drizzle-orm";
import { SignJWT, base64url } from "jose";
import type { Database } from "../db";
import { devices, edges } from "../db/schema";
import { edgeApiOrigin } from "../edge-network";
import { isValidDeviceHandle } from "./device-handle";
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

export function capabilitySigningKey(signingKeyPkcs8: string) {
  return crypto.subtle.importKey(
    "pkcs8",
    buffer(base64url.decode(signingKeyPkcs8)),
    { name: "Ed25519" },
    false,
    ["sign"],
  );
}

export async function connectDashboardDevice(
  db: Database,
  userId: string,
  deviceHandle: string,
  browserPublicKey: string | null,
  signingKeyPkcs8: string,
  now = new Date(),
) {
  if (!isValidDeviceHandle(deviceHandle)) return null;
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
  const key = await capabilitySigningKey(signingKeyPkcs8);
  const lifetimeSeconds = 4 * 60 * 60;
  const expiresAt = issuedAt + lifetimeSeconds;
  const admissionToken = await new SignJWT({ device_id: device.id, user_id: userId })
    .setProtectedHeader({ alg: "EdDSA", typ: "JWT" })
    .setAudience(device.edgeId)
    .setIssuedAt(issuedAt)
    .setExpirationTime(expiresAt)
    .sign(key);
  const access = {
    user_id: userId,
    device_handle: device.handle,
    edge_api_origin: apiOrigin,
    device_id: device.id,
    admission_token: admissionToken,
    refresh_at: (issuedAt + lifetimeSeconds / 2) * 1000,
  };
  if (browserPublicKey === null) return access;
  let browserKey: Uint8Array;
  try {
    browserKey = base64url.decode(browserPublicKey);
  } catch {
    throw new InvalidBrowserKey();
  }
  if (browserKey.length !== 32 || base64url.encode(browserKey) !== browserPublicKey)
    throw new InvalidBrowserKey();
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
  const signature = new Uint8Array(await crypto.subtle.sign("Ed25519", key, buffer(signingInput)));
  const capability = new Uint8Array(169);
  capability.set(unsigned);
  capability.set(signature, unsigned.length);
  return {
    ...access,
    device_public_key: device.publicKey,
    device_key_version: device.keyVersion,
    capability: base64url.encode(capability),
  };
}

export class InvalidBrowserKey extends Error {}
