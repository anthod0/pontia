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
  POST as connect,
  OPTIONS as connectOptions,
} from "../src/routes/api/dashboard/devices/[device_handle]/connect/+server";
import { testDatabase } from "./database";

const callListDevices = listDevices as unknown as (event: RequestEvent) => Promise<Response>;
const callListDevicesOptions = listDevicesOptions as unknown as (
  event: RequestEvent,
) => Promise<Response>;
const callConnect = connect as unknown as (event: RequestEvent) => Promise<Response>;
const callConnectOptions = connectOptions as unknown as (event: RequestEvent) => Promise<Response>;

const database = testDatabase();
const dashboardOrigin = "https://app.pontia.dev";
const cloudOrigin = "https://pontia.dev";
const jwtSecret = "dashboard-discovery-signing-key-at-least-32-bytes";
const ownerId = "0195e7d1-1b22-7c33-9d44-123456789abc";
const otherId = "0195e7d2-1b22-7c33-9d44-123456789abc";
const sessionId = "0195e7d3-1b22-7c33-9d44-123456789abc";
const ownerDeviceId = "0195e7d4-1b22-7c33-9d44-123456789abc";
const browserPublicKey = "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk";
const signingKeys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
const signingKey = Buffer.from(
  await crypto.subtle.exportKey("pkcs8", signingKeys.privateKey),
).toString("base64url");

