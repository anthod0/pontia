import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { base64url } from "jose";
import { issueLogin } from "../src/lib/server/auth/jwt";
import { sha256Base64url } from "../src/lib/server/crypto";
import { authSessions, devices, edges, users } from "../src/lib/server/db/schema";
import { POST as launch } from "../src/routes/api/dashboard/devices/[device_handle]/bootstrap/+server";
import { POST as redeem } from "../src/routes/api/edge/dashboard-tickets/redeem/+server";
import { testDatabase } from "./database";

const callLaunch = launch as unknown as (event: RequestEvent) => Promise<Response>;
const callRedeem = redeem as unknown as (event: RequestEvent) => Promise<Response>;
const database = testDatabase();
const cloudOrigin = "https://pontia.dev";
const dashboardOrigin = "https://app.pontia.dev";
const jwtSecret = "dashboard-authorization-signing-key-at-least-32-bytes";
const userId = "0195e7d1-1b22-7c33-9d44-123456789abc";
const otherUserId = "0195e7d2-1b22-7c33-9d44-123456789abc";
const sessionId = "0195e7d3-1b22-7c33-9d44-123456789abc";
const deviceId = "0195e7d4-1b22-7c33-9d44-123456789abc";
const edgeId = "0195e7e1-1b22-7c33-9d44-123456789abc";
const edgeSecret = base64url.encode(new Uint8Array(32).fill(32));
const edgeCredential = `pec_v1_${edgeId}_${edgeSecret}`;

function launchEvent(
  handle: string,
  options: {
    origin?: string;
    token?: string;
    query?: string;
    body?: URLSearchParams;
    binding?: unknown;
  } = {},
) {
  const url = new URL(
    `/api/dashboard/devices/${handle}/bootstrap${options.query ?? ""}`,
    cloudOrigin,
  );
  return {
    url,
    params: { device_handle: handle },
    request: new Request(url, {
      method: "POST",
      headers: options.origin === undefined ? undefined : { Origin: options.origin },
      body: options.body,
    }),
    platform: {
      env: {
        DB: options.binding ?? database.binding,
        JWT_SECRET: jwtSecret,
        AUTH_ORIGIN: cloudOrigin,
      },
    },
    cookies: { get: (name: string) => (name === "_at" ? options.token : undefined) },
  } as unknown as RequestEvent;
}

