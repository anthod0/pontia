import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { base64url } from "jose";
import { sha256Base64url } from "../src/lib/server/crypto";
import { devices, edgeTickets, edges, users } from "../src/lib/server/db/schema";
import {
  decodeDashboardAccessPayload,
  issueDashboardAccess,
  redeemDashboardAccess,
  type DashboardAccessDependencies,
} from "../src/lib/server/remote-access/dashboard-access";
import { testDatabase } from "./database";

const database = testDatabase();
const now = new Date("2099-01-01T00:00:00.000Z");
const capabilityExpiry = "2099-01-31T00:00:00.000Z";
const ownerId = "0195e7d1-1b22-7c33-9d44-123456789abc";
const otherId = "0195e7d2-1b22-7c33-9d44-123456789abc";
const deviceId = "0195e7d3-1b22-7c33-9d44-123456789abc";
const edgeId = "0195e7e1-1b22-7c33-9d44-123456789abc";
const otherEdgeId = "0195e7e2-1b22-7c33-9d44-123456789abc";
const secretBytes = new Uint8Array(32).fill(191);

const dependencies: DashboardAccessDependencies = {
  now: () => now,
  randomBytes: () => secretBytes.slice(),
};

async function seedFixture(tunnelUrl = "wss://brave-silver-atlas.edge.pontia.dev/tunnel") {
  await database.db.insert(users).values([{ id: ownerId }, { id: otherId }]);
  await database.db.insert(edges).values([
    {
      id: edgeId,
      userId: ownerId,
      name: "Owner edge",
      tunnelUrl,
      serviceCredentialHash: "owner-hash",
    },
    {
      id: otherEdgeId,
      userId: otherId,
      name: "Other edge",
      tunnelUrl: "wss://calm-blue-arthur.edge.pontia.dev/tunnel",
      serviceCredentialHash: "other-hash",
    },
  ]);
  await database.db.insert(devices).values({
    id: deviceId,
    userId: ownerId,
    edgeId,
    handle: "office-mac",
    name: "Office Mac",
  });
}

async function issueTicket() {
  const result = await issueDashboardAccess(database.db, ownerId, "office-mac", dependencies);
  if (result.status !== "issued") throw new Error("Expected a dashboard ticket");
  return result.value;
}

test("dashboard access payload decoding is strict", () => {
  expect(
    decodeDashboardAccessPayload({ device_id: deviceId, expires_at: capabilityExpiry }),
  ).toEqual({ device_id: deviceId, expires_at: capabilityExpiry });
  for (const invalid of [
    null,
    [],
    {},
    { device_id: deviceId },
    { device_id: "not-a-uuid", expires_at: capabilityExpiry },
    { device_id: deviceId.toUpperCase(), expires_at: capabilityExpiry },
    { device_id: deviceId, expires_at: "2099-01-31T00:00:00Z" },
    { device_id: deviceId, expires_at: "not-a-time" },
    { device_id: deviceId, expires_at: capabilityExpiry, device_handle: "office-mac" },
  ]) {
    expect(decodeDashboardAccessPayload(invalid)).toBeNull();
  }
});

test("issuing binds a short dashboard ticket to the device and its canonical edge", async () => {
  await seedFixture();

  const issued = await issueTicket();
  const secret = base64url.encode(secretBytes);
  expect(issued.bootstrapUrl).toBe(
    "https://brave-silver-atlas.edge.pontia.dev/dashboard/bootstrap",
  );
  expect(issued.ticket).toBe(`pet_v1_${secret}`);

  const stored = await database.db.select().from(edgeTickets).get();
  expect(stored).toMatchObject({
    purpose: "dashboard_access",
    userId: ownerId,
    expectedEdgeId: edgeId,
    payload: JSON.stringify({ device_id: deviceId, expires_at: capabilityExpiry }),
    expiresAt: "2099-01-01T00:01:00.000Z",
    createdAt: now.toISOString(),
    consumedAt: null,
    secretHash: await sha256Base64url(secret),
  });
  expect(JSON.stringify(stored)).not.toContain(secret);
});

test("issuing hides unowned devices and rejects noncanonical edge tunnel URLs", async () => {
  await seedFixture();
  expect(await issueDashboardAccess(database.db, otherId, "office-mac", dependencies)).toEqual({
    status: "device_not_found",
  });
  expect(await issueDashboardAccess(database.db, ownerId, "missing-device", dependencies)).toEqual({
    status: "device_not_found",
  });

  await database.db
    .update(edges)
    .set({ tunnelUrl: "wss://attacker.example/tunnel" })
    .where(eq(edges.id, edgeId));
  expect(await issueDashboardAccess(database.db, ownerId, "office-mac", dependencies)).toEqual({
    status: "device_not_found",
  });
});

test("redemption succeeds once and returns only current device capability fields", async () => {
  await seedFixture();
  const issued = await issueTicket();

  expect(await redeemDashboardAccess(database.db, edgeId, issued.ticket)).toEqual({
    deviceId,
    deviceHandle: "office-mac",
    expiresAt: capabilityExpiry,
  });
  expect(await redeemDashboardAccess(database.db, edgeId, issued.ticket)).toBeNull();
});

test("redemption rejects the wrong edge, purpose, expiry, owner, and binding without consuming", async () => {
  for (const testCase of [
    "edge",
    "purpose",
    "expiry",
    "capability-lifetime",
    "owner",
    "binding",
  ] as const) {
    await database.db.delete(devices);
    await database.db.delete(edges);
    await database.db.delete(users);
    await seedFixture();
    const issued = await issueTicket();
    const stored = await database.db.select().from(edgeTickets).get();
    if (!stored) throw new Error("Expected a stored dashboard ticket");

    let redeemingEdge = edgeId;
    if (testCase === "edge") redeemingEdge = otherEdgeId;
    if (testCase === "purpose") {
      await database.db
        .update(edgeTickets)
        .set({ purpose: "device_tunnel" })
        .where(eq(edgeTickets.id, stored.id));
    }
    if (testCase === "expiry") {
      await database.db
        .update(edgeTickets)
        .set({ expiresAt: "2000-01-01T00:00:00.000Z" })
        .where(eq(edgeTickets.id, stored.id));
    }
    if (testCase === "capability-lifetime") {
      await database.db
        .update(edgeTickets)
        .set({
          payload: JSON.stringify({
            device_id: deviceId,
            expires_at: "2099-02-01T00:00:00.000Z",
          }),
        })
        .where(eq(edgeTickets.id, stored.id));
    }
    if (testCase === "owner") {
      await database.db.update(devices).set({ userId: otherId }).where(eq(devices.id, deviceId));
    }
    if (testCase === "binding") {
      await database.db
        .update(devices)
        .set({ edgeId: otherEdgeId })
        .where(eq(devices.id, deviceId));
    }

    expect(await redeemDashboardAccess(database.db, redeemingEdge, issued.ticket)).toBeNull();
    const unchanged = await database.db
      .select({ consumedAt: edgeTickets.consumedAt })
      .from(edgeTickets)
      .where(eq(edgeTickets.id, stored.id))
      .get();
    expect(unchanged?.consumedAt).toBeNull();
  }
});
