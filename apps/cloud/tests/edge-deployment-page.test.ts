import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { issueLogin } from "../src/lib/server/auth/jwt";
import { authSessions, edges, users } from "../src/lib/server/db/schema";
import { actions as edgeActions, load } from "../src/routes/settings/edges/+page.server";
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
    cookies: { get: (name: string) => (name === "_at" ? login.token : undefined) },
    platform: {
      env: {
        AUTH_ORIGIN: "https://pontia.example",
        DB: database.binding,
        JWT_SECRET: secret,
      },
    },
    url: new URL("https://pontia.example/settings/edges"),
  } as unknown as RequestEvent;
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

test("the page separates owned and public edges in stable order without credentials", async () => {
  const event = await authenticatedEvent(ownerId);
  await database.db.insert(users).values({ id: "user-other" });
  const createdAt = "2026-01-01T00:00:00.000Z";
  await database.db.insert(edges).values(
    [
      { id: "edge-z", userId: ownerId, name: "Tokyo" },
      { id: "edge-b", userId: ownerId, name: "Singapore" },
      { id: "edge-a", userId: ownerId, name: "Singapore" },
      { id: "edge-private", userId: "user-other", name: "Private" },
      { id: "edge-public", userId: "user-other", name: "Public", accessScope: "public" as const },
    ].map((edge) => ({
      ...edge,
      dnsLabel: edge.id,
      tunnelUrl: `wss://${edge.id}.example/tunnel`,
      serviceCredentialHash: "secret-hash",
      createdAt,
    })),
  );
  const result = await loadEdges(event);
  expect(result.ownedEdges).toEqual([
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
  ]);
});

test("an owner can rename an edge without changing its DNS label", async () => {
  const event = await authenticatedEvent(ownerId);
  const edgeId = "edge-owned";
  await database.db.insert(edges).values({
    id: edgeId,
    userId: ownerId,
    name: "brave-atlas",
    dnsLabel: "brave-atlas",
    tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
    serviceCredentialHash: "secret-hash",
  });
  event.request = new Request(event.url, {
    method: "POST",
    body: new URLSearchParams({ edge_id: edgeId, name: "  Home server  " }),
  });

  expect(await renameEdge(event)).toEqual({ success: "edge_renamed" });
  expect(
    await database.db
      .select({ name: edges.name, dnsLabel: edges.dnsLabel })
      .from(edges)
      .where(eq(edges.id, edgeId))
      .get(),
  ).toEqual({ name: "Home server", dnsLabel: "brave-atlas" });
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

  const result = (await issueDeployment({
    cookies: { get: (name: string) => (name === "_at" ? login.token : undefined) },
    platform: {
      env: {
        AUTH_ORIGIN: "https://pontia.example",
        DB: database.binding,
        JWT_SECRET: secret,
      },
    },
    url: new URL("https://pontia.example/settings/edges/deploy"),
  } as unknown as RequestEvent)) as { deployment: { command: string } };

  expect(result.deployment.command).toContain("sudo pontia-edge init");
  expect(result.deployment.command).not.toContain("--agree-to-lets-encrypt-subscriber-agreement");
});

test("an authenticated user can issue an edge deployment", async () => {
  const userId = "0199791c-6600-7000-8000-000000000003";
  const loginId = "0199791c-6600-7000-8000-000000000004";
  const secret = "test-signing-secret-with-at-least-32-bytes";
  await database.db.insert(users).values({ id: userId });
  await database.db.insert(authSessions).values({
    id: loginId,
    userId,
    expiresAt: "2099-01-01T00:00:00.000Z",
  });
  const login = await issueLogin(database.db, loginId, secret);

  const result = (await issueDeployment({
    cookies: { get: (name: string) => (name === "_at" ? login.token : undefined) },
    platform: {
      env: {
        AUTH_ORIGIN: "https://pontia.example",
        DB: database.binding,
        JWT_SECRET: secret,
      },
    },
    url: new URL("https://pontia.example/settings/edges"),
  } as unknown as RequestEvent)) as { deployment: { command: string } };

  expect(result.deployment.command).toContain("sudo pontia-edge init");
  expect(result.deployment.command).not.toContain("--agree-to-lets-encrypt-subscriber-agreement");
});
