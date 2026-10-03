import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { base64url } from "jose";
import { sha256Base64url } from "../src/lib/server/crypto";
import { edgeTickets, edges, users } from "../src/lib/server/db/schema";
import {
  confirmEdgeDeployment,
  decodeDeploymentPayload,
  enrollEdge,
  issueEdgeDeployment,
  type DeploymentDependencies,
} from "../src/lib/server/edge-deployment";
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
    heroName: async () => "brave-atlas",
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

test("deployment payload decoding accepts only one reviewed hero name", async () => {
  expect(await decodeDeploymentPayload(database.db, { name: "brave-atlas" })).toEqual({
    name: "brave-atlas",
  });
  for (const invalid of [
    null,
    [],
    {},
    { name: "custom-name" },
    { name: "brave-atlas", extra: true },
  ]) {
    expect(await decodeDeploymentPayload(database.db, invalid)).toBeNull();
  }
});

test("issuing a deployment creates a one-hour bound ticket and copyable command", async () => {
  const deployment = await issuedDeployment();
  const ticket = ticketFrom(deployment.command);
  const secret = ticket.slice("pet_v1_".length);

  expect(deployment).toMatchObject({
    edgeId,
    name: "brave-atlas",
    expiresAt: "2099-01-01T01:00:00.000Z",
  });
  expect(deployment.command).toContain(
    "curl -fsSL 'https://get.pontia.dev/install-edge.sh' | sudo sh",
  );
  expect(deployment.command).toContain("--cloud-origin 'https://pontia.example'");
  expect(deployment.command).toContain(`--edge-id '${edgeId}'`);
  expect(deployment.command).toContain(`--ticket '${ticket}'`);
  const stored = await database.db.select().from(edgeTickets).get();
  expect(stored).toMatchObject({
    purpose: "edge_deployment",
    userId: "user-owner",
    expectedEdgeId: edgeId,
    payload: JSON.stringify({ name: "brave-atlas" }),
    expiresAt: "2099-01-01T01:00:00.000Z",
    secretHash: await sha256Base64url(secret),
    consumedAt: null,
  });
  expect(JSON.stringify(stored)).not.toContain(ticket);
});

test("enrollment is retryable authorization and does not register or consume the deployment", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  const expected = {
    status: "authorized" as const,
    edge: {
      edgeId,
      name: "brave-atlas",
      tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
    },
  };

  expect(await enrollEdge(database.db, ticket, credential)).toEqual(expected);
  expect(await enrollEdge(database.db, ticket, credential)).toEqual(expected);
  expect(await database.db.select().from(edges)).toHaveLength(0);
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});

test("health failure leaves the deployment unregistered and unconsumed", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);

  expect(await confirmEdgeDeployment(database.db, ticket, credential, async () => false)).toEqual({
    status: "unhealthy",
    edge: {
      edgeId,
      name: "brave-atlas",
      tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
    },
  });
  expect(await database.db.select().from(edges)).toHaveLength(0);
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});

test("successful health verification atomically registers the edge and supports response-loss retry", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  let verifications = 0;

  const result = await confirmEdgeDeployment(database.db, ticket, credential, async (identity) => {
    verifications += 1;
    expect(identity.tunnelUrl).toBe("wss://brave-atlas.edge.pontia.dev/tunnel");
    expect(await database.db.select().from(edges)).toHaveLength(0);
    expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
    return true;
  });
  expect(result.status).toBe("created");
  expect(await database.db.select().from(edges).get()).toMatchObject({
    id: edgeId,
    userId: "user-owner",
    accessScope: "private",
    name: "brave-atlas",
    tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
    serviceCredentialHash: await sha256Base64url(credentialSecret),
  });
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).not.toBeNull();

  expect(
    await confirmEdgeDeployment(database.db, ticket, credential, async () => {
      verifications += 1;
      return false;
    }),
  ).toMatchObject({ status: "existing", edge: { edgeId } });
  expect(verifications).toBe(1);
  expect(await database.db.select().from(edges)).toHaveLength(1);
});

