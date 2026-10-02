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
    platform: { env: { DB: database.binding } },
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

  const body = { name: "HTTP Device", edge_id: edgeId };
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
