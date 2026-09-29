import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { issueLogin } from "../src/lib/server/auth/jwt";
import { authSessions, users } from "../src/lib/server/db/schema";
import { actions } from "../src/routes/edges/+page.server";
import { testDatabase } from "./database";

const issueDeployment = actions.default as (event: RequestEvent) => Promise<unknown>;
const database = testDatabase();

test("an unauthenticated browser cannot issue an edge deployment", async () => {
  const result = (await issueDeployment({
    cookies: { get: () => undefined },
  } as unknown as RequestEvent)) as { status: number; data: { error: string } };

  expect(result.status).toBe(401);
  expect(result.data).toEqual({ error: "Sign in to create an edge deployment." });
});

test("a revoked browser login cannot issue an edge deployment", async () => {
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
    platform: { env: { DB: database.binding, JWT_SECRET: secret } },
  } as unknown as RequestEvent)) as { status: number };

  expect(result.status).toBe(401);
});
