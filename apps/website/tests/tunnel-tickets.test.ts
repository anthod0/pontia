import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { base64url } from "jose";
import { sha256Base64url } from "../src/lib/server/crypto";
import { parseEdgeTicket } from "../src/lib/server/edge-tickets";
import { devices, edgeTickets, edges, users } from "../src/lib/server/db/schema";
import {
  decodeDeviceTunnelPayload,
  issueTunnelTicket,
  redeemTunnelTicket,
  type TunnelTicketDependencies,
} from "../src/lib/server/remote-access/tickets";
import { testDatabase } from "./database";

const database = testDatabase();
const now = new Date("2026-09-24T12:00:00.000Z");
const redemptionTime = new Date("2099-01-01T00:00:00.000Z");
const deviceId = "0199791c-6600-7000-8000-000000000001";
const otherDeviceId = "0199791c-6600-7000-8000-000000000002";
const secretBytes = new Uint8Array(32).fill(191);

function dependencies(currentTime = now): TunnelTicketDependencies {
  return {
    now: () => currentTime,
    randomBytes: () => secretBytes.slice(),
  };
}

async function seedTicketFixture() {
  await database.db.insert(users).values([{ id: "user-owner" }, { id: "user-other" }]);
  await database.db.insert(edges).values([
    {
      id: "edge-target",
      name: "Target Edge",
      tunnelUrl: "wss://target.example/tunnel",
      serviceCredentialHash: "target-hash",
    },
    {
      id: "edge-other",
      name: "Other Edge",
      tunnelUrl: "wss://other.example/tunnel",
      serviceCredentialHash: "other-hash",
    },
  ]);
  await database.db.insert(devices).values({
    id: deviceId,
    userId: "user-owner",
    edgeId: "edge-target",
  });
}

async function issuedTicket() {
  const result = await issueTunnelTicket(
    database.db,
    "user-owner",
    deviceId,
    dependencies(redemptionTime),
  );
  if (result.status !== "issued") throw new Error("Expected a ticket");
  return result.value;
}

test("ticket parsing accepts only canonical pet v1 tickets", () => {
  const secret = base64url.encode(secretBytes);
  const ticket = `pet_v1_${secret}`;

  expect(secret).toContain("-");
  expect(secret).toContain("_");
  expect(parseEdgeTicket(ticket)).toEqual({ secret });
  for (const invalid of [
    ticket.replace("pet_v1", "pet_v2"),
    ticket.replace("pet_v1", "ptt_v1"),
    `pet_v1_${deviceId}_${secret}`,
    `${ticket}x`,
    ticket.replace(secret, `${secret.slice(0, -1)}=`),
    `pet_v1_${"A".repeat(42)}`,
  ]) {
    expect(parseEdgeTicket(invalid)).toBeNull();
  }
});

test("device tunnel payload decoding is strict", () => {
  expect(decodeDeviceTunnelPayload({ device_id: deviceId })).toEqual({ device_id: deviceId });
  for (const invalid of [
    null,
    [],
    "device",
    {},
    { device_id: 1 },
    { device_id: "not-a-uuid" },
    { device_id: deviceId.toUpperCase() },
    { device_id: deviceId, extra: true },
  ]) {
    expect(decodeDeviceTunnelPayload(invalid)).toBeNull();
  }
});

test("issuing binds an opaque 60 second ticket without storing its secret", async () => {
  await seedTicketFixture();

  const result = await issueTunnelTicket(database.db, "user-owner", deviceId, dependencies());
  const secret = base64url.encode(secretBytes);

  expect(result).toEqual({
    status: "issued",
    value: {
      ticket: `pet_v1_${secret}`,
      tunnelUrl: "wss://target.example/tunnel",
      expiresAt: "2026-09-24T12:01:00.000Z",
    },
  });
  const stored = await database.db.select().from(edgeTickets).get();
  expect(stored).toMatchObject({
    purpose: "device_tunnel",
    userId: "user-owner",
    expectedEdgeId: "edge-target",
    payload: JSON.stringify({ device_id: deviceId }),
    expiresAt: "2026-09-24T12:01:00.000Z",
    consumedAt: null,
    secretHash: await sha256Base64url(secret),
  });
  expect(Number.isInteger(stored?.id)).toBe(true);
  expect(JSON.stringify(stored)).not.toContain(secret);
});

