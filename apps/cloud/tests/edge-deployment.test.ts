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
  type OwnershipVerification,
} from "../src/lib/server/edge-deployment";
import { testDatabase } from "./database";

const database = testDatabase();
const edgeId = "0199791c-6600-7000-8000-000000000001";
const hostname = "edge.example.com";
const now = new Date("2099-01-01T00:00:00.000Z");
const ticketBytes = new Uint8Array(32).fill(41);
const credentialSecret = base64url.encode(new Uint8Array(32).fill(42));
const credential = `pec_v1_${edgeId}_${credentialSecret}`;
const verified = async (): Promise<OwnershipVerification> => "verified";

function dependencies(): DeploymentDependencies {
  return {
    now: () => now,
    randomBytes: () => ticketBytes.slice(),
    edgeId: () => edgeId,
  };
}

async function issuedDeployment(name = hostname) {
  await database.db.insert(users).values({ id: "user-owner" }).onConflictDoNothing();
  const result = await issueEdgeDeployment(database.db, "user-owner", name, dependencies());
  if (result.status !== "issued") throw new Error(`Expected a deployment, got ${result.status}`);
  return result.deployment;
}

function ticketFrom(command: string) {
  const match = /--ticket '(pet_v1_[A-Za-z0-9_-]{43})'/.exec(command);
  if (!match) throw new Error("Expected deployment ticket in command");
  return match[1];
}

test("deployment payload decoding accepts only one public hostname", () => {
  for (const name of [hostname, "sg.edge.pontia.dev", "relay.pontia.run"]) {
    expect(decodeDeploymentPayload({ hostname: name })).toEqual({ hostname: name });
  }
  for (const invalid of [
    null,
    [],
    {},
    { name: "brave-atlas" },
    { hostname: "localhost" },
    { hostname: "10.0.0.1" },
    { hostname: "Edge.Example.com" },
    { hostname, extra: true },
  ]) {
    expect(decodeDeploymentPayload(invalid)).toBeNull();
  }
});

test("issuing a deployment binds the hostname to a one-hour ticket and copyable command", async () => {
  const deployment = await issuedDeployment();
  const ticket = ticketFrom(deployment.command);
  const secret = ticket.slice("pet_v1_".length);

  expect(deployment).toMatchObject({
    edgeId,
    hostname,
    expiresAt: "2099-01-01T01:00:00.000Z",
  });
  expect(deployment.command).toBe(
    `curl -fsSL 'https://get.pontia.dev/install-edge.sh' | sudo sh &&\nsudo pontia-edge init \\\n  --edge-id '${edgeId}' \\\n  --ticket '${ticket}'`,
  );
  const stored = await database.db.select().from(edgeTickets).get();
  expect(stored).toMatchObject({
    purpose: "edge_deployment",
    userId: "user-owner",
    expectedEdgeId: edgeId,
    payload: JSON.stringify({ hostname }),
    expiresAt: "2099-01-01T01:00:00.000Z",
    secretHash: await sha256Base64url(secret),
    consumedAt: null,
  });
  expect(JSON.stringify(stored)).not.toContain(ticket);
});

test("issuing rejects invalid hostnames and hostnames already registered on any port", async () => {
  await database.db.insert(users).values({ id: "user-owner" });
  for (const invalid of ["", "localhost", "edge.local", "8.8.8.8", "Edge.Example.com"]) {
    expect(await issueEdgeDeployment(database.db, "user-owner", invalid, dependencies())).toEqual({
      status: "invalid",
    });
  }
  await database.db.insert(edges).values({
    id: "0199791c-6600-7000-8000-000000000002",
    userId: "user-owner",
    name: "Home server",
    tunnelUrl: `wss://${hostname}:8443/tunnel`,
    serviceCredentialHash: "unrelated",
  });
  expect(await issueEdgeDeployment(database.db, "user-owner", hostname, dependencies())).toEqual({
    status: "conflict",
  });
  expect(
    (await issueEdgeDeployment(database.db, "user-owner", `other.${hostname}`, dependencies()))
      .status,
  ).toBe("issued");
  expect(await database.db.select().from(edgeTickets)).toHaveLength(1);
});

test("enrollment is retryable authorization and does not register or consume the deployment", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  const expected = {
    status: "authorized" as const,
    edge: { edgeId, name: hostname, tunnelUrl: `wss://${hostname}/tunnel` },
  };

  expect(await enrollEdge(database.db, ticket, credential)).toEqual(expected);
  expect(await enrollEdge(database.db, ticket, credential)).toEqual(expected);
  expect(await database.db.select().from(edges)).toHaveLength(0);
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});

test("failed ownership checks leave the deployment unregistered and unconsumed", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);

  for (const status of ["failed", "non_public_ip", "rate_limited"] as const) {
    expect(
      await confirmEdgeDeployment(database.db, ticket, credential, async () => status),
    ).toEqual({
      status,
      edge: { edgeId, name: hostname, tunnelUrl: `wss://${hostname}/tunnel` },
    });
  }
  expect(await database.db.select().from(edges)).toHaveLength(0);
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});