function redeemEvent(body: unknown, options: { credential?: string; binding?: unknown } = {}) {
  const url = new URL("/api/edge/dashboard-tickets/redeem", cloudOrigin);
  return {
    url,
    params: {},
    request: new Request(url, {
      method: "POST",
      headers: {
        ...(options.credential ? { Authorization: `Bearer ${options.credential}` } : undefined),
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    }),
    platform: { env: { DB: options.binding ?? database.binding } },
  } as unknown as RequestEvent;
}

async function seedFixtureAndLogin(tunnelUrl = "wss://brave-atlas.edge.pontia.dev/tunnel") {
  await database.db.insert(users).values([{ id: userId }, { id: otherUserId }]);
  await database.db.insert(authSessions).values({
    id: sessionId,
    userId,
    kind: "browser",
    expiresAt: "2099-01-01T00:00:00.000Z",
  });
  await database.db.insert(edges).values({
    id: edgeId,
    userId,
    name: "Owner edge",
    tunnelUrl,
    serviceCredentialHash: await sha256Base64url(edgeSecret),
  });
  await database.db.insert(devices).values([
    {
      id: deviceId,
      userId,
      edgeId,
      handle: "office-mac",
      name: "Office Mac",
    },
    {
      id: "0195e7d5-1b22-7c33-9d44-123456789abc",
      userId: otherUserId,
      edgeId,
      handle: "other-device",
      name: "Other device",
    },
  ]);
  return (await issueLogin(database.db, sessionId, jwtSecret)).token;
}

function ticketFromHtml(html: string) {
  const match = /name="ticket" value="(pet_v1_[A-Za-z0-9_-]{43})"/.exec(html);
  if (!match) throw new Error("Expected a dashboard ticket form field");
  return match[1];
}

test("launch returns a no-store nonce-authorized form to the derived edge bootstrap", async () => {
  const token = await seedFixtureAndLogin();
  const response = await callLaunch(launchEvent("office-mac", { origin: dashboardOrigin, token }));

  expect(response.status).toBe(200);
  expect(response.headers.get("cache-control")).toBe("no-store");
  expect(response.headers.get("referrer-policy")).toBe("origin");
  expect(response.headers.get("content-type")).toBe("text/html; charset=utf-8");
  const csp = response.headers.get("content-security-policy") ?? "";
  expect(csp).toContain("form-action https://brave-atlas.edge.pontia.dev https://app.pontia.dev");
  expect(csp).toContain("script-src 'nonce-");
  expect(csp).not.toContain("unsafe-inline");

  const html = await response.text();
  expect(html).toContain(
    'method="post" action="https://brave-atlas.edge.pontia.dev/dashboard/bootstrap"',
  );
  expect(html).toContain('document.getElementById("dashboard-bootstrap").submit()');
  expect(html).not.toContain(token);
  expect(ticketFromHtml(html)).toMatch(/^pet_v1_[A-Za-z0-9_-]{43}$/);
});

test("custom endpoint port survives bootstrap form action and CSP", async () => {
  const token = await seedFixtureAndLogin("wss://brave-atlas.edge.pontia.dev:8443/tunnel");
  const response = await callLaunch(launchEvent("office-mac", { origin: dashboardOrigin, token }));
  expect(response.status).toBe(200);
  expect(response.headers.get("content-security-policy")).toContain(
    "form-action https://brave-atlas.edge.pontia.dev:8443 https://app.pontia.dev",
  );
  expect(await response.text()).toContain(
    'action="https://brave-atlas.edge.pontia.dev:8443/dashboard/bootstrap"',
  );
});

test("launch requires an active login and an exact approved Origin", async () => {
  const token = await seedFixtureAndLogin();

  const unauthenticated = await callLaunch(launchEvent("office-mac", { origin: dashboardOrigin }));
  expect(unauthenticated.status).toBe(401);

  for (const origin of [undefined, "https://evil.example", `${dashboardOrigin}/`]) {
    const response = await callLaunch(launchEvent("office-mac", { origin, token }));
    expect(response.status).toBe(403);
    expect(await response.text()).not.toContain(token);
  }

  const cloudRequest = await callLaunch(launchEvent("office-mac", { origin: cloudOrigin, token }));
  expect(cloudRequest.status).toBe(200);

  const queryOverride = await callLaunch(
    launchEvent("office-mac", {
      origin: dashboardOrigin,
      token,
      query: "?edge_origin=https://evil.example",
    }),
  );
  expect(queryOverride.status).toBe(400);

  const bodyOverride = await callLaunch(
    launchEvent("office-mac", {
      origin: dashboardOrigin,
      token,
      body: new URLSearchParams({ device_id: deviceId }),
    }),
  );
  expect(bodyOverride.status).toBe(400);
});

test("launch does not reveal missing or unowned handles and fails closed on bad tunnel data", async () => {
  const token = await seedFixtureAndLogin();
  const unowned = await callLaunch(launchEvent("other-device", { origin: dashboardOrigin, token }));
  const missing = await callLaunch(
    launchEvent("missing-device", { origin: dashboardOrigin, token }),
  );
  expect(unowned.status).toBe(404);
  expect(missing.status).toBe(404);
  expect(await unowned.text()).toBe(await missing.text());

  await database.db.update(edges).set({ tunnelUrl: "wss://attacker.example/tunnel" });
  const invalidEdge = await callLaunch(
    launchEvent("office-mac", { origin: dashboardOrigin, token }),
  );
  expect(invalidEdge.status).toBe(404);
});

test("the edge-only redemption endpoint accepts only a ticket and returns minimal fields", async () => {
  const token = await seedFixtureAndLogin();
  const launchResponse = await callLaunch(
    launchEvent("office-mac", { origin: dashboardOrigin, token }),
  );
  const ticket = ticketFromHtml(await launchResponse.text());

  const unauthorized = await callRedeem(redeemEvent({ ticket }));
  expect(unauthorized.status).toBe(401);
  expect(await unauthorized.text()).not.toContain(ticket);

  const extraField = await callRedeem(
    redeemEvent({ ticket, device_id: deviceId }, { credential: edgeCredential }),
  );
  expect(extraField.status).toBe(401);
  expect(await extraField.text()).not.toContain(ticket);

  const redeemed = await callRedeem(redeemEvent({ ticket }, { credential: edgeCredential }));
  expect(redeemed.status).toBe(200);
  const body = (await redeemed.json()) as Record<string, string>;
  expect(Object.keys(body).sort()).toEqual(["device_handle", "device_id", "expires_at"]);
  expect(body.device_id).toBe(deviceId);
  expect(body.device_handle).toBe("office-mac");
  expect(Date.parse(body.expires_at)).toBeGreaterThan(Date.now());
  expect(JSON.stringify(body)).not.toContain(ticket);
  expect(JSON.stringify(body)).not.toContain(edgeCredential);

  const replay = await callRedeem(redeemEvent({ ticket }, { credential: edgeCredential }));
  expect(replay.status).toBe(401);
  expect(await replay.text()).not.toContain(ticket);
});

test("dashboard authorization endpoints map database failures without leaking credentials", async () => {
  const token = await seedFixtureAndLogin();
  const unavailableBinding = {};
  const launchResponse = await callLaunch(
    launchEvent("office-mac", {
      origin: dashboardOrigin,
      token,
      binding: unavailableBinding,
    }),
  );
  expect(launchResponse.status).toBe(503);
  expect(await launchResponse.text()).not.toContain(token);

  const redeemResponse = await callRedeem(
    redeemEvent(
      { ticket: "pet_v1_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" },
      {
        credential: edgeCredential,
        binding: unavailableBinding,
      },
    ),
  );
  expect(redeemResponse.status).toBe(503);
  expect(await redeemResponse.text()).not.toContain(edgeCredential);
});
