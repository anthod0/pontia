import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { issueLogin } from "../src/lib/server/auth/jwt";
import { accounts, authSessions, devices, edges, users } from "../src/lib/server/db/schema";
import { load as loadSettingsLayout } from "../src/routes/settings/+layout.server";
import { load as loadSettings } from "../src/routes/settings/+page.server";
import {
  actions as accountActions,
  load as loadAccountPage,
} from "../src/routes/settings/account/+page.server";
import {
  actions as deviceActions,
  load as loadDevicesPage,
} from "../src/routes/settings/devices/+page.server";
import {
  actions as sessionActions,
  load as loadSessionsPage,
} from "../src/routes/settings/sessions/+page.server";
import { testDatabase } from "./database";

const database = testDatabase();
const secret = "test-signing-secret-with-at-least-32-bytes";
const userId = "0199791c-6600-7000-8000-000000000001";
const currentSessionId = "0199791c-6600-7000-8000-000000000002";
const loadAccount = loadAccountPage as unknown as (event: RequestEvent) => Promise<{
  profile: { displayName: string | null };
}>;
const loadSessions = loadSessionsPage as unknown as (event: RequestEvent) => Promise<{
  sessions: { id: string; current: boolean }[];
}>;
const loadDevices = loadDevicesPage as unknown as (event: RequestEvent) => Promise<{
  devices: { id: string; edgeName: string }[];
}>;

async function signedInEvent(fields: Record<string, string> = {}) {
  const login = await issueLogin(database.db, currentSessionId, secret);
  const body = new FormData();
  for (const [name, value] of Object.entries(fields)) body.set(name, value);
  return {
    cookies: { get: (name: string) => (name === "_at" ? login.token : undefined) },
    platform: { env: { DB: database.binding, JWT_SECRET: secret } },
    request: new Request("https://pontia.example/settings/account", { method: "POST", body }),
    url: new URL("https://pontia.example/settings/account"),
  } as unknown as RequestEvent;
}

test("settings require sign-in and the settings root opens account settings", async () => {
  await expect(
    (loadSettingsLayout as unknown as (event: RequestEvent) => Promise<unknown>)({
      cookies: { get: () => undefined },
    } as unknown as RequestEvent),
  ).rejects.toMatchObject({ status: 303, location: "/login" });
  await expect(Promise.resolve().then(() => loadSettings({} as never))).rejects.toMatchObject({
    status: 303,
    location: "/settings/account",
  });
});

async function insertCurrentUser() {
  await database.db.insert(users).values({ id: userId, displayName: "Old name" });
  await database.db.insert(authSessions).values({
    id: currentSessionId,
    userId,
    kind: "browser",
    expiresAt: "2099-01-01T00:00:00.000Z",
  });
}

test("the settings pages return the current profile, active logins, and registered devices", async () => {
  await insertCurrentUser();
  const edgeId = "0199791c-6600-7000-8000-000000000020";
  const deviceId = "0199791c-6600-7000-8000-000000000030";
  await database.db.insert(edges).values({
    id: edgeId,
    userId,
    name: "Home edge",
    dnsLabel: "home-edge",
    tunnelUrl: "wss://home.example/tunnel",
    serviceCredentialHash: "hash",
  });
  await database.db.insert(devices).values({
    id: deviceId,
    userId,
    edgeId,
    handle: "home-device",
    name: "Home device",
  });

  const event = await signedInEvent();
  const [accountPage, sessionsPage, devicesPage] = await Promise.all([
    loadAccount(event),
    loadSessions(event),
    loadDevices(event),
  ]);

  expect(accountPage.profile.displayName).toBe("Old name");
  expect(sessionsPage.sessions).toEqual([
    expect.objectContaining({ id: currentSessionId, current: true }),
  ]);
  expect(devicesPage.devices).toEqual([
    expect.objectContaining({ id: deviceId, edgeName: "Home edge" }),
  ]);
});

test("signing out other browsers preserves the current browser and CLI credentials", async () => {
  await insertCurrentUser();
  await database.db.insert(authSessions).values([
    {
      id: "0199791c-6600-7000-8000-000000000003",
      userId,
      kind: "browser",
      expiresAt: "2099-01-01T00:00:00.000Z",
    },
    {
      id: "0199791c-6600-7000-8000-000000000004",
      userId,
      kind: "cli",
      tokenHash: "cli-hash",
    },
  ]);

  await (sessionActions.revokeOtherBrowsers as (event: RequestEvent) => Promise<unknown>)(
    await signedInEvent(),
  );

  expect(
    (await database.db.select().from(authSessions).where(eq(authSessions.userId, userId))).map(
      (session) => session.id,
    ),
  ).toEqual([currentSessionId, "0199791c-6600-7000-8000-000000000004"]);
});

