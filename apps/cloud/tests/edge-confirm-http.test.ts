import { afterEach, beforeAll, expect, mock, spyOn, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { base64url, decodeJwt, generateKeyPair } from "jose";
import { edges, edgeTickets, users } from "../src/lib/server/db/schema";
import { issueEdgeDeployment } from "../src/lib/server/edge-deployment";
import { POST as confirm } from "../src/routes/api/edge/confirm/+server";
import { testDatabase } from "./database";

const database = testDatabase();
const call = confirm as unknown as (event: RequestEvent) => Promise<Response>;
const edgeId = "0199791c-6600-7000-8000-000000000001";
const hostname = "edge.example.com";
const secretBytes = new Uint8Array(32).fill(71);
const credential = `pec_v1_${edgeId}_${base64url.encode(secretBytes)}`;
let signingKey = "";

beforeAll(async () => {
  const { privateKey } = await generateKeyPair("EdDSA", { extractable: true });
  signingKey = base64url.encode(new Uint8Array(await crypto.subtle.exportKey("pkcs8", privateKey)));
});
afterEach(() => mock.restore());

async function deploymentTicket(host = hostname) {
  await database.db.insert(users).values({ id: "network-owner" });
  const result = await issueEdgeDeployment(database.db, "network-owner", host, {
    now: () => new Date("2099-01-01T00:00:00.000Z"),
    randomBytes: () => new Uint8Array(32).fill(72),
    edgeId: () => edgeId,
  });
  if (result.status !== "issued") throw new Error("Expected deployment");
  return /--ticket '(pet_v1_[A-Za-z0-9_-]{43})'/.exec(result.deployment.command)![1];
}

function event(body: unknown, limits: string[] = [], allowed = true) {
  const url = new URL("https://pontia.example/api/edge/confirm");
  return {
    url,
    request: new Request(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
    platform: {
      env: {
        DB: database.binding,
        E2E_CAPABILITY_SIGNING_KEY: signingKey,
        EDGE_CONFIRM_RATE_LIMIT: {
          async limit({ key }: { key: string }) {
            limits.push(key);
            return { success: allowed };
          },
        },
      },
    },
  } as unknown as RequestEvent;
}

/** An independent implementation of the edge's HMAC proof. */
async function edgeProof(token: string) {
  const claims = decodeJwt(token) as { aud: string; jti: string; host: string; port: number };
  const encoder = new TextEncoder();
  const label = encoder.encode("pontia-edge-ownership-key-v1\0");
  const keyInput = new Uint8Array(label.length + 32);
  keyInput.set(label);
  keyInput.set(secretBytes, label.length);
  const key = await crypto.subtle.importKey(
    "raw",
    await crypto.subtle.digest("SHA-256", keyInput),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const message = `pontia-edge-ownership-v1\0${claims.aud}\0${claims.host}\0${claims.port}\0${claims.jti}`;
  return base64url.encode(
    new Uint8Array(await crypto.subtle.sign("HMAC", key, encoder.encode(message))),
  );
}

type Edge = (url: URL, init: RequestInit) => Response | Promise<Response>;

function network(addresses: { A?: string[]; AAAA?: string[] }, edge: Edge) {
  const requests: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation((async (input: string, init: RequestInit) => {
    const url = new URL(String(input));
    requests.push(url.origin + url.pathname);
    if (url.hostname === "cloudflare-dns.com") {
      const type = url.searchParams.get("type") as "A" | "AAAA";
      expect(url.searchParams.get("name")).toBe(hostname);
      return Response.json({
        Status: 0,
        Answer: (addresses[type] ?? []).map((data) => ({ type: type === "A" ? 1 : 28, data })),
      });
    }
    return edge(url, init);
  }) as typeof fetch);
  return requests;
}

const genuineEdge: Edge = async (url, init) => {
  expect(url.pathname).toBe("/__pontia/verify-ownership");
  expect(init.redirect).toBe("manual");
  const token = new Headers(init.headers).get("authorization")!.slice("Bearer ".length);
  return new Response(await edgeProof(token), { headers: { "Content-Type": "text/plain" } });
};

test("rejects malformed confirmation requests", async () => {
  for (const body of [
    null,
    [],
    { ticket: "t", service_credential: credential },
    { ticket: "t", service_credential: credential, port: 25 },
    { ticket: "t", service_credential: credential, port: 443, candidate_ipv4: "8.8.8.8" },
  ]) {
    const response = await call(event(body));
    expect(response.status).toBe(400);
  }
  expect(
    (await call(event({ ticket: "t", service_credential: credential, port: 443 }))).status,
  ).toBe(401);
});

test("a verified ownership proof registers the edge on its own domain", async () => {
  const ticket = await deploymentTicket();
  const limits: string[] = [];
  const requests = network({ A: ["8.8.8.8"], AAAA: ["2606:4700:4700::1111"] }, genuineEdge);

  const response = await call(
    event({ ticket, service_credential: credential, port: 8443 }, limits),
  );
  expect(response.status).toBe(200);
  expect((await response.json()) as unknown).toEqual({
    edge_id: edgeId,
    hostname,
    tunnel_url: `wss://${hostname}:8443/tunnel`,
  });
  expect(limits).toEqual([edgeId]);
  expect(requests).toContain(`https://${hostname}:8443/__pontia/verify-ownership`);
  expect(await database.db.select().from(edges).get()).toMatchObject({
    id: edgeId,
    name: hostname,
    dnsLabel: null,
    previewEnabled: 0,
    accessScope: "private",
    tunnelUrl: `wss://${hostname}:8443/tunnel`,
  });

  // A lost response is retried without probing again.
  requests.length = 0;
  const retry = await call(event({ ticket, service_credential: credential, port: 8443 }, limits));
  expect(retry.status).toBe(200);
  expect(requests).toEqual([]);
  expect(limits).toEqual([edgeId]);
});

test("a public IPv6 address skips DNS and registers with a bracketed authority", async () => {
  const address = "2606:4700:4700::1111";
  const ticket = await deploymentTicket(address);
  const requests: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation((async (input: string, init: RequestInit) => {
    const url = new URL(String(input));
    requests.push(url.toString());
    expect(url.hostname).toBe(`[${address}]`);
    return genuineEdge(url, init);
  }) as typeof fetch);

  const response = await call(event({ ticket, service_credential: credential, port: 8443 }));

  expect(response.status).toBe(200);
  expect((await response.json()) as unknown).toEqual({
    edge_id: edgeId,
    hostname: address,
    tunnel_url: `wss://[${address}]:8443/tunnel`,
  });
  expect(requests).toEqual([`https://[${address}]:8443/__pontia/verify-ownership`]);
});

test("rate limiting stops confirmation before any outbound request", async () => {
  const ticket = await deploymentTicket();
  const requests = network({ A: ["8.8.8.8"] }, genuineEdge);
  const response = await call(
    event({ ticket, service_credential: credential, port: 443 }, [], false),
  );
  expect(response.status).toBe(429);
  expect((await response.json()) as unknown).toEqual({ error: "rate_limited" });
  expect(requests).toEqual([]);
});

test.each([
  ["no records", {}],
  ["a private IPv4 address", { A: ["10.0.0.5"] }],
  ["a loopback IPv6 address", { A: ["8.8.8.8"], AAAA: ["::1"] }],
  ["a link-local address", { A: ["169.254.169.254"] }],
])("DNS resolving to %s is rejected before contacting the endpoint", async (_case, addresses) => {
  const ticket = await deploymentTicket();
  const requests = network(addresses, () => {
    throw new Error("must not contact a non-public endpoint");
  });
  const response = await call(event({ ticket, service_credential: credential, port: 443 }));
  expect(response.status).toBe(422);
  expect((await response.json()) as unknown).toEqual({ error: "non_public_ip" });
  expect(requests.every((url) => url.startsWith("https://cloudflare-dns.com/"))).toBeTrue();
  expect(await database.db.select().from(edges)).toHaveLength(0);
});

test.each([
  ["a site that only answers health checks", () => new Response("ok")],
  ["a wrong HMAC", () => new Response(base64url.encode(new Uint8Array(32)))],
  [
    "a redirect to an internal service",
    () => new Response(null, { status: 302, headers: { Location: "http://169.254.169.254/" } }),
  ],
  [
    "an unreachable or untrusted endpoint",
    () => {
      throw new TypeError("certificate verification failed");
    },
  ],
])("%s fails ownership verification", async (_case, edge) => {
  const ticket = await deploymentTicket();
  network({ A: ["8.8.8.8"] }, edge);
  const response = await call(event({ ticket, service_credential: credential, port: 443 }));
  expect(response.status).toBe(422);
  expect((await response.json()) as unknown).toEqual({ error: "ownership_verification_failed" });
  expect(await database.db.select().from(edges)).toHaveLength(0);
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});

test("another edge's endpoint cannot be registered twice", async () => {
  const ticket = await deploymentTicket();
  await database.db.insert(edges).values({
    id: "0199791c-6600-7000-8000-000000000002",
    userId: "network-owner",
    name: "Existing",
    tunnelUrl: `wss://${hostname}/tunnel`,
    serviceCredentialHash: "other",
  });
  const requests = network({ A: ["8.8.8.8"] }, genuineEdge);
  const response = await call(event({ ticket, service_credential: credential, port: 443 }));
  expect(response.status).toBe(409);
  expect((await response.json()) as unknown).toEqual({ error: "endpoint_already_registered" });
  expect(requests).toEqual([]);
});
