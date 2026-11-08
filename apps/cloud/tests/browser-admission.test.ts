import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { base64url } from "jose";
import { sha256Base64url } from "../src/lib/server/crypto";
import { devices, edges, edgeTickets, users } from "../src/lib/server/db/schema";
import { connectDashboardDevice } from "../src/lib/server/remote-access/device-connect";
import { redeemTunnelTicket } from "../src/lib/server/remote-access/tickets";
import { POST } from "../src/routes/api/edge/browser-admission-tickets/redeem/+server";
import { testDatabase } from "./database";

const database = testDatabase();
const deviceId = "0195e7d3-1b22-7c33-9d44-123456789abc";
const edgeId = "0195e7d2-1b22-7c33-9d44-123456789abc";
const secret = base64url.encode(new Uint8Array(32).fill(32));
const credential = `pec_v1_${edgeId}_${secret}`;
const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
const signingKey = base64url.encode(
  new Uint8Array(await crypto.subtle.exportKey("pkcs8", keys.privateKey)),
);
const publicKey = base64url.encode(new Uint8Array(32).fill(7));
const redeem = POST as unknown as (event: RequestEvent) => Promise<Response>;

async function connect(now = new Date()) {
  return (await connectDashboardDevice(
    database.db,
    "owner",
    "office-device",
    publicKey,
    signingKey,
    now,
  ))!;
}

async function seed() {
  await database.db.insert(users).values([{ id: "owner" }, { id: "other" }]);
  await database.db.insert(edges).values({
    id: edgeId,
    userId: "owner",
    name: "Edge",
    dnsLabel: "brave-atlas",
    tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
    serviceCredentialHash: await sha256Base64url(secret),
  });
  await database.db.insert(devices).values({
    id: deviceId,
    userId: "owner",
    edgeId,
    handle: "office-device",
    name: "Office",
    e2ePublicKey: publicKey,
    e2eKeyVersion: 1,
  });
}

function request(body: unknown, token = credential, binding: unknown = database.binding) {
  return {
    request: new Request("https://pontia.dev/api/edge/browser-admission-tickets/redeem", {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
    platform: { env: { DB: binding } },
  } as RequestEvent;
}

test("connect issues a separate 60 second admission ticket bound to the same device and edge", async () => {
  await seed();
  const now = new Date();
  const connection = await connect(now);
  const ticket = await database.db.select().from(edgeTickets).get();
  expect(ticket).toMatchObject({
    purpose: "browser_admission",
    userId: "owner",
    expectedEdgeId: edgeId,
    payload: JSON.stringify({ device_id: connection.device_id }),
    consumedAt: null,
    createdAt: now.toISOString(),
    expiresAt: new Date(now.getTime() + 60_000).toISOString(),
  });
  expect(await redeemTunnelTicket(database.db, edgeId, connection.admission_ticket)).toBeNull();
  const responses = await Promise.all(
    Array.from({ length: 4 }, () => redeem(request({ ticket: connection.admission_ticket }))),
  );
  expect(responses.map((r) => r.status).sort()).toEqual([200, 401, 401, 401]);
  expect((await responses.find((r) => r.ok)!.json()) as unknown).toEqual({ device_id: deviceId });
});

test.each([
  "edge_deployment",
  "device_tunnel",
  "expired",
  "wrong-edge",
  "stale-owner",
  "stale-edge",
  "invalid-payload",
])("admission redemption rejects %s without consuming the ticket", async (kind) => {
  await seed();
  const connection = await connect();
  if (kind === "edge_deployment" || kind === "device_tunnel")
    await database.db.update(edgeTickets).set({ purpose: kind });
  if (kind === "expired")
    await database.db.update(edgeTickets).set({ expiresAt: "2000-01-01T00:00:00.000Z" });
  if (kind === "wrong-edge")
    await database.db.update(edgeTickets).set({ expectedEdgeId: "other-edge" });
  if (kind === "stale-owner")
    await database.db.update(devices).set({ userId: "other" }).where(eq(devices.id, deviceId));
  if (kind === "stale-edge") {
    await database.db.insert(edges).values({
      id: "other-edge",
      userId: "owner",
      name: "Other",
      dnsLabel: "calm-atlas",
      tunnelUrl: "wss://calm-atlas.edge.pontia.dev/tunnel",
      serviceCredentialHash: "other-hash",
    });
    await database.db.update(devices).set({ edgeId: "other-edge" }).where(eq(devices.id, deviceId));
  }
  if (kind === "invalid-payload")
    await database.db
      .update(edgeTickets)
      .set({ payload: JSON.stringify({ device_id: deviceId, extra: true }) });
  const response = await redeem(request({ ticket: connection.admission_ticket }));
  expect(response.status).toBe(401);
  expect((await response.json()) as unknown).toEqual({ error: "invalid_admission_ticket" });
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});

test("redemption distinguishes malformed tickets, edge credentials and service failures", async () => {
  await seed();
  for (const body of [
    {},
    { ticket: 7 },
    { ticket: "invalid" },
    { ticket: "invalid", device_id: deviceId },
  ]) {
    const response = await redeem(request(body));
    expect(response.status).toBe(401);
    expect((await response.json()) as unknown).toEqual({ error: "invalid_admission_ticket" });
  }
  const denied = await redeem(request({ ticket: "invalid" }, "invalid"));
  expect(denied.status).toBe(401);
  expect((await denied.json()) as unknown).toEqual({ error: "invalid_edge_credentials" });
  const unavailable = await redeem(request({ ticket: "invalid" }, credential, {}));
  expect(unavailable.status).toBe(503);
  expect((await unavailable.json()) as unknown).toEqual({ error: "service_unavailable" });
});
