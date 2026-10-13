import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { login } from "../src/lib/server/auth/identity";
import { createBrowserRefreshToken, refreshBrowserSession } from "../src/lib/server/auth/refresh";
import { authSessions } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const database = testDatabase();
const profile = {
  provider: "github" as const,
  providerSubject: "refresh-test-user",
  email: null,
  emailVerified: false,
  displayName: null,
  avatarUrl: null,
};

test("browser refresh token issuance and refresh record the latest use", async () => {
  const createdAt = new Date("2026-01-01T00:00:00.000Z");
  const issuedAt = new Date("2026-01-02T00:00:00.000Z");
  const refreshedAt = new Date("2026-01-03T00:00:00.000Z");
  const sessionId = await login(database.db, profile, createdAt);

  expect(
    await database.db
      .select({ lastSeenAt: authSessions.lastSeenAt })
      .from(authSessions)
      .where(eq(authSessions.id, sessionId))
      .get(),
  ).toEqual({ lastSeenAt: createdAt.toISOString() });

  const issued = await createBrowserRefreshToken(database.db, sessionId, issuedAt);
  expect(
    await database.db
      .select({ lastSeenAt: authSessions.lastSeenAt })
      .from(authSessions)
      .where(eq(authSessions.id, sessionId))
      .get(),
  ).toEqual({ lastSeenAt: issuedAt.toISOString() });

  await refreshBrowserSession(database.db, issued.token, refreshedAt);
  expect(
    await database.db
      .select({ lastSeenAt: authSessions.lastSeenAt })
      .from(authSessions)
      .where(eq(authSessions.id, sessionId))
      .get(),
  ).toEqual({ lastSeenAt: refreshedAt.toISOString() });
});
