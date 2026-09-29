import { expect, test } from "bun:test";
import { base64url } from "jose";
import { eq } from "drizzle-orm";
import {
  decodeDeploymentPayload,
  enrollEdge,
  issueEdgeDeployment,
  type DeploymentDependencies,
} from "../src/lib/server/edge-deployment";
import { sha256Base64url } from "../src/lib/server/crypto";
import { edgeTickets, edges, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const database = testDatabase();
const edgeId = "0199791c-6600-7000-8000-000000000001";
const now = new Date("2099-01-01T00:00:00.000Z");
const ticketBytes = new Uint8Array(32).fill(41);
const credentialSecret = base64url.encode(new Uint8Array(32).fill(42));
const credential = `pec_v1_${edgeId}_${credentialSecret}`;

function dependencies(): DeploymentDependencies {
  return {
    now: () => now,
    randomBytes: () => ticketBytes.slice(),
    edgeId: () => edgeId,
    heroName: () => "brave-silver-atlas",
  };
}

async function issuedDeployment() {
  await database.db.insert(users).values({ id: "user-owner" });
  return issueEdgeDeployment(database.db, "user-owner", "https://pontia.example", dependencies());
}

function ticketFrom(command: string) {
  const match = /--ticket '(pet_v1_[A-Za-z0-9_-]{43})'/.exec(command);
  if (!match) throw new Error("Expected deployment ticket in command");
  return match[1];
}

test("deployment payload decoding accepts only one reviewed hero name", () => {
  expect(decodeDeploymentPayload({ name: "brave-silver-atlas" })).toEqual({
    name: "brave-silver-atlas",
  });
  for (const invalid of [
    null,
    [],
    {},
    { name: "custom-name" },
    { name: "brave-silver-atlas", extra: true },
  ]) {
    expect(decodeDeploymentPayload(invalid)).toBeNull();
  }
});

test("issuing a deployment creates a one-hour bound ticket and copyable command", async () => {
  const deployment = await issuedDeployment();
  const ticket = ticketFrom(deployment.command);
  const secret = ticket.slice("pet_v1_".length);

  expect(deployment).toMatchObject({
    edgeId,
    name: "brave-silver-atlas",
    expiresAt: "2099-01-01T01:00:00.000Z",
  });
  expect(deployment.command).toBe(
    `curl -fsSL 'https://pontia.example/install-edge.sh' | sudo sh &&\nsudo pontia-edge init \\\n  --website-origin 'https://pontia.example' \\\n  --edge-id '${edgeId}' \\\n  --ticket '${ticket}' \\\n  --agree-to-lets-encrypt-subscriber-agreement`,
  );
  const stored = await database.db.select().from(edgeTickets).get();
  expect(stored).toMatchObject({
    purpose: "edge_deployment",
    userId: "user-owner",
    expectedEdgeId: edgeId,
    payload: JSON.stringify({ name: "brave-silver-atlas" }),
    expiresAt: "2099-01-01T01:00:00.000Z",
    secretHash: await sha256Base64url(secret),
    consumedAt: null,
  });
  expect(JSON.stringify(stored)).not.toContain(ticket);
});

test("enrollment atomically creates a private owned edge and supports an identical retry", async () => {
  const deployment = await issuedDeployment();
  const ticket = ticketFrom(deployment.command);

  expect(await enrollEdge(database.db, ticket, credential)).toEqual({
    status: "created",
    edge: {
      edgeId,
      name: "brave-silver-atlas",
      tunnelUrl: "wss://brave-silver-atlas.edge.pontia.dev/tunnel",
    },
  });
  expect(await database.db.select().from(edges).get()).toMatchObject({
    id: edgeId,
    userId: "user-owner",
    accessScope: "private",
    name: "brave-silver-atlas",
    tunnelUrl: "wss://brave-silver-atlas.edge.pontia.dev/tunnel",
    serviceCredentialHash: await sha256Base64url(credentialSecret),
  });
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).not.toBeNull();
  expect(await enrollEdge(database.db, ticket, credential)).toEqual({
    status: "existing",
    edge: {
      edgeId,
      name: "brave-silver-atlas",
      tunnelUrl: "wss://brave-silver-atlas.edge.pontia.dev/tunnel",
    },
  });
  expect(await database.db.select().from(edges)).toHaveLength(1);
});

test("concurrent identical enrollment requests create one edge", async () => {
  const deployment = await issuedDeployment();
  const ticket = ticketFrom(deployment.command);

  const results = await Promise.all([
    enrollEdge(database.db, ticket, credential),
    enrollEdge(database.db, ticket, credential),
  ]);

  expect(results.map((result) => result.status).sort()).toEqual(["created", "existing"]);
  expect(await database.db.select().from(edges)).toHaveLength(1);
});

test("enrollment rejects a different credential without changing the consumed result", async () => {
  const deployment = await issuedDeployment();
  const ticket = ticketFrom(deployment.command);
  await enrollEdge(database.db, ticket, credential);
  const different = `pec_v1_${edgeId}_${base64url.encode(new Uint8Array(32).fill(43))}`;

  expect(await enrollEdge(database.db, ticket, different)).toEqual({ status: "invalid" });
  expect((await database.db.select().from(edges).get())?.serviceCredentialHash).toBe(
    await sha256Base64url(credentialSecret),
  );
});

test("wrong edge, purpose, malformed payload, and expired tickets are not consumed", async () => {
  for (const testCase of ["wrong-edge", "purpose", "payload", "expired"]) {
    await database.db.delete(edges);
    await database.db.delete(users);
    const deployment = await issuedDeployment();
    const ticket = ticketFrom(deployment.command);
    const stored = await database.db.select().from(edgeTickets).get();
    if (!stored) throw new Error("Expected ticket");

    let submittedCredential = credential;
    if (testCase === "wrong-edge") {
      submittedCredential = `pec_v1_0199791c-6600-7000-8000-000000000002_${credentialSecret}`;
    } else if (testCase === "purpose") {
      await database.db
        .update(edgeTickets)
        .set({ purpose: "device_tunnel" })
        .where(eq(edgeTickets.id, stored.id));
    } else if (testCase === "payload") {
      await database.db
        .update(edgeTickets)
        .set({ payload: JSON.stringify({ name: "brave-silver-atlas", extra: true }) })
        .where(eq(edgeTickets.id, stored.id));
    } else {
      await database.db
        .update(edgeTickets)
        .set({ expiresAt: "2000-01-01T00:00:00.000Z" })
        .where(eq(edgeTickets.id, stored.id));
    }

    expect(await enrollEdge(database.db, ticket, submittedCredential)).toEqual({
      status: "invalid",
    });
    expect(await database.db.select().from(edges)).toHaveLength(0);
    expect(
      (
        await database.db
          .select({ consumedAt: edgeTickets.consumedAt })
          .from(edgeTickets)
          .where(eq(edgeTickets.id, stored.id))
          .get()
      )?.consumedAt,
    ).toBeNull();
  }
});
