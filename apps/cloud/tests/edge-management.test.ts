import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { removeOwnedEdge, type EdgeDnsCleanup } from "../src/lib/server/edge-management";
import { devices, edges, edgeTickets, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const database = testDatabase();
const ownerId = "0199791c-6600-7000-8000-000000000001";
const otherUserId = "0199791c-6600-7000-8000-000000000002";
const edgeId = "0199791c-6600-7000-8000-000000000003";

const managed = { tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel" };
const selfHosted = { tunnelUrl: "wss://edge.example.com/tunnel" };

async function insertEdgeResources(
  accessScope: "private" | "public" = "private",
  endpoint: { tunnelUrl: string } = managed,
) {
  await database.db.insert(users).values([{ id: ownerId }, { id: otherUserId }]);
  await database.db.insert(edges).values({
    id: edgeId,
    userId: ownerId,
    name: "Brave Atlas",
    ...endpoint,
    serviceCredentialHash: "hash",
    accessScope,
  });
  await database.db.insert(devices).values({
    id: "0199791c-6600-7000-8000-000000000004",
    userId: accessScope === "public" ? otherUserId : ownerId,
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

const noDns: EdgeDnsCleanup = {
  async cleanupHostname() {
    throw new Error("unexpected DNS cleanup");
  },
};

async function expectRemoved() {
  expect(await database.db.select().from(edges).where(eq(edges.id, edgeId))).toEqual([]);
  expect(await database.db.select().from(devices).where(eq(devices.edgeId, edgeId))).toEqual([]);
  expect(
    await database.db.select().from(edgeTickets).where(eq(edgeTickets.expectedEdgeId, edgeId)),
  ).toEqual([]);
}

test("deleting a managed edge of a legacy DNS owner cleans DNS and removes its resources", async () => {
  await insertEdgeResources();
  const cleaned: string[][] = [];

  expect(
    await removeOwnedEdge(
      database.db,
      ownerId,
      edgeId,
      {
        async cleanupHostname(hostname, label) {
          cleaned.push([hostname, label]);
        },
      },
      true,
    ),
  ).toEqual({ status: "deleted" });

  expect(cleaned).toEqual([["brave-atlas.edge.pontia.dev", "brave-atlas"]]);
  await expectRemoved();
});

test("deleting a self-hosted edge on its own domain never touches DNS", async () => {
  for (const managesDns of [true, false]) {
    await database.db.delete(devices);
    await database.db.delete(edges);
    await database.db.delete(users);
    await insertEdgeResources("private", selfHosted);
    expect(await removeOwnedEdge(database.db, ownerId, edgeId, noDns, managesDns)).toEqual({
      status: "deleted",
    });
    await expectRemoved();
  }
});

test("a managed edge outside the legacy DNS allowlist is removed without DNS cleanup", async () => {
  await insertEdgeResources();
  expect(await removeOwnedEdge(database.db, ownerId, edgeId, noDns, false)).toEqual({
    status: "deleted",
  });
  await expectRemoved();
});

test("a DNS cleanup failure keeps the edge and dependent resources available for retry", async () => {
  await insertEdgeResources();

  await expect(
    removeOwnedEdge(
      database.db,
      ownerId,
      edgeId,
      {
        async cleanupHostname() {
          throw new Error("provider unavailable");
        },
      },
      true,
    ),
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

  expect(await removeOwnedEdge(database.db, otherUserId, edgeId, noDns, true)).toEqual({
    status: "not_found",
  });

  expect(await database.db.select().from(edges).where(eq(edges.id, edgeId))).toHaveLength(1);
});

test("an owner cannot remove a public edge or its resources", async () => {
  await insertEdgeResources("public");
  expect(await removeOwnedEdge(database.db, ownerId, edgeId, noDns, true)).toEqual({
    status: "not_found",
  });
  expect(await database.db.select().from(edges)).toHaveLength(1);
  expect(await database.db.select().from(devices)).toHaveLength(1);
  expect(await database.db.select().from(edgeTickets)).toHaveLength(1);
});