test("only final verified port is persisted and repeated init cannot reconfigure it", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  const tunnelUrl = "wss://brave-atlas.edge.pontia.dev:8443/tunnel";
  expect(
    await confirmEdgeDeployment(
      database.db,
      ticket,
      credential,
      async (identity) => {
        expect(identity.tunnelUrl).toBe(tunnelUrl);
        return false;
      },
      8443,
    ),
  ).toMatchObject({ status: "unhealthy" });
  expect(await database.db.select().from(edges)).toHaveLength(0);
  const result = await confirmEdgeDeployment(
    database.db,
    ticket,
    credential,
    async (identity) => {
      expect(identity.tunnelUrl).toBe(tunnelUrl);
      return true;
    },
    8443,
  );
  expect(result).toMatchObject({ status: "created", edge: { tunnelUrl } });
  expect((await database.db.select().from(edges).get())?.tunnelUrl).toBe(tunnelUrl);
  expect(await enrollEdge(database.db, ticket, credential)).toMatchObject({
    status: "existing",
    edge: { tunnelUrl },
  });
  expect(
    await confirmEdgeDeployment(
      database.db,
      ticket,
      credential,
      async () => {
        throw new Error("completed enrollment must not probe again");
      },
      9443,
    ),
  ).toMatchObject({ status: "existing", edge: { tunnelUrl } });
});

test("concurrent final confirmations deterministically return the same edge", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  let arrivals = 0;
  let release: (() => void) | undefined;
  const bothVerifying = new Promise<void>((resolve) => {
    release = resolve;
  });
  const verify = async () => {
    arrivals += 1;
    if (arrivals === 2) release?.();
    await bothVerifying;
    return true;
  };

  const results = await Promise.all([
    confirmEdgeDeployment(database.db, ticket, credential, verify),
    confirmEdgeDeployment(database.db, ticket, credential, verify),
  ]);

  expect(results.map((result) => result.status).sort()).toEqual(["created", "existing"]);
  expect(await database.db.select().from(edges)).toHaveLength(1);
});

test("a different credential cannot retry or replace a completed deployment", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  await confirmEdgeDeployment(database.db, ticket, credential, async () => true);
  const different = `pec_v1_${edgeId}_${base64url.encode(new Uint8Array(32).fill(43))}`;

  expect(await enrollEdge(database.db, ticket, different)).toEqual({ status: "invalid" });
  expect(await confirmEdgeDeployment(database.db, ticket, different, async () => true)).toEqual({
    status: "invalid",
  });
  expect((await database.db.select().from(edges).get())?.serviceCredentialHash).toBe(
    await sha256Base64url(credentialSecret),
  );
});

test("wrong edge, purpose, malformed payload, and expired tickets never register", async () => {
  for (const testCase of ["wrong-edge", "purpose", "payload", "expired"]) {
    await database.db.delete(edges);
    await database.db.delete(users);
    const ticket = ticketFrom((await issuedDeployment()).command);
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
        .set({ payload: JSON.stringify({ name: "brave-atlas", extra: true }) })
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
    expect(
      await confirmEdgeDeployment(database.db, ticket, submittedCredential, async () => true),
    ).toEqual({ status: "invalid" });
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

test("a failed registration batch does not consume the deployment ticket", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  await database.db.insert(edges).values({
    id: "0199791c-6600-7000-8000-000000000002",
    userId: "user-owner",
    name: "conflicting-edge",
    tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
    serviceCredentialHash: "unrelated",
  });

  expect(await confirmEdgeDeployment(database.db, ticket, credential, async () => true)).toEqual({
    status: "invalid",
  });
  expect(await database.db.select().from(edges)).toHaveLength(1);
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});

test("expired deployment records reserve their DNS names until cron deletes them", async () => {
  await issuedDeployment();
  await database.db
    .update(edgeTickets)
    .set({ expiresAt: "2000-01-01T00:00:00.000Z" })
    .where(eq(edgeTickets.purpose, "edge_deployment"));

  await expect(
    issueEdgeDeployment(database.db, "user-owner", "https://pontia.example", dependencies()),
  ).rejects.toThrow("Unable to allocate an edge name");
});