test("issuing hides devices not owned by the authenticated user", async () => {
  await seedTicketFixture();

  expect(await issueTunnelTicket(database.db, "user-other", deviceId, dependencies())).toEqual({
    status: "device_not_found",
  });
  expect(await issueTunnelTicket(database.db, "user-owner", otherDeviceId, dependencies())).toEqual(
    { status: "device_not_found" },
  );
});

test("a ticket can be redeemed once only by its bound edge", async () => {
  await seedTicketFixture();
  const issued = await issuedTicket();

  expect(await redeemTunnelTicket(database.db, "edge-other", issued.ticket)).toBeNull();
  expect(await redeemTunnelTicket(database.db, "edge-target", issued.ticket)).toEqual({
    deviceId,
  });
  expect(await redeemTunnelTicket(database.db, "edge-target", issued.ticket)).toBeNull();
});

test("redemption rejects wrong secrets, purpose, expiry, payload, and stale bindings", async () => {
  const cases = [
    "wrong-secret",
    "wrong-purpose",
    "expired",
    "invalid-payload",
    "stale-owner",
    "stale-binding",
  ];
  for (const testCase of cases) {
    await database.db.delete(users);
    await database.db.delete(edges);
    await seedTicketFixture();
    const issued = await issuedTicket();
    const stored = await database.db.select().from(edgeTickets).get();
    if (!stored) throw new Error("Expected stored ticket");

    if (testCase === "wrong-secret") {
      const replacement = base64url.encode(new Uint8Array(32).fill(24));
      expect(
        await redeemTunnelTicket(database.db, "edge-target", `pet_v1_${replacement}`),
      ).toBeNull();
    } else if (testCase === "wrong-purpose") {
      await database.db
        .update(edgeTickets)
        .set({ purpose: "dashboard_access" })
        .where(eq(edgeTickets.id, stored.id));
      expect(await redeemTunnelTicket(database.db, "edge-target", issued.ticket)).toBeNull();
    } else if (testCase === "expired") {
      await database.db
        .update(edgeTickets)
        .set({ expiresAt: "2000-01-01T00:00:00.000Z" })
        .where(eq(edgeTickets.id, stored.id));
      expect(await redeemTunnelTicket(database.db, "edge-target", issued.ticket)).toBeNull();
    } else if (testCase === "invalid-payload") {
      await database.db
        .update(edgeTickets)
        .set({ payload: JSON.stringify({ device_id: deviceId, extra: true }) })
        .where(eq(edgeTickets.id, stored.id));
      expect(await redeemTunnelTicket(database.db, "edge-target", issued.ticket)).toBeNull();
    } else if (testCase === "stale-owner") {
      await database.db
        .update(devices)
        .set({ userId: "user-other" })
        .where(eq(devices.id, deviceId));
      expect(await redeemTunnelTicket(database.db, "edge-target", issued.ticket)).toBeNull();
    } else {
      await database.db
        .update(devices)
        .set({ edgeId: "edge-other" })
        .where(eq(devices.id, deviceId));
      expect(await redeemTunnelTicket(database.db, "edge-target", issued.ticket)).toBeNull();
    }

    const unchanged = await database.db
      .select({ consumedAt: edgeTickets.consumedAt })
      .from(edgeTickets)
      .where(eq(edgeTickets.id, stored.id))
      .get();
    expect(unchanged?.consumedAt).toBeNull();
  }
});

test("concurrent redemption consumes a ticket at most once", async () => {
  await seedTicketFixture();
  const issued = await issuedTicket();

  const results = await Promise.all(
    Array.from({ length: 8 }, () => redeemTunnelTicket(database.db, "edge-target", issued.ticket)),
  );

  expect(results.filter((result) => result !== null)).toEqual([{ deviceId }]);
});

test("issuing removes expired edge ticket records", async () => {
  await seedTicketFixture();
  await database.db.insert(edgeTickets).values({
    purpose: "device_tunnel",
    secretHash: "expired-hash",
    userId: "user-owner",
    expectedEdgeId: "edge-target",
    payload: JSON.stringify({ device_id: deviceId }),
    expiresAt: now.toISOString(),
  });

  await issuedTicket();

  expect(
    await database.db
      .select({ id: edgeTickets.id })
      .from(edgeTickets)
      .where(eq(edgeTickets.secretHash, "expired-hash"))
      .get(),
  ).toBeUndefined();
});
