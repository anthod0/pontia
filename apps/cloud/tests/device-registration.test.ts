import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { devices, edges, users } from "../src/lib/server/db/schema";
import {
  findRegisteredDevice,
  registerDevice as registerDeviceWithIdentity,
  registrationEdges,
  unregisterDevice,
} from "../src/lib/server/remote-access/registration";
import { testDatabase } from "./database";

const database = testDatabase();
const edgeTokyo = "0195e7b2-3f45-7a61-8c20-1d34a56b7890";
const edgeSingapore = "0195e7b8-91c2-73d4-a560-2f78b90c1234";
const registrationEdge = "0195e7b9-91c2-73d4-a560-2f78b90c1234";
const conflictEdge = "0195e7ba-91c2-73d4-a560-2f78b90c1234";
const conflictOtherEdge = "0195e7bb-91c2-73d4-a560-2f78b90c1234";
const deviceId = "0195e7c1-1b22-7c33-9d44-123456789abc";

function registerDevice(
  db: Parameters<typeof registerDeviceWithIdentity>[0],
  userId: string,
  id: string,
  name: string,
  edgeId: string,
) {
  return registerDeviceWithIdentity(
    db,
    userId,
    id,
    name,
    edgeId,
    `${id.replaceAll("-", "")}AAAAAAAAAAA`,
    1,
  );
}

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
    dnsLabel: id,
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
      handle: "my-device",
      name: "My device",
      edgeId: registrationEdge,
      edgeName: "Tokyo",
      edgeMode: "fixed",
      e2ePublicKey: `${deviceId.replaceAll("-", "")}AAAAAAAAAAA`,
      e2eKeyVersion: 1,
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

test("device unregistration is idempotent and enforces ownership", async () => {
  await database.db.insert(users).values([{ id: "user-owner" }, { id: "user-other" }]);
  await insertEdge(registrationEdge, "Tokyo");
  await database.db.insert(devices).values({
    id: deviceId,
    userId: "user-owner",
    edgeId: registrationEdge,
    handle: "owned-device",
    name: "Owned",
  });

  expect(await unregisterDevice(database.db, "user-other", deviceId)).toEqual({
    status: "conflict",
  });
  expect(await unregisterDevice(database.db, "user-owner", deviceId)).toEqual({
    status: "deleted",
  });
  expect(await unregisterDevice(database.db, "user-owner", deviceId)).toEqual({
    status: "not_found",
  });
});

test("device names are required by the database", async () => {
  await database.db.insert(users).values({ id: "user-owner" });
  await insertEdge(registrationEdge, "Tokyo");

  await expect(
    database.binding
      .prepare("INSERT INTO devices(id, user_id, edge_id, handle, name) VALUES (?, ?, ?, ?, NULL)")
      .bind(deviceId, "user-owner", registrationEdge, "missing-name")
      .run(),
  ).rejects.toThrow();
});

test("device handles are stable and unique within a user", async () => {
  await database.db.insert(users).values({ id: "user-owner" });
  await insertEdge(registrationEdge, "Tokyo");
  const secondDeviceId = "0195e7c2-1b22-7c33-9d44-123456789abc";

  const first = await registerDevice(
    database.db,
    "user-owner",
    deviceId,
    "Office machine",
    registrationEdge,
  );
  const second = await registerDevice(
    database.db,
    "user-owner",
    secondDeviceId,
    "Office machine",
    registrationEdge,
  );
  expect(first).toMatchObject({ device: { handle: "office-machine" } });
  expect(second).toMatchObject({ device: { handle: "office-machine-0195e7c2" } });
  expect(
    await registerDevice(
      database.db,
      "user-owner",
      secondDeviceId,
      "Renamed machine",
      registrationEdge,
    ),
  ).toMatchObject({ status: "existing", device: { handle: "office-machine-0195e7c2" } });
});

test("registration rejects invalid edges and device conflicts", async () => {
  await database.db.insert(users).values([{ id: "user-owner" }, { id: "user-other" }]);
  await insertEdge(conflictEdge, "Tokyo");
  await insertEdge(conflictOtherEdge, "Singapore", "user-other");
  await database.db.insert(devices).values({
    id: deviceId,
    userId: "user-owner",
    edgeId: conflictEdge,
    handle: "owned-device",
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

test("automatic registration requires an accessible edge and keeps its binding on retry", async () => {
  await database.db.insert(users).values([{ id: "owner" }, { id: "other" }]);
  await insertEdge(edgeSingapore, "Private", "other");
  const register = () => registerDevice(database.db, "owner", deviceId, "Workstation", "auto");
  expect(await register()).toEqual({ status: "edge_not_found" });
  await insertEdge(edgeTokyo, "Public", "other", "public");
  expect(await register()).toMatchObject({
    status: "created",
    device: { edgeMode: "auto", edgeId: edgeTokyo },
  });
  await insertEdge(registrationEdge, "Another", "owner");
  expect(await register()).toMatchObject({
    status: "existing",
    device: { edgeMode: "auto", edgeId: edgeTokyo },
  });
});

test("the selection migration keeps existing devices pinned and defaults new rows to auto", async () => {
  const db = new Database(":memory:");
  try {
    db.exec("CREATE TABLE devices(id TEXT PRIMARY KEY, edge_id TEXT NOT NULL)");
    const insert = "INSERT INTO devices (id, edge_id) VALUES (?, ?)";
    db.run(insert, ["before", edgeTokyo]);
    const migration = new URL(
      "../migrations/20261124104004_auto_edge_selection/migration.sql",
      import.meta.url,
    );
    db.exec(await Bun.file(migration).text());
    db.run(insert, ["after", edgeSingapore]);
    expect(db.query("SELECT id, edge_id, edge_mode FROM devices ORDER BY id").all()).toEqual([
      { id: "after", edge_id: edgeSingapore, edge_mode: "auto" },
      { id: "before", edge_id: edgeTokyo, edge_mode: "fixed" },
    ]);
  } finally {
    db.close();
  }
});
