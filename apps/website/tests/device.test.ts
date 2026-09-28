import { expect, test } from "bun:test";
import { eq } from "drizzle-orm";
import {
  beginDeviceAuthorization,
  decideDeviceAuthorization,
  parseUserCode,
  pollDeviceAuthorization,
  recordAuthorizationAttempt,
  recordPollAttempt,
  recordUserCodeAttempt,
} from "../src/lib/server/auth/device";
import { activeLogin, login } from "../src/lib/server/auth/identity";
import { authSessions, deviceAuthorizations } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const profile = {
  provider: "google" as const,
  providerSubject: "device-test-user",
  email: null,
  emailVerified: false,
  displayName: "Device user",
  avatarUrl: null,
};
const database = testDatabase();
const now = new Date("2026-09-27T00:00:00.000Z");

async function userId() {
  const id = await login(database.db, profile, now);
  return (await activeLogin(database.db, id, undefined, now))!.userId;
}

test("device authorization is rate limited, approved once, and stores only credential hashes", async () => {
  const authorization = await beginDeviceAuthorization(
    database.db,
    "https://example.com/device",
    now,
  );
  expect(authorization.user_code).toMatch(/^[BCDFGHJKLMNPQRSTVWXZ]{4}-[BCDFGHJKLMNPQRSTVWXZ]{4}$/);
  expect(authorization.device_code).toHaveLength(43);
  const [stored] = await database.db.select().from(deviceAuthorizations);
  expect(stored.deviceCodeHash).not.toContain(authorization.device_code);
  expect(stored.userCode).toBe(authorization.user_code.replace("-", ""));

  expect(await pollDeviceAuthorization(database.db, authorization.device_code, now)).toEqual({
    status: "authorization_pending",
  });
  expect(
    await pollDeviceAuthorization(
      database.db,
      authorization.device_code,
      new Date(now.getTime() + 1000),
    ),
  ).toEqual({ status: "slow_down" });
  expect(
    await decideDeviceAuthorization(
      database.db,
      authorization.user_code,
      await userId(),
      "approved",
      new Date(now.getTime() + 1000),
    ),
  ).toBe(true);

  const result = await pollDeviceAuthorization(
    database.db,
    authorization.device_code,
    new Date(now.getTime() + 5000),
  );
  expect(result.status).toBe("authorized");
  if (result.status !== "authorized") throw new Error("expected authorization");
  expect(result.token).toMatch(/^ptr_v1_[0-9a-f-]+_[A-Za-z0-9_-]{43}$/);
  const secret = result.token.split("_")[3];
  const [session] = await database.db
    .select()
    .from(authSessions)
    .where(eq(authSessions.kind, "cli"));
  expect(session).toMatchObject({
    kind: "cli",
    accountId: null,
    expiresAt: null,
  });
  expect(session.tokenHash).not.toBe(secret);
  expect(
    await pollDeviceAuthorization(
      database.db,
      authorization.device_code,
      new Date(now.getTime() + 10_000),
    ),
  ).toEqual({ status: "expired_token" });
  expect(
    await database.db.select().from(authSessions).where(eq(authSessions.kind, "cli")),
  ).toHaveLength(1);
});

test("denied and expired requests cannot be approved or exchanged", async () => {
  const denied = await beginDeviceAuthorization(database.db, "https://example.com/device", now);
  expect(
    await decideDeviceAuthorization(database.db, denied.user_code, await userId(), "denied", now),
  ).toBe(true);
  expect(
    await decideDeviceAuthorization(database.db, denied.user_code, await userId(), "approved", now),
  ).toBe(false);
  expect(await pollDeviceAuthorization(database.db, denied.device_code, now)).toEqual({
    status: "access_denied",
  });

  const expired = await beginDeviceAuthorization(database.db, "https://example.com/device", now);
  const afterExpiry = new Date(now.getTime() + 300_000);
  expect(
    await decideDeviceAuthorization(
      database.db,
      expired.user_code,
      await userId(),
      "approved",
      afterExpiry,
    ),
  ).toBe(false);
  expect(await pollDeviceAuthorization(database.db, expired.device_code, afterExpiry)).toEqual({
    status: "expired_token",
  });
});

test("user code validation and authenticated attempt limiting reject enumeration", async () => {
  expect(parseUserCode("BCDF-GHJK")).toBe("BCDFGHJK");
  expect(parseUserCode("bcdf-ghjk")).toBeNull();
  expect(parseUserCode("BCDFGHJK")).toBeNull();
  expect(parseUserCode("AAAA-AAAA")).toBeNull();
  let userCodeAttemptsAllowed = true;
  for (let attempt = 0; attempt < 10; attempt++)
    userCodeAttemptsAllowed &&= await recordUserCodeAttempt(database.db, "browser-login", now);
  expect(userCodeAttemptsAllowed).toBe(true);
  expect(await recordUserCodeAttempt(database.db, "browser-login", now)).toBe(false);
  expect(
    await recordUserCodeAttempt(database.db, "browser-login", new Date(now.getTime() + 300_001)),
  ).toBe(true);

  let authorizationAttemptsAllowed = true;
  for (let attempt = 0; attempt < 20; attempt++)
    authorizationAttemptsAllowed &&= await recordAuthorizationAttempt(
      database.db,
      "192.0.2.1",
      now,
    );
  expect(authorizationAttemptsAllowed).toBe(true);
  expect(await recordAuthorizationAttempt(database.db, "192.0.2.1", now)).toBe(false);

  let pollAttemptsAllowed = true;
  for (let attempt = 0; attempt < 120; attempt++)
    pollAttemptsAllowed &&= await recordPollAttempt(database.db, "192.0.2.1", now);
  expect(pollAttemptsAllowed).toBe(true);
  expect(await recordPollAttempt(database.db, "192.0.2.1", now)).toBe(false);
});
