import { afterEach, expect, mock, spyOn, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { base64url } from "jose";
import { sha256Base64url } from "../src/lib/server/crypto";
import { edges, users } from "../src/lib/server/db/schema";
import { CloudflareDnsProvider } from "../src/lib/server/edge-network";
import { issueEdgeDeployment } from "../src/lib/server/edge-deployment";
import { POST } from "../src/routes/api/edge/dns-challenge/+server";
import { manageEdgePreview } from "../src/lib/server/edge-preview";
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
        PREVIEW_ZONE: "p.pontia.dev",
        CLOUDFLARE_PREVIEW_DNS_ZONE_ID: "preview-zone",
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
    name: "Brave Atlas",
    dnsLabel: "brave-atlas",
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
  const deployment = await issueEdgeDeployment(database.db, "owner", {
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

test("upgraded private edge provisions wildcard DNS, confirms TLS and cleans up its preview records", async () => {
  await registered();
  const base = "brave-atlas.p.pontia.dev";
  const records = new Map<
    string,
    { id: string; type: string; name: string; content: string; proxied?: boolean }
  >();
  let healthy = false;
  mockProvider(async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === `__probe.${base}`) {
      expect(url.port).toBe("8443");
      expect(url.pathname).toBe("/__pontia_preview_health");
      expect(init?.redirect).toBe("manual");
      return new Response(healthy ? "ok" : "bad");
    }
    if (url.pathname.includes("/zones/zone/")) {
      expect(url.searchParams.get("name")).toBe(hostname);
      return Response.json({
        success: true,
        result: [{ id: "control", name: hostname, type: "A", content: "8.8.8.8" }],
      });
    }
    expect(url.pathname).toContain("/zones/preview-zone/");
    if (init?.method === "POST") {
      const body = JSON.parse(String(init.body));
      records.set(body.type, { ...body, id: body.type });
    } else if (init?.method === "DELETE") {
      records.delete(url.pathname.split("/").at(-1)!);
    }
    const matched = [...records.values()].filter(
      (r) => r.name === url.searchParams.get("name") && r.type === url.searchParams.get("type"),
    );
    return Response.json({ success: true, result: matched });
  });
  for (let i = 0; i < 2; i++)
    expect((await manageEdgePreview(event({}), "provision")).status).toBe(200);
  expect(records.get("A")).toMatchObject({ name: `*.${base}`, content: "8.8.8.8", proxied: false });
  expect((await call(event({ scope: "preview", operation: "publish", value }))).status).toBe(200);
  expect(records.get("TXT")?.name).toBe(`_acme-challenge.${base}`);
  expect((await manageEdgePreview(event({}), "confirm")).status).toBe(422);
  expect((await database.db.select().from(edges).get())?.previewEnabled).toBe(0);
  healthy = true;
  expect((await manageEdgePreview(event({}), "confirm")).status).toBe(200);
  expect((await database.db.select().from(edges).get())?.previewEnabled).toBe(1);
  await new CloudflareDnsProvider("secret", "preview-zone", "p.pontia.dev").cleanupHostname(base);
  expect(records.size).toBe(0);
});