test("verified ownership registers an unlabeled private edge and supports response-loss retry", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  let verifications = 0;

  const result = await confirmEdgeDeployment(database.db, ticket, credential, async (candidate) => {
    verifications += 1;
    expect(candidate).toEqual({ edgeId, hostname, port: 443, credentialSecret });
    expect(await database.db.select().from(edges)).toHaveLength(0);
    return "verified";
  });
  expect(result).toEqual({
    status: "created",
    edge: { edgeId, name: hostname, tunnelUrl: `wss://${hostname}/tunnel` },
  });
  expect(await database.db.select().from(edges).get()).toMatchObject({
    id: edgeId,
    userId: "user-owner",
    accessScope: "private",
    name: hostname,
    dnsLabel: null,
    previewEnabled: 0,
    tunnelUrl: `wss://${hostname}/tunnel`,
    serviceCredentialHash: await sha256Base64url(credentialSecret),
  });
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).not.toBeNull();

  expect(
    await confirmEdgeDeployment(database.db, ticket, credential, async () => {
      verifications += 1;
      return "failed";
    }),
  ).toMatchObject({ status: "existing", edge: { edgeId } });
  expect(verifications).toBe(1);
  expect(await enrollEdge(database.db, ticket, credential)).toMatchObject({
    status: "existing",
    edge: { tunnelUrl: `wss://${hostname}/tunnel` },
  });
});

test("the verified port is persisted and a completed deployment cannot change it", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  const tunnelUrl = `wss://${hostname}:8443/tunnel`;
  const result = await confirmEdgeDeployment(
    database.db,
    ticket,
    credential,
    async (candidate) => {
      expect(candidate.port).toBe(8443);
      return "verified";
    },
    8443,
  );
  expect(result).toMatchObject({ status: "created", edge: { tunnelUrl } });
  expect(
    await confirmEdgeDeployment(database.db, ticket, credential, verified, 8443),
  ).toMatchObject({ status: "existing", edge: { tunnelUrl } });
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
  ).toEqual({ status: "invalid" });
  expect(await confirmEdgeDeployment(database.db, ticket, credential, verified, 25)).toEqual({
    status: "invalid",
  });
});

test("an endpoint registered by another edge is a conflict without probing", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  await database.db.insert(edges).values({
    id: "0199791c-6600-7000-8000-000000000002",
    userId: "user-owner",
    name: "Other",
    tunnelUrl: `wss://${hostname}/tunnel`,
    serviceCredentialHash: "unrelated",
  });

  expect(
    await confirmEdgeDeployment(database.db, ticket, credential, async () => {
      throw new Error("conflicting endpoints must not be probed");
    }),
  ).toEqual({ status: "conflict" });
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});

test("concurrent final confirmations deterministically return the same edge", async () => {
  const ticket = ticketFrom((await issuedDeployment()).command);
  let arrivals = 0;
  let release: (() => void) | undefined;
  const bothVerifying = new Promise<void>((resolve) => {
    release = resolve;
  });
  const verify = async (): Promise<OwnershipVerification> => {
    arrivals += 1;
    if (arrivals === 2) release?.();
    await bothVerifying;
    return "verified";
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
  await confirmEdgeDeployment(database.db, ticket, credential, verified);
  const different = `pec_v1_${edgeId}_${base64url.encode(new Uint8Array(32).fill(43))}`;

  expect(await enrollEdge(database.db, ticket, different)).toEqual({ status: "invalid" });
  expect(await confirmEdgeDeployment(database.db, ticket, different, verified)).toEqual({
    status: "invalid",
  });
  expect((await database.db.select().from(edges).get())?.serviceCredentialHash).toBe(
    await sha256Base64url(credentialSecret),
  );
});

test("wrong edge, purpose, malformed payload, and expired tickets never register", async () => {
  for (const testCase of ["wrong-edge", "purpose", "payload", "legacy-payload", "expired"]) {
    await database.db.delete(edges);
    await database.db.delete(edgeTickets);
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
    } else if (testCase === "payload" || testCase === "legacy-payload") {
      await database.db
        .update(edgeTickets)
        .set({
          payload: JSON.stringify(
            testCase === "payload" ? { hostname, extra: true } : { name: "brave-atlas" },
          ),
        })
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
    expect(await confirmEdgeDeployment(database.db, ticket, submittedCredential, verified)).toEqual(
      { status: "invalid" },
    );
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

test("unlabeled edges cannot enable previews", async () => {
  await database.db.insert(users).values({ id: "user-owner" });
  const previewWithoutLabel = async () => {
    await database.db.insert(edges).values({
      id: edgeId,
      userId: "user-owner",
      name: hostname,
      tunnelUrl: `wss://${hostname}/tunnel`,
      serviceCredentialHash: "unrelated",
      previewEnabled: 1,
    });
  };
  await expect(previewWithoutLabel()).rejects.toThrow();
  await database.db.insert(edges).values([
    {
      id: edgeId,
      userId: "user-owner",
      name: hostname,
      tunnelUrl: `wss://${hostname}/tunnel`,
      serviceCredentialHash: "first",
    },
    {
      id: "0199791c-6600-7000-8000-000000000002",
      userId: "user-owner",
      name: "Second",
      tunnelUrl: `wss://second.example.com/tunnel`,
      serviceCredentialHash: "second",
    },
  ]);
  expect(await database.db.select().from(edges)).toHaveLength(2);
});
