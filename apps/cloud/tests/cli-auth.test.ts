import { expect, test } from "bun:test";
import { base64url } from "jose";
import { authenticateCliCredential } from "../src/lib/server/auth/cli";
import { sha256Base64url } from "../src/lib/server/crypto";
import { authSessions, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const database = testDatabase();
const sessionId = "0195e7b2-3f45-7a61-8c20-1d34a56b7890";
const secret = base64url.encode(new Uint8Array(32).fill(7));

test("CLI credential authenticates its trusted user", async () => {
  await database.db.insert(users).values({ id: "user-cli" });
  await database.db.insert(authSessions).values({
    id: sessionId,
    userId: "user-cli",
    kind: "cli",
    expiresAt: null,
    tokenHash: await sha256Base64url(secret),
  });

  expect(await authenticateCliCredential(database.db, `ptr_v1_${sessionId}_${secret}`)).toEqual({
    userId: "user-cli",
    sessionId,
  });
  expect(
    await authenticateCliCredential(
      database.db,
      `ptr_v1_${sessionId}_${base64url.encode(new Uint8Array(32).fill(8))}`,
    ),
  ).toBeNull();
});

test("CLI credentials with no expiry remain valid in the future", async () => {
  await database.db.insert(users).values({ id: "user-cli" });
  await database.db.insert(authSessions).values({
    id: sessionId,
    userId: "user-cli",
    kind: "cli",
    expiresAt: null,
    tokenHash: await sha256Base64url(secret),
  });

  expect(
    await authenticateCliCredential(
      database.db,
      `ptr_v1_${sessionId}_${secret}`,
      new Date("2099-01-01T00:00:00.000Z"),
    ),
  ).toEqual({ userId: "user-cli", sessionId });
});

for (const { name, expiresAt, valid } of [
  { name: "before expiry", expiresAt: "2030-01-01T00:00:00.001Z", valid: true },
  { name: "at expiry", expiresAt: "2030-01-01T00:00:00.000Z", valid: false },
  { name: "after expiry", expiresAt: "2029-12-31T23:59:59.999Z", valid: false },
]) {
  test(`CLI credential authentication ${name}`, async () => {
    await database.db.insert(users).values({ id: "user-cli" });
    await database.db.insert(authSessions).values({
      id: sessionId,
      userId: "user-cli",
      kind: "cli",
      expiresAt,
      tokenHash: await sha256Base64url(secret),
    });

    const principal = await authenticateCliCredential(
      database.db,
      `ptr_v1_${sessionId}_${secret}`,
      new Date("2030-01-01T00:00:00.000Z"),
    );
    expect(principal).toEqual(valid ? { userId: "user-cli", sessionId } : null);
  });
}

test("CLI credential parser rejects other kinds and non-canonical tokens", async () => {
  await database.db.insert(users).values({ id: "user-browser" });
  await database.db.insert(authSessions).values({
    id: sessionId,
    userId: "user-browser",
    kind: "browser",
    tokenHash: await sha256Base64url(secret),
  });

  for (const credential of [
    `ptr_v1_${sessionId}_${secret}`,
    `ptr_v2_${sessionId}_${secret}`,
    `ptr_v1_not-a-uuid_${secret}`,
    `ptr_v1_${sessionId}_${secret}=`,
    `ptr_v1_${sessionId}_${secret}_extra`,
  ]) {
    expect(await authenticateCliCredential(database.db, credential)).toBeNull();
  }
});
