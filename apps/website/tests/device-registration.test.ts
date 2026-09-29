import { expect, test } from "bun:test";
import { devices, edges, users } from "../src/lib/server/db/schema";
import {
  findRegisteredDevice,
  registerDevice,
  registrationEdges,
} from "../src/lib/server/remote-access/registration";
import { testDatabase } from "./database";

const database = testDatabase();
const edgeTokyo = "0195e7b2-3f45-7a61-8c20-1d34a56b7890";
const edgeSingapore = "0195e7b8-91c2-73d4-a560-2f78b90c1234";
const registrationEdge = "0195e7b9-91c2-73d4-a560-2f78b90c1234";
const conflictEdge = "0195e7ba-91c2-73d4-a560-2f78b90c1234";
const conflictOtherEdge = "0195e7bb-91c2-73d4-a560-2f78b90c1234";
const deviceId = "0195e7c1-1b22-7c33-9d44-123456789abc";

async function insertEdge(
  id: string,
  name: string,
  userId = "user-owner",
  accessScope: "private" | "public" = "private",
) {
  await database.db.insert(edges).values({
    id,
    userId,
    accessScope,
    name,
    tunnelUrl: `wss://${id}.example/tunnel`,
    serviceCredentialHash: "hash",
  });
}

test("registration edges expose owned and public edges only in stable order", async () => {
  await database.db.insert(users).values([{ id: "user-owner" }, { id: "user-other" }]);
  await insertEdge(edgeTokyo, "Tokyo");
  await insertEdge(edgeSingapore, "Singapore", "user-other", "public");
  await insertEdge(conflictEdge, "Hidden", "user-other");

  expect(await registrationEdges(database.db, "user-owner")).toEqual([
    { id: edgeSingapore, name: "Singapore" },
    { id: edgeTokyo, name: "Tokyo" },
  ]);
});

test("a user can register a device only to an owned or public edge", async () => {
  await database.db.insert(users).values([{ id: "user-owner" }, { id: "user-other" }]);
  await insertEdge(edgeTokyo, "Private", "user-other");
  await insertEdge(edgeSingapore, "Public", "user-other", "public");

  expect(
    await registerDevice(database.db, "user-owner", deviceId, "Private target", edgeTokyo),
  ).toEqual({ status: "edge_not_found" });
  expect(
    await registerDevice(database.db, "user-owner", deviceId, "Public target", edgeSingapore),
  ).toMatchObject({ status: "created", device: { edgeId: edgeSingapore } });
});

test("device registration creates once and returns the stored record on retry", async () => {
  await database.db.insert(users).values({ id: "user-owner" });
  await insertEdge(registrationEdge, "Tokyo");

  expect(
    await registerDevice(database.db, "user-owner", deviceId, "My device", registrationEdge),
  ).toEqual({
    status: "created",
    device: {
      id: deviceId,
      name: "My device",
      edgeId: registrationEdge,
      edgeName: "Tokyo",
    },
  });
  expect(
    await registerDevice(
      database.db,
      "user-owner",
      deviceId,
      "Retry name is not a rename",
      registrationEdge,
    ),
  ).toMatchObject({
    status: "existing",
    device: { name: "My device" },
  });
  expect(await database.db.select().from(devices)).toHaveLength(1);
  expect(await findRegisteredDevice(database.db, "user-owner", deviceId)).toMatchObject({
    status: "found",
    device: { edgeId: registrationEdge },
  });
});

test("registration rejects invalid edges and device conflicts", async () => {
  await database.db.insert(users).values([{ id: "user-owner" }, { id: "user-other" }]);
  await insertEdge(conflictEdge, "Tokyo");
  await insertEdge(conflictOtherEdge, "Singapore", "user-other");
  await database.db.insert(devices).values({
    id: deviceId,
    userId: "user-owner",
    edgeId: conflictEdge,
    name: "Owned",
  });

  expect(await registerDevice(database.db, "user-other", deviceId, "Other", conflictEdge)).toEqual({
    status: "conflict",
  });
  expect(
    await registerDevice(database.db, "user-owner", deviceId, "Owned", conflictOtherEdge),
  ).toEqual({ status: "conflict" });
  expect(
    await registerDevice(
      database.db,
      "user-owner",
      "0195e7c2-1b22-7c33-9d44-123456789abc",
      "New",
      conflictOtherEdge,
    ),
  ).toEqual({ status: "edge_not_found" });
  expect(
    await registerDevice(
      database.db,
      "user-owner",
      "0195e7c3-1b22-7c33-9d44-123456789abc",
      "Missing",
      "0195e7c4-1b22-7c33-9d44-123456789abc",
    ),
  ).toEqual({ status: "edge_not_found" });
  expect(await findRegisteredDevice(database.db, "user-other", deviceId)).toEqual({
    status: "conflict",
  });
});
