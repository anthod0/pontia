import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { issueLogin } from "../src/lib/server/auth/jwt";
import { authSessions, edges, users } from "../src/lib/server/db/schema";
import { load } from "../src/routes/edges/+page.server";
import { actions, load as loadDeployment } from "../src/routes/edges/deploy/+page.server";
import { testDatabase } from "./database";

const issueDeployment = actions.default as (event: RequestEvent) => Promise<unknown>;
const database = testDatabase();
const loadEdges = load as unknown as (event: RequestEvent) => Promise<{ edges: unknown[] }>;
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
    url: new URL("https://pontia.example/edges"),
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

test("a user without edges receives an empty list", async () => {
  const event = await authenticatedEvent(ownerId);
  expect((await loadEdges(event)).edges).toEqual([]);
});

test("the page lists only owned edges in stable order without credentials", async () => {
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
      tunnelUrl: `wss://${edge.id}.example/tunnel`,
      serviceCredentialHash: "secret-hash",
      createdAt,
    })),
  );

  expect((await loadEdges(event)).edges).toEqual([
    { id: "edge-a", name: "Singapore", tunnelUrl: "wss://edge-a.example/tunnel", createdAt },
    { id: "edge-b", name: "Singapore", tunnelUrl: "wss://edge-b.example/tunnel", createdAt },
    { id: "edge-z", name: "Tokyo", tunnelUrl: "wss://edge-z.example/tunnel", createdAt },
  ]);
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
    url: new URL("https://pontia.example/edges/deploy"),
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
    url: new URL("https://pontia.example/edges"),
  } as unknown as RequestEvent)) as { deployment: { command: string } };

  expect(result.deployment.command).toContain("sudo pontia-edge init");
  expect(result.deployment.command).not.toContain("--agree-to-lets-encrypt-subscriber-agreement");
});
