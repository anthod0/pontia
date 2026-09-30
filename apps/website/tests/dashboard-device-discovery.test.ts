import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { issueLogin } from "../src/lib/server/auth/jwt";
import { authSessions, devices, edges, users } from "../src/lib/server/db/schema";
import { edgeApiOrigin } from "../src/lib/server/edge-network";
import {
  GET as listDevices,
  OPTIONS as listDevicesOptions,
} from "../src/routes/api/dashboard/devices/+server";
import {
  GET as getTarget,
  OPTIONS as getTargetOptions,
} from "../src/routes/api/dashboard/devices/[device_handle]/target/+server";
import { testDatabase } from "./database";

const callListDevices = listDevices as unknown as (event: RequestEvent) => Promise<Response>;
const callListDevicesOptions = listDevicesOptions as unknown as (
  event: RequestEvent,
) => Promise<Response>;
const callGetTarget = getTarget as unknown as (event: RequestEvent) => Promise<Response>;
const callGetTargetOptions = getTargetOptions as unknown as (
  event: RequestEvent,
) => Promise<Response>;

const database = testDatabase();
const dashboardOrigin = "https://app.pontia.dev";
const websiteOrigin = "https://pontia.dev";
const jwtSecret = "dashboard-discovery-signing-key-at-least-32-bytes";
const ownerId = "0195e7d1-1b22-7c33-9d44-123456789abc";
const otherId = "0195e7d2-1b22-7c33-9d44-123456789abc";
const sessionId = "0195e7d3-1b22-7c33-9d44-123456789abc";
const ownerDeviceId = "0195e7d4-1b22-7c33-9d44-123456789abc";

function event(
  path: string,
  options: { method?: string; origin?: string; token?: string } = {},
): RequestEvent {
  const url = new URL(path, websiteOrigin);
  return {
    url,
    params: { device_handle: url.pathname.split("/").at(-2) },
    request: new Request(url, {
      method: options.method ?? "GET",
      headers: options.origin === undefined ? undefined : { Origin: options.origin },
    }),
    platform: {
      env: { DB: database.binding, JWT_SECRET: jwtSecret, AUTH_ORIGIN: websiteOrigin },
    },
    cookies: { get: (name: string) => (name === "_at" ? options.token : undefined) },
  } as unknown as RequestEvent;
}

async function seedDashboardFixtureAndLogin() {
  await database.db.insert(users).values([{ id: ownerId }, { id: otherId }]);
  await database.db.insert(authSessions).values({
    id: sessionId,
    userId: ownerId,
    kind: "browser",
    expiresAt: new Date(Date.now() + 86_400_000).toISOString(),
  });
  await database.db.insert(edges).values([
    {
      id: "0195e7e1-1b22-7c33-9d44-123456789abc",
      userId: ownerId,
      name: "Owner edge",
      tunnelUrl: "wss://brave-silver-atlas.edge.pontia.dev/tunnel",
      serviceCredentialHash: "owner-hash",
    },
    {
      id: "0195e7e2-1b22-7c33-9d44-123456789abc",
      userId: otherId,
      name: "Other edge",
      tunnelUrl: "wss://calm-blue-arthur.edge.pontia.dev/tunnel",
      serviceCredentialHash: "other-hash",
    },
  ]);
  await database.db.insert(devices).values([
    {
      id: ownerDeviceId,
      userId: ownerId,
      edgeId: "0195e7e1-1b22-7c33-9d44-123456789abc",
      handle: "office-mac",
      name: "Office Mac",
    },
    {
      id: "0195e7d5-1b22-7c33-9d44-123456789abc",
      userId: otherId,
      edgeId: "0195e7e2-1b22-7c33-9d44-123456789abc",
      handle: "other-device",
      name: "Other device",
    },
  ]);
  return (await issueLogin(database.db, sessionId, jwtSecret)).token;
}

function expectCredentialedCors(response: Response) {
  expect(response.headers.get("access-control-allow-origin")).toBe(dashboardOrigin);
  expect(response.headers.get("access-control-allow-credentials")).toBe("true");
  expect(response.headers.get("vary")).toBe("Origin");
}