function event(
  path: string,
  options: { method?: string; origin?: string; token?: string; body?: unknown } = {},
): RequestEvent {
  const url = new URL(path, cloudOrigin);
  const method = options.method ?? (url.pathname.endsWith("/connect") ? "POST" : "GET");
  return {
    url,
    params: { device_handle: url.pathname.split("/").at(-2) },
    request: new Request(url, {
      method,
      headers: {
        ...(options.origin ? { Origin: options.origin } : {}),
        "Content-Type": "application/json",
      },
      body:
        method === "POST"
          ? JSON.stringify(options.body ?? { browser_public_key: browserPublicKey })
          : undefined,
    }),
    platform: {
      env: {
        DB: database.binding,
        JWT_SECRET: jwtSecret,
        AUTH_ORIGIN: cloudOrigin,
        E2E_CAPABILITY_SIGNING_KEY: signingKey,
      },
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
      dnsLabel: "brave-atlas",
      tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
      serviceCredentialHash: "owner-hash",
    },
    {
      id: "0195e7e2-1b22-7c33-9d44-123456789abc",
      userId: otherId,
      name: "Other edge",
      dnsLabel: "calm-arthur",
      tunnelUrl: "wss://calm-arthur.edge.pontia.dev/tunnel",
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
      e2ePublicKey: browserPublicKey,
      e2eKeyVersion: 1,
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
  expect(edgeApiOrigin("wss://brave-atlas.edge.pontia.dev/tunnel")).toBe(
    "https://brave-atlas.edge.pontia.dev",
  );
  expect(edgeApiOrigin("wss://registered-name.edge.pontia.dev/tunnel")).toBe(
    "https://registered-name.edge.pontia.dev",
  );
  expect(edgeApiOrigin("wss://brave-atlas.edge.pontia.dev:8443/tunnel")).toBe(
    "https://brave-atlas.edge.pontia.dev:8443",
  );
  for (const tunnelUrl of [
    "ws://brave-atlas.edge.pontia.dev/tunnel",
    "wss://brave-atlas.edge.pontia.dev:25/tunnel",
    "wss://brave-atlas.edge.pontia.dev/tunnel?target=x",
    "wss://-invalid.edge.pontia.dev/tunnel",
    "wss://brave-atlas.edge.pontia.dev/tunnel/",
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

test("accepts an issued access JWT without reading its browser session", async () => {
  const token = await seedDashboardFixtureAndLogin();
  await database.db.delete(authSessions).where(eq(authSessions.id, sessionId));
  const response = await callListDevices(
    event("/api/dashboard/devices", { origin: dashboardOrigin, token }),
  );
  expect(response.status).toBe(200);
});

test("connect returns trusted routing and browser-bound signed authorization without request overrides", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const response = await callConnect(
    event(
      "/api/dashboard/devices/office-mac/connect?device_id=attacker&edge_api_origin=https://attacker.example",
      { origin: dashboardOrigin, token },
    ),
  );
  expect(response.status).toBe(200);
  const body = (await response.json()) as Record<string, unknown>;
  expect(body).toEqual({
    device_handle: "office-mac",
    device_id: ownerDeviceId,
    edge_api_origin: "https://brave-atlas.edge.pontia.dev",
    device_public_key: browserPublicKey,
    device_key_version: 1,
    capability: expect.any(String),
  });
  const capability = Buffer.from(body.capability as string, "base64url");
  const signed = Buffer.concat([
    Buffer.from("pontia-e2e-capability-v1\0"),
    capability.subarray(0, 105),
  ]);
  expect(
    await crypto.subtle.verify("Ed25519", signingKeys.publicKey, capability.subarray(105), signed),
  ).toBe(true);
  expect(capability.subarray(25, 57).toString("base64url")).toBe(browserPublicKey);
  expectCredentialedCors(response);
});

test("connect does not reveal another user's handle", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const other = await callConnect(
    event("/api/dashboard/devices/other-device/connect", { origin: dashboardOrigin, token }),
  );
  const missing = await callConnect(
    event("/api/dashboard/devices/missing-device/connect", { origin: dashboardOrigin, token }),
  );
  expect(other.status).toBe(404);
  expect(missing.status).toBe(404);
  expect(await other.json()).toEqual(await missing.json());
});

test("connect follows the device's current binding after a public edge becomes private", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const edgeId = "0195e7e2-1b22-7c33-9d44-123456789abc";
  await database.db.update(edges).set({ accessScope: "public" }).where(eq(edges.id, edgeId));
  await database.db.insert(devices).values({
    id: "0195e7d6-1b22-7c33-9d44-123456789abc",
    userId: ownerId,
    edgeId,
    handle: "public-edge-device",
    name: "Public edge device",
    e2ePublicKey: Buffer.alloc(32, 7).toString("base64url"),
    e2eKeyVersion: 1,
  });
  await database.db.update(edges).set({ accessScope: "private" }).where(eq(edges.id, edgeId));

  const response = await callConnect(
    event("/api/dashboard/devices/public-edge-device/connect", { origin: dashboardOrigin, token }),
  );
  expect(response.status).toBe(200);
  expect((await response.json()) as { edge_api_origin: string }).toMatchObject({
    edge_api_origin: "https://calm-arthur.edge.pontia.dev",
  });
});

test("connect rejects invalid tunnel URLs", async () => {
  const token = await seedDashboardFixtureAndLogin();
  const edgeId = "0195e7e3-1b22-7c33-9d44-123456789abc";
  await database.db.insert(edges).values({
    id: edgeId,
    userId: ownerId,
    name: "Invalid edge",
    dnsLabel: "invalid-edge",
    tunnelUrl: "wss://attacker.example/tunnel",
    serviceCredentialHash: "invalid-hash",
  });
  await database.db.insert(devices).values({
    id: "0195e7d6-1b22-7c33-9d44-123456789abc",
    userId: ownerId,
    edgeId,
    handle: "invalid-edge",
    name: "Invalid edge",
    e2ePublicKey: Buffer.alloc(32, 8).toString("base64url"),
    e2eKeyVersion: 1,
  });
  const response = await callConnect(
    event("/api/dashboard/devices/invalid-edge/connect", { origin: dashboardOrigin, token }),
  );
  expect(response.status).toBe(404);
});

test("connect rejects malformed browser keys and extra request fields", async () => {
  const token = await seedDashboardFixtureAndLogin();
  for (const [body, status] of [
    [{}, 400],
    [{ browser_public_key: 12 }, 400],
    [{ browser_public_key: "invalid" }, 404],
    [{ browser_public_key: `${browserPublicKey}=` }, 404],
    [{ browser_public_key: browserPublicKey, device_id: ownerDeviceId }, 400],
  ] as const) {
    const response = await callConnect(
      event("/api/dashboard/devices/office-mac/connect", {
        origin: dashboardOrigin,
        token,
        body,
      }),
    );
    expect(response.status).toBe(status);
  }
});

test("connect refuses a device without an E2E key", async () => {
  const token = await seedDashboardFixtureAndLogin();
  await database.db
    .update(devices)
    .set({ e2ePublicKey: null, e2eKeyVersion: 0 })
    .where(eq(devices.id, ownerDeviceId));
  const response = await callConnect(
    event("/api/dashboard/devices/office-mac/connect", { origin: dashboardOrigin, token }),
  );
  expect(response.status).toBe(404);
});

test("device listing and connect enforce authenticated credentialed Dashboard CORS", async () => {
  const token = await seedDashboardFixtureAndLogin();
  for (const [path, get, options] of [
    ["/api/dashboard/devices", callListDevices, callListDevicesOptions],
    ["/api/dashboard/devices/office-mac/connect", callConnect, callConnectOptions],
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
