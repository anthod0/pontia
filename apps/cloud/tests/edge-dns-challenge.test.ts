import { afterEach, expect, mock, spyOn, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { base64url } from "jose";
import { sha256Base64url } from "../src/lib/server/crypto";
import { edges, users } from "../src/lib/server/db/schema";
import { CloudflareDnsProvider } from "../src/lib/server/edge-network";
import { issueEdgeDeployment } from "../src/lib/server/edge-deployment";
import { POST } from "../src/routes/api/edge/dns-challenge/+server";
import { testDatabase } from "./database";
import { mockProvider } from "./provider";

const database = testDatabase();
const call = POST as unknown as (event: RequestEvent) => Promise<Response>;
const edgeId = "0199791c-6600-7000-8000-000000000001";
const secret = base64url.encode(new Uint8Array(32).fill(31));
const credential = `pec_v1_${edgeId}_${secret}`;
const hostname = "brave-atlas.edge.pontia.dev";
const value = "x".repeat(43);
afterEach(() => mock.restore());

function event(body: unknown, token = credential, allowed = true, keys: string[] = []) {
  const url = new URL("https://pontia.example/api/edge/dns-challenge");
  return {
    url,
    request: new Request(url, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
    platform: {
      env: {
        DB: database.binding,
        CLOUDFLARE_DNS_TOKEN: "provider-secret",
        CLOUDFLARE_DNS_ZONE_ID: "zone",
        EDGE_DNS_RATE_LIMIT: {
          async limit({ key }: { key: string }) {
            keys.push(key);
            return { success: allowed };
          },
        },
      },
    },
  } as unknown as RequestEvent;
}

async function registered() {
  await database.db.insert(users).values({ id: "owner" });
  await database.db.insert(edges).values({
    id: edgeId,
    userId: "owner",
    accessScope: "private",
    name: "brave-atlas",
    tunnelUrl: `wss://${hostname}:8443/tunnel`,
    serviceCredentialHash: await sha256Base64url(secret),
  });
}

test("TXT publication retries reuse value and old cleanup cannot delete newer values", async () => {
  const records = [
    { id: "newer", type: "TXT", name: `_acme-challenge.${hostname}`, content: "y".repeat(43) },
  ];
  const writes: string[] = [];
  mockProvider(async (input, init) => {
    const url = String(input);
    if (init?.method === "POST") {
      records.push({ id: "current", ...JSON.parse(String(init.body)) });
      writes.push("create");
    } else if (init?.method === "DELETE") {
      const id = url.split("/").at(-1);
      const index = records.findIndex((record) => record.id === id);
      if (index !== -1) records.splice(index, 1);
      writes.push(`delete:${id}`);
    }
    return Response.json({ success: true, result: records });
  });
  const dns = new CloudflareDnsProvider("secret", "zone");
  await dns.publishTxt(hostname, value);
  await dns.publishTxt(hostname, value);
  await dns.cleanupTxt(hostname, value);
  await dns.cleanupTxt(hostname, value);
  expect(writes).toEqual(["create", "delete:current"]);
  expect(records.map((record) => record.id)).toEqual(["newer"]);
  await expect(dns.publishTxt("attacker.example", value)).rejects.toThrow();
});

test("renewal authenticates before edge-ID rate limiting and derives its own TXT hostname", async () => {
  await registered();
  const keys: string[] = [];
  const body = { operation: "publish", value };
  expect((await call(event(body, "invalid", true, keys))).status).toBe(401);
  expect(keys).toEqual([]);
  expect((await call(event(body, credential, false, keys))).status).toBe(429);
  expect(keys).toEqual([edgeId]);
  expect((await call(event({ ...body, hostname: "attacker.example" }))).status).toBe(400);
  const requests: string[] = [];
  mockProvider(async (input, init) => {
    requests.push(init?.method === "POST" ? String(init.body) : String(input));
    return Response.json({ success: true, result: [] });
  });
  const log = spyOn(console, "info").mockImplementation(() => undefined);
  expect((await call(event(body))).status).toBe(200);
  expect(JSON.parse(requests[1])).toMatchObject({
    name: `_acme-challenge.${hostname}`,
    content: JSON.stringify(value),
  });
  expect(JSON.stringify(log.mock.calls)).not.toContain(credential);
  expect(JSON.stringify(log.mock.calls)).not.toContain("provider-secret");
  expect((await call(event({ operation: "cleanup", value }))).status).toBe(200);
});

test("pending deployment can clean its own TXT but cannot bypass network verification to publish", async () => {
  await database.db.insert(users).values({ id: "owner" });
  const deployment = await issueEdgeDeployment(database.db, "owner", "https://pontia.example", {
    now: () => new Date("2099-01-01"),
    randomBytes: () => new Uint8Array(32).fill(32),
    edgeId: () => edgeId,
    heroName: async () => "brave-atlas",
  });
  const ticket = /--ticket '(pet_v1_[A-Za-z0-9_-]{43})'/.exec(deployment.command)![1];
  mockProvider(async () => Response.json({ success: true, result: [] }));
  expect((await call(event({ operation: "publish", value, ticket }))).status).toBe(401);
  expect((await call(event({ operation: "cleanup", value, ticket }))).status).toBe(200);
  expect(
    (
      await call(
        event(
          { operation: "cleanup", value, ticket },
          credential.replace(edgeId, "0199791c-6600-7000-8000-000000000002"),
        ),
      )
    ).status,
  ).toBe(401);
});
