import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { base64url } from "jose";
import { GET as listEdges } from "../src/routes/api/remote/edges/+server";
import {
  DELETE as deleteDevice,
  GET as getDevice,
  PUT as putDevice,
} from "../src/routes/api/remote/devices/[device_id]/+server";
import { sha256Base64url } from "../src/lib/server/crypto";
import { authSessions, edges, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const callListEdges = listEdges as unknown as (event: RequestEvent) => Promise<Response>;
const callDeleteDevice = deleteDevice as unknown as (event: RequestEvent) => Promise<Response>;
const callGetDevice = getDevice as unknown as (event: RequestEvent) => Promise<Response>;
const callPutDevice = putDevice as unknown as (event: RequestEvent) => Promise<Response>;

const database = testDatabase();
const sessionId = "0195e7d1-1b22-7c33-9d44-123456789abc";
const edgeId = "0195e7d2-1b22-7c33-9d44-123456789abc";
const deviceId = "0195e7d3-1b22-7c33-9d44-123456789abc";
const secret = base64url.encode(new Uint8Array(32).fill(9));
const credential = `ptr_v1_${sessionId}_${secret}`;
let proofPrivateKey = "";
const capabilityVerificationKey = base64url.encode(new Uint8Array(32).fill(3));

function event(path: string, options?: { method?: string; token?: string; body?: unknown }) {
  const url = new URL(path, "https://example.com");
  return {
    url,
    params: { device_id: url.pathname.split("/").at(-1) },
    request: new Request(url, {
      method: options?.method ?? "GET",
      headers: options?.token
        ? {
            Authorization: `Bearer ${options.token}`,
            "Content-Type": "application/json",
          }
        : undefined,
      body: options?.body === undefined ? undefined : JSON.stringify(options.body),
    }),
    platform: {
      env: {
        DB: database.binding,
        E2E_REGISTRATION_PROOF_PRIVATE_KEY: proofPrivateKey,
        E2E_CAPABILITY_VERIFICATION_KEY: capabilityVerificationKey,
      },
    },
  } as unknown as RequestEvent;
}

async function authenticatedRecords() {
  await database.db.insert(users).values({ id: "user-http" });
  await database.db.insert(authSessions).values({
    id: sessionId,
    userId: "user-http",
    kind: "cli",
    tokenHash: await sha256Base64url(secret),
  });
  await database.db.insert(edges).values({
    id: edgeId,
    userId: "user-http",
    name: "HTTP Edge",
    tunnelUrl: "wss://http-edge.example/tunnel",
    serviceCredentialHash: "hash",
  });
}

test("remote registration endpoints require a valid CLI bearer credential", async () => {
  const response = await callListEdges(event("/api/remote/edges"));
  const deleted = await callDeleteDevice(
    event(`/api/remote/devices/${deviceId}`, { method: "DELETE" }),
  );

  expect(response.status).toBe(401);
  const error = (await response.json()) as { error: string };
  expect(error).toEqual({ error: "invalid_credentials" });
  expect(deleted.status).toBe(401);
});

test("remote HTTP API lists edges and creates, retries, and reads a device", async () => {
  await authenticatedRecords();

  const listed = await callListEdges(event("/api/remote/edges", { token: credential }));
  expect(listed.status).toBe(200);
  const edgeList = (await listed.json()) as Array<{ id: string; name: string }>;
  expect(edgeList).toContainEqual({ id: edgeId, name: "HTTP Edge" });

  const cloud = await crypto.subtle.generateKey("X25519", true, ["deriveBits"]);
  const device = await crypto.subtle.generateKey("X25519", true, ["deriveBits"]);
  proofPrivateKey = base64url.encode(
    new Uint8Array(await crypto.subtle.exportKey("pkcs8", cloud.privateKey)),
  );
  const devicePublic = new Uint8Array(await crypto.subtle.exportKey("raw", device.publicKey));
  const shared = await crypto.subtle.deriveBits(
    { name: "X25519", public: cloud.publicKey },
    device.privateKey,
    256,
  );
  const hkdf = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveKey"]);
  const proofKey = await crypto.subtle.deriveKey(
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
  const proofMessage = new Uint8Array(id.length + 1 + 8 + 32);
  proofMessage.set(id);
  new DataView(proofMessage.buffer).setBigUint64(id.length + 1, 1n);
  proofMessage.set(devicePublic, id.length + 9);
  const proof = new Uint8Array(await crypto.subtle.sign("HMAC", proofKey, proofMessage));
  const body = {
    name: "HTTP Device",
    edge_id: edgeId,
    e2e_public_key: base64url.encode(devicePublic),
    e2e_key_version: 1,
    e2e_key_proof: base64url.encode(proof),
  };
  const created = await callPutDevice(
    event(`/api/remote/devices/${deviceId}`, {
      method: "PUT",
      token: credential,
      body,
    }),
  );
  expect(created.status).toBe(201);
  const createdDevice = (await created.json()) as Record<string, unknown>;
  expect(createdDevice).toEqual({
    id: deviceId,
    device_handle: "http-device",
    name: "HTTP Device",
    edge_id: edgeId,
    edge_name: "HTTP Edge",
    e2e_public_key: base64url.encode(devicePublic),
    e2e_key_version: 1,
    capability_verification_key: capabilityVerificationKey,
  });

  const retried = await callPutDevice(
    event(`/api/remote/devices/${deviceId}`, {
      method: "PUT",
      token: credential,
      body,
    }),
  );
  expect(retried.status).toBe(200);
  const found = await callGetDevice(
    event(`/api/remote/devices/${deviceId}`, { token: credential }),
  );
  expect(found.status).toBe(200);
  expect(await found.json()).toMatchObject({ id: deviceId, edge_id: edgeId });

  const deleted = await callDeleteDevice(
    event(`/api/remote/devices/${deviceId}`, { method: "DELETE", token: credential }),
  );
  expect(deleted.status).toBe(204);
  const retriedDelete = await callDeleteDevice(
    event(`/api/remote/devices/${deviceId}`, { method: "DELETE", token: credential }),
  );
  expect(retriedDelete.status).toBe(204);
  const missing = await callGetDevice(
    event(`/api/remote/devices/${deviceId}`, { token: credential }),
  );
  expect(missing.status).toBe(404);
});
