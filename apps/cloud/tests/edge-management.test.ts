import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { removeOwnedEdge } from "../src/lib/server/edge-management";
import { devices, edges, edgeTickets, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const database = testDatabase();
const ownerId = "0199791c-6600-7000-8000-000000000001";
const otherUserId = "0199791c-6600-7000-8000-000000000002";
const edgeId = "0199791c-6600-7000-8000-000000000003";

async function insertEdgeResources() {
  await database.db.insert(users).values([{ id: ownerId }, { id: otherUserId }]);
  await database.db.insert(edges).values({
    id: edgeId,
    userId: ownerId,
    name: "Brave Atlas",
    dnsLabel: "brave-atlas",
    tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
    serviceCredentialHash: "hash",
    accessScope: "public",
  });
  await database.db.insert(devices).values({
    id: "0199791c-6600-7000-8000-000000000004",
    userId: otherUserId,
    edgeId,
    handle: "remote-device",
    name: "Remote device",
  });
  await database.db.insert(edgeTickets).values({
    purpose: "device_tunnel",
    secretHash: "ticket-hash",
    userId: ownerId,
    expectedEdgeId: edgeId,
    payload: "{}",
    expiresAt: "2099-01-01T00:00:00.000Z",
  });
}

test("deleting an owned edge cleans DNS and removes its dependent Cloud resources", async () => {
  await insertEdgeResources();
  const cleaned: string[] = [];

  expect(
    await removeOwnedEdge(database.db, ownerId, edgeId, {
      async cleanupHostname(hostname) {
        cleaned.push(hostname);
      },
    }),
  ).toEqual({ status: "deleted" });

  expect(cleaned).toEqual(["brave-atlas.edge.pontia.dev"]);
  expect(await database.db.select().from(edges).where(eq(edges.id, edgeId))).toEqual([]);
  expect(await database.db.select().from(devices).where(eq(devices.edgeId, edgeId))).toEqual([]);
  expect(
    await database.db.select().from(edgeTickets).where(eq(edgeTickets.expectedEdgeId, edgeId)),
  ).toEqual([]);
});

test("a DNS cleanup failure keeps the edge and dependent resources available for retry", async () => {
  await insertEdgeResources();

  await expect(
    removeOwnedEdge(database.db, ownerId, edgeId, {
      async cleanupHostname() {
        throw new Error("provider unavailable");
      },
    }),
  ).rejects.toThrow("provider unavailable");

  expect(await database.db.select().from(edges).where(eq(edges.id, edgeId))).toHaveLength(1);
  expect(await database.db.select().from(devices).where(eq(devices.edgeId, edgeId))).toHaveLength(
    1,
  );
  expect(
    await database.db.select().from(edgeTickets).where(eq(edgeTickets.expectedEdgeId, edgeId)),
  ).toHaveLength(1);
});

test("an edge cannot be deleted by another user", async () => {
  await insertEdgeResources();
  let cleanupCalled = false;

  expect(
    await removeOwnedEdge(database.db, otherUserId, edgeId, {
      async cleanupHostname() {
        cleanupCalled = true;
      },
    }),
  ).toEqual({ status: "not_found" });

  expect(cleanupCalled).toBe(false);
  expect(await database.db.select().from(edges).where(eq(edges.id, edgeId))).toHaveLength(1);
});