test("revoking all CLI credentials does not revoke browser sessions", async () => {
  await insertCurrentUser();
  await database.db.insert(authSessions).values({
    id: "0199791c-6600-7000-8000-000000000004",
    userId,
    kind: "cli",
    tokenHash: "cli-hash",
  });

  await (sessionActions.revokeCli as (event: RequestEvent) => Promise<unknown>)(
    await signedInEvent(),
  );

  expect(
    await database.db.select().from(authSessions).where(eq(authSessions.userId, userId)),
  ).toEqual([expect.objectContaining({ id: currentSessionId, kind: "browser" })]);
});

test("the last sign-in method cannot be unlinked", async () => {
  await insertCurrentUser();
  await database.db.insert(accounts).values({
    id: "account-google",
    userId,
    provider: "google",
    providerSubject: "google-user",
  });

  const result = (await (
    accountActions.unlinkProvider as (event: RequestEvent) => Promise<unknown>
  )(await signedInEvent({ provider: "google" }))) as { status: number };

  expect(result.status).toBe(409);
  expect(await database.db.select().from(accounts).where(eq(accounts.userId, userId))).toHaveLength(
    1,
  );
});

test("unlinking a provider revokes only browser sessions created with that provider", async () => {
  await insertCurrentUser();
  await database.db.insert(accounts).values([
    {
      id: "account-google",
      userId,
      provider: "google",
      providerSubject: "google-user",
    },
    {
      id: "account-github",
      userId,
      provider: "github",
      providerSubject: "github-user",
    },
  ]);
  await database.db
    .update(authSessions)
    .set({ accountId: "account-google" })
    .where(eq(authSessions.id, currentSessionId));
  await database.db.insert(authSessions).values([
    {
      id: "0199791c-6600-7000-8000-000000000003",
      userId,
      accountId: "account-github",
      kind: "browser",
      expiresAt: "2099-01-01T00:00:00.000Z",
    },
    {
      id: "0199791c-6600-7000-8000-000000000004",
      userId,
      kind: "cli",
      tokenHash: "cli-hash",
    },
  ]);
  const event = await signedInEvent({ provider: "google" });

  await (accountActions.unlinkProvider as (event: RequestEvent) => Promise<unknown>)(event);

  expect(
    (await database.db.select().from(accounts).where(eq(accounts.userId, userId))).map(
      (account) => account.provider,
    ),
  ).toEqual(["github"]);
  expect(
    (await database.db.select().from(authSessions).where(eq(authSessions.userId, userId))).map(
      (session) => session.id,
    ),
  ).toEqual(["0199791c-6600-7000-8000-000000000003", "0199791c-6600-7000-8000-000000000004"]);
});

test("removing a device does not remove another user's device", async () => {
  await insertCurrentUser();
  const otherUserId = "0199791c-6600-7000-8000-000000000010";
  await database.db.insert(users).values({ id: otherUserId });
  await database.db.insert(edges).values({
    id: "0199791c-6600-7000-8000-000000000020",
    userId,
    name: "Edge",
    dnsLabel: "edge",
    tunnelUrl: "wss://edge.example/tunnel",
    serviceCredentialHash: "hash",
    accessScope: "public",
  });
  await database.db.insert(devices).values([
    {
      id: "0199791c-6600-7000-8000-000000000030",
      userId,
      edgeId: "0199791c-6600-7000-8000-000000000020",
      handle: "owner-device",
      name: "Owner device",
    },
    {
      id: "0199791c-6600-7000-8000-000000000031",
      userId: otherUserId,
      edgeId: "0199791c-6600-7000-8000-000000000020",
      handle: "other-device",
      name: "Other device",
    },
  ]);

  await (deviceActions.removeDevice as (event: RequestEvent) => Promise<unknown>)(
    await signedInEvent({ device_id: "0199791c-6600-7000-8000-000000000030" }),
  );

  expect((await database.db.select().from(devices)).map((device) => device.id)).toEqual([
    "0199791c-6600-7000-8000-000000000031",
  ]);
});
