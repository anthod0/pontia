import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import { CompactSign } from "jose";
import { v7 as uuidv7 } from "uuid";
import { authSessions, users } from "../src/lib/server/db/schema";
import { activeLogin, login, logout } from "../src/lib/server/auth/identity";
import { issueLogin, signedLogin, signingKey, verifyLogin } from "../src/lib/server/auth/jwt";
import { testDatabase } from "./database";

const database = testDatabase();
const secret = "test-jwt-secret-with-at-least-32-bytes";
const now = new Date("2026-09-26T00:00:00Z");
const later = new Date("2026-09-26T00:15:00Z");

async function credential() {
  const id = await login(
    database.db,
    {
      provider: "google",
      providerSubject: "sub",
      email: null,
      emailVerified: false,
      displayName: "Original",
      avatarUrl: null,
    },
    now,
  );
  const issued = await issueLogin(database.db, id, secret, now);
  return {
    id,
    token: issued.token,
    claims: await verifyLogin(issued.token, secret, now),
  };
}

test("JWT-only verification expires in fifteen minutes; reissuance reads the current profile", async () => {
  const { id, token, claims } = await credential();
  expect(claims.exp - claims.iat).toBe(900);
  await expect(verifyLogin(token, secret, later)).rejects.toThrow();
  await database.db
    .update(users)
    .set({ displayName: "Updated", avatarUrl: "https://example.com/new" })
    .where(eq(users.id, claims.user_id));
  const reissued = await issueLogin(database.db, id, secret, later, claims.user_id);
  expect(await verifyLogin(reissued.token, secret, later)).toMatchObject({
    sub: id,
    user_id: claims.user_id,
    display_name: "Updated",
    avatar_url: "https://example.com/new",
    iat: claims.exp,
    exp: claims.exp + 900,
  });
  expect((await activeLogin(database.db, id, claims.user_id, later))!.expiresAt).toBe(
    "2026-10-26T00:00:00.000Z",
  );
});

test("logout prevents reissuance while an already-issued JWT remains valid until expiration", async () => {
  const { id, token, claims } = await credential();
  await logout(database.db, id, claims.user_id);
  expect(await verifyLogin(token, secret, now)).toMatchObject({ sub: id });
  await expect(issueLogin(database.db, id, secret, later, claims.user_id)).rejects.toThrow();
});

test("issuance caps JWT lifetime at the login expiry and refuses expiry or mismatched users", async () => {
  const { id, claims } = await credential();
  const nearEnd = new Date("2026-10-25T23:45:00Z");
  const reissued = await issueLogin(database.db, id, secret, nearEnd, claims.user_id);
  expect((await verifyLogin(reissued.token, secret, nearEnd)).exp).toBe(
    Date.parse("2026-10-26T00:00:00Z") / 1000,
  );
  await expect(
    issueLogin(database.db, id, secret, new Date("2026-10-26T00:00:00Z"), claims.user_id),
  ).rejects.toThrow();
  const otherId = uuidv7();
  await database.db.insert(users).values({ id: otherId });
  await database.db.update(authSessions).set({ userId: otherId }).where(eq(authSessions.id, id));
  await expect(issueLogin(database.db, id, secret, later, claims.user_id)).rejects.toThrow();
  expect(claims.user_id).not.toBe(otherId);
});

test("signed JWT parsing rejects tampering, wrong headers, and malformed claims", async () => {
  const { token, claims } = await credential();
  const parts = token.split(".");
  parts[1] = Buffer.from(JSON.stringify({ ...claims, user_id: uuidv7() })).toString("base64url");
  await expect(signedLogin(parts.join("."), secret, now)).rejects.toThrow();
  for (const [payload, header] of [
    [claims, { alg: "HS384", typ: "at+jwt" }],
    [claims, { alg: "HS256", typ: "oauth-state+jwt" }],
    [
      { ...claims, user_id: "not-a-uuid" },
      { alg: "HS256", typ: "at+jwt" },
    ],
    [
      { ...claims, exp: "expired" },
      { alg: "HS256", typ: "at+jwt" },
    ],
    [
      { ...claims, exp: claims.iat + 901 },
      { alg: "HS256", typ: "at+jwt" },
    ],
    [
      { ...claims, display_name: 123 },
      { alg: "HS256", typ: "at+jwt" },
    ],
    [
      { ...claims, nbf: claims.exp + 1 },
      { alg: "HS256", typ: "at+jwt" },
    ],
  ] as const) {
    const invalid = await new CompactSign(new TextEncoder().encode(JSON.stringify(payload)))
      .setProtectedHeader(header)
      .sign(signingKey(secret));
    await expect(signedLogin(invalid, secret, now)).rejects.toThrow();
  }
});
