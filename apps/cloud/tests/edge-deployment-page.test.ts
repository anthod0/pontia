import { afterEach, expect, mock, spyOn, test } from "bun:test";
import { base64url, generateKeyPair } from "jose";
import type { RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { issueLogin } from "../src/lib/server/auth/jwt";
import { authSessions, edges, users } from "../src/lib/server/db/schema";
import { actions as edgeActions, load } from "../src/routes/settings/edges/+page.server";
import { GET as getEdgeHealth } from "../src/routes/settings/edges/health/+server";
import {
  actions as deploymentActions,
  load as loadDeployment,
} from "../src/routes/settings/edges/deploy/+page.server";
import { testDatabase } from "./database";

const issueDeployment = deploymentActions.default as (event: RequestEvent) => Promise<unknown>;
const renameEdge = edgeActions.rename as (event: RequestEvent) => Promise<unknown>;
const database = testDatabase();
const loadEdges = load as unknown as (
  event: RequestEvent,
) => Promise<{ ownedEdges: unknown[]; publicEdges: unknown[] }>;
const ownerId = "0199791c-6600-7000-8000-000000000011";
const loadEdgeHealth = getEdgeHealth as unknown as (event: RequestEvent) => Promise<Response>;
const { privateKey: edgeHealthPrivateKey } = await generateKeyPair("EdDSA", {
  extractable: true,
});
const edgeHealthSigningKey = base64url.encode(
  new Uint8Array(await crypto.subtle.exportKey("pkcs8", edgeHealthPrivateKey)),
);

afterEach(() => mock.restore());

const healthLimits: string[] = [];
let healthAllowed = true;

async function authenticatedEvent(userId: string): Promise<RequestEvent> {
  const secret = "test-signing-secret-with-at-least-32-bytes";
  const loginId = "0199791c-6600-7000-8000-000000000010";
  await database.db.insert(users).values({ id: userId });
  await database.db.insert(authSessions).values({
    id: loginId,
    userId,
    expiresAt: "2099-01-01T00:00:00.000Z",
  });
  const login = await issueLogin(database.db, loginId, secret);
  return {
    cookies: { get: (name: string) => (name === "__Host-_at" ? login.token : undefined) },
    platform: {
      env: {
        AUTH_ORIGIN: "https://pontia.example",
        DB: database.binding,
        JWT_SECRET: secret,
        E2E_CAPABILITY_SIGNING_KEY: edgeHealthSigningKey,
        EDGE_HEALTH_RATE_LIMIT: {
          async limit({ key }: { key: string }) {
            healthLimits.push(key);
            return { success: healthAllowed };
          },
        },
      },
    },
    url: new URL("https://pontia.example/settings/edges"),
  } as unknown as RequestEvent;
}

function deploymentRequest(event: RequestEvent, hostname: string) {
  event.request = new Request("https://pontia.example/settings/edges/deploy", {
    method: "POST",
    body: new URLSearchParams({ hostname }),
  });
  return event;
}

test("the edge list requires sign-in", async () => {
  await expect(
    loadEdges({
      cookies: { get: () => undefined },
    } as unknown as RequestEvent),
  ).rejects.toMatchObject({ status: 303, location: "/login" });
});

test("the deployment page requires sign-in", async () => {
  const deploymentPage = loadDeployment as unknown as (event: RequestEvent) => Promise<unknown>;
  await expect(
    deploymentPage({ cookies: { get: () => undefined } } as unknown as RequestEvent),
  ).rejects.toMatchObject({ status: 303, location: "/login" });
});

test("a user without edges receives empty owned and public lists", async () => {
  const event = await authenticatedEvent(ownerId);
  const result = await loadEdges(event);
  expect(result.ownedEdges).toEqual([]);
  expect(result.publicEdges).toEqual([]);
});

test("the page includes owned public edges in both lists in stable order without credentials", async () => {
  const event = await authenticatedEvent(ownerId);
  await database.db.insert(users).values({ id: "user-other" });
  const createdAt = "2026-01-01T00:00:00.000Z";
  await database.db.insert(edges).values(
    [
      { id: "edge-z", userId: ownerId, name: "Tokyo" },
      { id: "edge-b", userId: ownerId, name: "Singapore" },
      { id: "edge-a", userId: ownerId, name: "Singapore" },
      { id: "edge-owned-public", userId: ownerId, name: "Shared", accessScope: "public" as const },
      { id: "edge-private", userId: "user-other", name: "Private" },
      { id: "edge-public", userId: "user-other", name: "Public", accessScope: "public" as const },
    ].map((edge) => ({
      ...edge,
      tunnelUrl: `wss://${edge.id}.example/tunnel`,
      serviceCredentialHash: "secret-hash",
      createdAt,
    })),
  );
  const result = await loadEdges(event);
  expect(result.ownedEdges).toEqual([
    {
      id: "edge-owned-public",
      name: "Shared",
      tunnelUrl: "wss://edge-owned-public.example/tunnel",
      createdAt,
    },
    {
      id: "edge-a",
      name: "Singapore",
      tunnelUrl: "wss://edge-a.example/tunnel",
      createdAt,
    },
    {
      id: "edge-b",
      name: "Singapore",
      tunnelUrl: "wss://edge-b.example/tunnel",
      createdAt,
    },
    {
      id: "edge-z",
      name: "Tokyo",
      tunnelUrl: "wss://edge-z.example/tunnel",
      createdAt,
    },
  ]);
  expect(result.publicEdges).toEqual([
    {
      id: "edge-public",
      name: "Public",
      tunnelUrl: "wss://edge-public.example/tunnel",
      createdAt,
    },
    {
      id: "edge-owned-public",
      name: "Shared",
      tunnelUrl: "wss://edge-owned-public.example/tunnel",
      createdAt,
    },
  ]);
});

test("the health endpoint checks only edges visible to the signed-in user", async () => {
  const event = await authenticatedEvent(ownerId);
  await database.db.insert(users).values({ id: "user-other" });
  await database.db.insert(edges).values([
    {
      id: "edge-owned",
      userId: ownerId,
      name: "Owned",
      tunnelUrl: "wss://owned-edge.edge.pontia.dev/tunnel",
      serviceCredentialHash: "secret-hash",
    },
    {
      id: "edge-public",
      userId: "user-other",
      name: "Public",
      tunnelUrl: "wss://public-edge.edge.pontia.dev/tunnel",
      serviceCredentialHash: "secret-hash",
      accessScope: "public",
    },
    {
      id: "edge-private",
      userId: "user-other",
      name: "Private",
      tunnelUrl: "wss://private-edge.edge.pontia.dev/tunnel",
      serviceCredentialHash: "secret-hash",
    },
    {
      id: "edge-custom",
      userId: ownerId,
      name: "relay.example.com",
      tunnelUrl: "wss://relay.example.com:8443/tunnel",
      serviceCredentialHash: "secret-hash",
    },
  ]);
  const requested: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation((async (input) => {
    const url = String(input);
    requested.push(url);
    return url.includes("owned-edge") || url.includes("relay.example.com")
      ? new Response("ok", { headers: { "Pontia-Edge-Version": "1.2.3-rc.1+build" } })
      : new Response("unavailable", { status: 503 });
  }) as typeof fetch);

  healthLimits.length = 0;
  const response = await loadEdgeHealth(event);

  expect(response.status).toBe(200);
  expect((await response.json()) as unknown).toEqual({
    edges: {
      "edge-owned": { status: "healthy", version: "1.2.3-rc.1+build" },
      "edge-custom": { status: "healthy", version: "1.2.3-rc.1+build" },
      "edge-public": { status: "unreachable" },
    },
  });
  expect(requested.sort()).toEqual([
    "https://owned-edge.edge.pontia.dev/__pontia/version",
    "https://public-edge.edge.pontia.dev/__pontia/version",
    "https://relay.example.com:8443/__pontia/version",
  ]);
  expect(healthLimits).toEqual([ownerId]);
});

test("the health endpoint is rate limited per user before probing", async () => {
  const event = await authenticatedEvent(ownerId);
  spyOn(globalThis, "fetch").mockImplementation((async () => {
    throw new Error("rate-limited checks must not probe edges");
  }) as unknown as typeof fetch);
  healthAllowed = false;
  try {
    const response = await loadEdgeHealth(event);
    expect(response.status).toBe(429);
    expect((await response.json()) as unknown).toEqual({ error: "rate_limited" });
  } finally {
    healthAllowed = true;
  }
});

test("the health endpoint requires sign-in", async () => {
  const response = await loadEdgeHealth({
    cookies: { get: () => undefined },
  } as unknown as RequestEvent);

  expect(response.status).toBe(401);
});

test("an owner can rename an edge without changing its tunnel URL", async () => {
  const event = await authenticatedEvent(ownerId);
  const edgeId = "edge-owned";
  await database.db.insert(edges).values({
    id: edgeId,
    userId: ownerId,
    name: "brave-atlas",
    tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
    serviceCredentialHash: "secret-hash",
  });
  event.request = new Request(event.url, {
    method: "POST",
    body: new URLSearchParams({ edge_id: edgeId, name: "  Home server  " }),
  });

  expect(await renameEdge(event)).toEqual({ success: "edge_renamed" });
  const longName = `${"a".repeat(63)}.${"b".repeat(63)}.${"c".repeat(63)}.${"d".repeat(61)}`;
  event.request = new Request(event.url, {
    method: "POST",
    body: new URLSearchParams({ edge_id: edgeId, name: longName }),
  });
  expect(await renameEdge(event)).toEqual({ success: "edge_renamed" });
  event.request = new Request(event.url, {
    method: "POST",
    body: new URLSearchParams({ edge_id: edgeId, name: `${longName}x` }),
  });
  expect(await renameEdge(event)).toMatchObject({ status: 400 });
  event.request = new Request(event.url, {
    method: "POST",
    body: new URLSearchParams({ edge_id: edgeId, name: "  Home server  " }),
  });
  await renameEdge(event);
  expect(
    await database.db
      .select({ name: edges.name, tunnelUrl: edges.tunnelUrl })
      .from(edges)
      .where(eq(edges.id, edgeId))
      .get(),
  ).toEqual({
    name: "Home server",
    tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
  });
});

test("an unauthenticated browser cannot issue an edge deployment", async () => {
  const result = (await issueDeployment({
    cookies: { get: () => undefined },
  } as unknown as RequestEvent)) as { status: number; data: { error: string } };

  expect(result.status).toBe(401);
  expect(result.data).toEqual({ error: "Sign in to create an edge deployment." });
});

test("an issued access JWT remains usable until it expires", async () => {
  const userId = "0199791c-6600-7000-8000-000000000001";
  const loginId = "0199791c-6600-7000-8000-000000000002";
  const secret = "test-signing-secret-with-at-least-32-bytes";
  await database.db.insert(users).values({ id: userId });
  await database.db.insert(authSessions).values({
    id: loginId,
    userId,
    expiresAt: "2099-01-01T00:00:00.000Z",
  });
  const login = await issueLogin(database.db, loginId, secret);
  await database.db.delete(authSessions).where(eq(authSessions.id, loginId));

  const result = (await issueDeployment(
    deploymentRequest(
      {
        cookies: { get: (name: string) => (name === "__Host-_at" ? login.token : undefined) },
        platform: {
          env: {
            AUTH_ORIGIN: "https://pontia.example",
            DB: database.binding,
            JWT_SECRET: secret,
          },
        },
        url: new URL("https://pontia.example/settings/edges/deploy"),
      } as unknown as RequestEvent,
      "edge.example.com",
    ),
  )) as { deployment: { command: string } };

  expect(result.deployment.command).toContain("sudo pontia-edge init");
  expect(result.deployment.command).not.toContain("--agree-to-lets-encrypt-subscriber-agreement");
});

test("an authenticated user issues a deployment for a normalized hostname they enter", async () => {
  const event = await authenticatedEvent(ownerId);
  const result = (await issueDeployment(deploymentRequest(event, "  Edge.Example.COM  "))) as {
    deployment: { hostname: string; command: string };
  };

  expect(result.deployment.hostname).toBe("edge.example.com");
  expect(result.deployment.command).toContain("sudo pontia-edge init");
  expect(result.deployment.command).not.toContain("--port");
  expect(result.deployment.command).not.toContain("--acme-challenge");
  expect(result.deployment.command).not.toContain("--agree-to-lets-encrypt-subscriber-agreement");
});

test("Pontia domains can be entered like any other domain", async () => {
  const event = await authenticatedEvent(ownerId);
  for (const hostname of ["sg.edge.pontia.dev", "relay.pontia.run"]) {
    const result = (await issueDeployment(deploymentRequest(event, hostname))) as {
      deployment: { hostname: string };
    };
    expect(result.deployment.hostname).toBe(hostname);
  }
});

test("public IP addresses can be entered without a domain", async () => {
  const event = await authenticatedEvent(ownerId);
  for (const hostname of ["8.8.8.8", "2606:4700:4700::1111"]) {
    const result = (await issueDeployment(deploymentRequest(event, hostname))) as {
      deployment: { hostname: string };
    };
    expect(result.deployment.hostname).toBe(hostname);
  }
});

test("invalid and already registered hostnames are rejected with the entered value", async () => {
  const event = await authenticatedEvent(ownerId);
  for (const hostname of [
    "",
    "localhost",
    "10.0.0.1",
    "2001:db8::1",
    "nas.local",
    "https://edge.example.com",
  ]) {
    expect(await issueDeployment(deploymentRequest(event, hostname))).toMatchObject({
      status: 400,
      data: { hostname: hostname.trim().toLowerCase() },
    });
  }
  await database.db.insert(edges).values({
    id: "edge-existing",
    userId: ownerId,
    name: "Existing",
    tunnelUrl: "wss://edge.example.com/tunnel",
    serviceCredentialHash: "secret-hash",
  });
  expect(await issueDeployment(deploymentRequest(event, "edge.example.com"))).toMatchObject({
    status: 409,
    data: { hostname: "edge.example.com" },
  });
});
