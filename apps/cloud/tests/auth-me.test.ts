import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { base64url } from "jose";
import { GET } from "../src/routes/api/auth/me/+server";
import { sha256Base64url } from "../src/lib/server/crypto";
import { authSessions, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const database = testDatabase();
const sessionId = "0195e7d1-1b22-7c33-9d44-123456789abc";
const secret = base64url.encode(new Uint8Array(32).fill(9));
const credential = `ptr_v1_${sessionId}_${secret}`;
const get = GET as unknown as (event: RequestEvent) => Promise<Response>;

function request(token?: string) {
  return get({
    request: new Request("https://example.com/api/auth/me", {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    }),
    platform: { env: { DB: database.binding } },
  } as unknown as RequestEvent);
}

async function signIn(displayName: string | null) {
  await database.db.insert(users).values({ id: "current-user", displayName });
  await database.db.insert(users).values({ id: "other-user", displayName: "Other" });
  await database.db.insert(authSessions).values({
    id: sessionId,
    userId: "current-user",
    kind: "cli",
    tokenHash: await sha256Base64url(secret),
  });
}

for (const displayName of ["Ada", null]) {
  test(`current user comes from the authenticated CLI session (${displayName})`, async () => {
    await signIn(displayName);
    const response = await request(credential);
    expect(response.status).toBe(200);
    const user = (await response.json()) as { id: string; display_name: string | null };
    expect(user).toEqual({ id: "current-user", display_name: displayName });
  });
}

test("missing, incorrect, expired, and revoked credentials cannot read the user", async () => {
  await signIn("Ada");
  expect((await request()).status).toBe(401);
  expect(
    (await request(`ptr_v1_${sessionId}_${base64url.encode(new Uint8Array(32))}`)).status,
  ).toBe(401);
  await database.db
    .update(authSessions)
    .set({ expiresAt: "2000-01-01T00:00:00.000Z" })
    .where(eq(authSessions.id, sessionId));
  expect((await request(credential)).status).toBe(401);
  await database.db.delete(authSessions).where(eq(authSessions.id, sessionId));
  expect((await request(credential)).status).toBe(401);
});

test("browser session secrets cannot authenticate as CLI credentials", async () => {
  await signIn("Ada");
  await database.db
    .update(authSessions)
    .set({ kind: "browser" })
    .where(eq(authSessions.id, sessionId));
  expect((await request(credential)).status).toBe(401);
});