test("derives only canonical Pontia edge API origins", () => {
  expect(edgeApiOrigin("wss://brave-silver-atlas.edge.pontia.dev/tunnel")).toBe(
    "https://brave-silver-atlas.edge.pontia.dev",
  );
  for (const tunnelUrl of [
    "ws://brave-silver-atlas.edge.pontia.dev/tunnel",
    "wss://brave-silver-atlas.edge.pontia.dev:444/tunnel",
    "wss://brave-silver-atlas.edge.pontia.dev/tunnel?target=x",
    "wss://invented-hero-name.edge.pontia.dev/tunnel",
    "wss://brave-silver-atlas.edge.pontia.dev/tunnel/",
  ]) {
    expect(edgeApiOrigin(tunnelUrl)).toBeNull();
  }
});

test("lists only the authenticated user's minimal stateless device records", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const response = await callListDevices(
    event("/api/dashboard/devices", { origin: dashboardOrigin, token }),
  );
  expect(response.status).toBe(200);
  expect((await response.json()) as unknown).toEqual([
    { device_handle: "office-mac", name: "Office Mac" },
  ]);
  expectCredentialedCors(response);
});

test("resolves a handle to trusted target fields without request overrides", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const response = await callGetTarget(
    event(
      "/api/dashboard/devices/office-mac/target?device_id=attacker&edge_api_origin=https://attacker.example",
      { origin: dashboardOrigin, token },
    ),
  );
  expect(response.status).toBe(200);
  expect((await response.json()) as unknown).toEqual({
    device_handle: "office-mac",
    device_id: ownerDeviceId,
    edge_api_origin: "https://brave-silver-atlas.edge.pontia.dev",
  });
  expectCredentialedCors(response);
});

test("target lookup does not reveal another user's handle", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const other = await callGetTarget(
    event("/api/dashboard/devices/other-device/target", { origin: dashboardOrigin, token }),
  );
  const missing = await callGetTarget(
    event("/api/dashboard/devices/missing-device/target", { origin: dashboardOrigin, token }),
  );
  expect(other.status).toBe(404);
  expect(missing.status).toBe(404);
  expect(await other.json()).toEqual(await missing.json());
});

test("target resolution follows the device's current binding after a public edge becomes private", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const edgeId = "0195e7e2-1b22-7c33-9d44-123456789abc";
  await database.db.update(edges).set({ accessScope: "public" }).where(eq(edges.id, edgeId));
  await database.db.insert(devices).values({
    id: "0195e7d6-1b22-7c33-9d44-123456789abc",
    userId: ownerId,
    edgeId,
    handle: "public-edge-device",
    name: "Public edge device",
  });
  await database.db.update(edges).set({ accessScope: "private" }).where(eq(edges.id, edgeId));

  const response = await callGetTarget(
    event("/api/dashboard/devices/public-edge-device/target", { origin: dashboardOrigin, token }),
  );
  expect(response.status).toBe(200);
  expect((await response.json()) as { edge_api_origin: string }).toMatchObject({
    edge_api_origin: "https://calm-blue-arthur.edge.pontia.dev",
  });
});

test("invalid tunnel URLs do not produce a target", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const edgeId = "0195e7e3-1b22-7c33-9d44-123456789abc";
  await database.db.insert(edges).values({
    id: edgeId,
    userId: ownerId,
    name: "Invalid edge",
    tunnelUrl: "wss://attacker.example/tunnel",
    serviceCredentialHash: "invalid-hash",
  });
  await database.db.insert(devices).values({
    id: "0195e7d6-1b22-7c33-9d44-123456789abc",
    userId: ownerId,
    edgeId,
    handle: "invalid-edge",
    name: "Invalid edge",
  });
  const response = await callGetTarget(
    event("/api/dashboard/devices/invalid-edge/target", { origin: dashboardOrigin, token }),
  );
  expect(response.status).toBe(404);
});

test("both discovery endpoints enforce authenticated credentialed Dashboard CORS", async () => {
  const token = await seedDashboardFixtureAndLogin();
  for (const [path, get, options] of [
    ["/api/dashboard/devices", callListDevices, callListDevicesOptions],
    ["/api/dashboard/devices/office-mac/target", callGetTarget, callGetTargetOptions],
  ] as const) {
    const unauthenticated = await get(event(path, { origin: dashboardOrigin }));
    expect(unauthenticated.status).toBe(401);
    expectCredentialedCors(unauthenticated);

    const denied = await get(event(path, { origin: "https://evil.example", token }));
    expect(denied.status).toBe(403);
    expect(denied.headers.get("access-control-allow-origin")).toBeNull();
    expect(denied.headers.get("vary")).toBe("Origin");

    const preflight = await options(event(path, { method: "OPTIONS", origin: dashboardOrigin }));
    expect(preflight.status).toBe(204);
    expectCredentialedCors(preflight);
  }
});
