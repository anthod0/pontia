import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { base64url } from "jose";
import {
  authenticateEdgeCredential,
  deviceBindingIsCurrent,
  findOwnedDeviceTarget,
} from "../src/lib/server/remote-access/resources";
import { devices, edges, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const database = testDatabase();
const secret = base64url.encode(new Uint8Array(32).fill(7));

async function hash(value: string) {
  return base64url.encode(
    new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value))),
  );
}

async function insertEdge(id: string, credentialSecret = secret, userId = "edge-owner") {
  await database.db.insert(edges).values({
    id,
    userId,
    name: `Edge ${id}`,
    dnsLabel: id,
    tunnelUrl: `wss://${id}.example.com/tunnel`,
    serviceCredentialHash: await hash(credentialSecret),
  });
}

test("edge credentials authenticate only a strict matching credential", async () => {
  await database.db.insert(users).values({ id: "edge-owner" });
  const edgeId = "0199791c-6600-7000-8000-000000000010";
  await insertEdge(edgeId);
  const credential = `pec_v1_${edgeId}_${secret}`;

  expect(await authenticateEdgeCredential(database.db, credential)).toEqual({
    edgeId,
  });
  expect(
    await authenticateEdgeCredential(
      database.db,
      `pec_v1_${edgeId}_${base64url.encode(new Uint8Array(32).fill(8))}`,
    ),
  ).toBeNull();
  expect(await authenticateEdgeCredential(database.db, `pec_v2_${edgeId}_${secret}`)).toBeNull();
  expect(
    await authenticateEdgeCredential(
      database.db,
      `pec_v1_0199791c-6600-7000-8000-000000000099_${secret}`,
    ),
  ).toBeNull();
  expect(await authenticateEdgeCredential(database.db, `${credential}_extra`)).toBeNull();
});

test("owned device lookup returns its current edge target", async () => {
  await database.db
    .insert(users)
    .values([{ id: "edge-owner" }, { id: "user-owner" }, { id: "user-other" }]);
  await insertEdge("edge-target", secret, "user-owner");
  await database.db.insert(devices).values({
    id: "device-owned",
    userId: "user-owner",
    edgeId: "edge-target",
    handle: "owned-device",
    name: "Owned device",
  });

  expect(await findOwnedDeviceTarget(database.db, "user-owner", "device-owned")).toEqual({
    deviceId: "device-owned",
    edgeId: "edge-target",
    tunnelUrl: "wss://edge-target.example.com/tunnel",
  });
  expect(await findOwnedDeviceTarget(database.db, "user-other", "device-owned")).toBeNull();
  expect(await findOwnedDeviceTarget(database.db, "user-owner", "device-missing")).toBeNull();
});

test("device binding checks owner, device, and edge together", async () => {
  await database.db.insert(users).values([{ id: "edge-owner" }, { id: "user-binding" }]);
  await insertEdge("edge-binding", secret, "user-binding");
  await insertEdge("edge-other");
  await database.db.insert(devices).values({
    id: "device-binding",
    userId: "user-binding",
    edgeId: "edge-binding",
    handle: "bound-device",
    name: "Bound device",
  });

  expect(
    await deviceBindingIsCurrent(database.db, "user-binding", "device-binding", "edge-binding"),
  ).toBe(true);
  expect(
    await deviceBindingIsCurrent(database.db, "user-binding", "device-binding", "edge-other"),
  ).toBe(false);
  expect(
    await deviceBindingIsCurrent(database.db, "user-other", "device-binding", "edge-binding"),
  ).toBe(false);
});

test("device foreign keys cascade owners and restrict deleting assigned edges", async () => {
  await database.db.insert(users).values([{ id: "edge-owner" }, { id: "user-constraints" }]);
  await insertEdge("edge-constraints");
  await database.db.insert(devices).values({
    id: "device-constraints",
    userId: "user-constraints",
    edgeId: "edge-constraints",
    handle: "constrained-device",
    name: "Constrained device",
  });

  await expect(
    Promise.resolve(database.db.delete(edges).where(eq(edges.id, "edge-constraints"))),
  ).rejects.toThrow();
  await database.db.delete(users).where(eq(users.id, "user-constraints"));
  expect(
    await database.db.select().from(devices).where(eq(devices.id, "device-constraints")),
  ).toHaveLength(0);
  await expect(
    Promise.resolve(database.db.delete(edges).where(eq(edges.id, "edge-constraints"))),
  ).resolves.toBeDefined();
});
