import { expect, test } from "bun:test";
import { base64url } from "jose";
import { devices, edges, users } from "../src/lib/server/db/schema";
import { issueE2eCapability } from "../src/lib/server/remote-access/e2e-capability";
import { verifyDeviceKeyProof } from "../src/lib/server/remote-access/device-key-proof";
import { testDatabase } from "./database";

const database = testDatabase();
const deviceId = "0195e7c1-1b22-7c33-9d44-123456789abc";
const handle = "office-device";

function bytes(value: ArrayBuffer): Uint8Array {
  return new Uint8Array(value);
}

async function capabilityKeys() {
  const pair = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
  return {
    privateKey: base64url.encode(bytes(await crypto.subtle.exportKey("pkcs8", pair.privateKey))),
    publicKey: pair.publicKey,
  };
}

async function seedDevice(publicKey: string) {
  await database.db.insert(users).values({ id: "user-owner" });
  await database.db.insert(edges).values({
    id: "0195e7b9-91c2-73d4-a560-2f78b90c1234",
    userId: "user-owner",
    name: "Tokyo",
    tunnelUrl: "wss://edge.example/tunnel",
    serviceCredentialHash: "hash",
  });
  await database.db.insert(devices).values({
    id: deviceId,
    userId: "user-owner",
    edgeId: "0195e7b9-91c2-73d4-a560-2f78b90c1234",
    handle,
    name: "Office",
    e2ePublicKey: publicKey,
    e2eKeyVersion: 3,
  });
}

test("capability is signed for the owned device and browser key with a 60 second window", async () => {
  const keys = await capabilityKeys();
  const devicePublic = base64url.encode(new Uint8Array(32).fill(7));
  const browserPublic = base64url.encode(new Uint8Array(32).fill(9));
  await seedDevice(devicePublic);

  const result = await issueE2eCapability(
    database.db,
    "user-owner",
    handle,
    browserPublic,
    keys.privateKey,
    new Date("2026-01-01T00:00:00.000Z"),
  );
  expect(result).not.toBeNull();
  const capability = base64url.decode(result!.capability);
  expect(capability).toHaveLength(169);
  expect(Array.from(capability.slice(25, 57))).toEqual(Array.from(base64url.decode(browserPublic)));
  const view = new DataView(capability.buffer, capability.byteOffset, capability.byteLength);
  expect(Number(view.getBigUint64(97)) - Number(view.getBigUint64(89))).toBe(60);
  const label = new TextEncoder().encode("pontia-e2e-capability-v1\0");
  const signed = new Uint8Array(label.length + 105);
  signed.set(label);
  signed.set(capability.slice(0, 105), label.length);
  expect(await crypto.subtle.verify("Ed25519", keys.publicKey, capability.slice(105), signed)).toBe(
    true,
  );
  expect(
    await issueE2eCapability(database.db, "user-other", handle, browserPublic, keys.privateKey),
  ).toBeNull();
});

test("registration proof requires possession of the submitted X25519 private key", async () => {
  const cloud = await crypto.subtle.generateKey("X25519", true, ["deriveBits"]);
  const device = await crypto.subtle.generateKey("X25519", true, ["deriveBits"]);
  const devicePublic = bytes(await crypto.subtle.exportKey("raw", device.publicKey));
  const cloudPublic = await crypto.subtle.importKey(
    "raw",
    await crypto.subtle.exportKey("raw", cloud.publicKey),
    "X25519",
    false,
    [],
  );
  const shared = await crypto.subtle.deriveBits(
    { name: "X25519", public: cloudPublic },
    device.privateKey,
    256,
  );
  const hkdf = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveKey"]);
  const key = await crypto.subtle.deriveKey(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt: new ArrayBuffer(0),
      info: new TextEncoder().encode("pontia-device-key-registration-v1\0"),
    },
    hkdf,
    { name: "HMAC", hash: "SHA-256", length: 256 },
    false,
    ["sign"],
  );
  const id = new TextEncoder().encode(deviceId);
  const message = new Uint8Array(id.length + 1 + 8 + 32);
  message.set(id);
  new DataView(message.buffer).setBigUint64(id.length + 1, 4n);
  message.set(devicePublic, id.length + 9);
  const proof = bytes(await crypto.subtle.sign("HMAC", key, message));
  const cloudPrivate = base64url.encode(
    bytes(await crypto.subtle.exportKey("pkcs8", cloud.privateKey)),
  );

  expect(
    await verifyDeviceKeyProof(
      deviceId,
      4,
      base64url.encode(devicePublic),
      base64url.encode(proof),
      cloudPrivate,
    ),
  ).toBe(true);
  expect(
    await verifyDeviceKeyProof(
      deviceId,
      5,
      base64url.encode(devicePublic),
      base64url.encode(proof),
      cloudPrivate,
    ),
  ).toBe(false);
});
