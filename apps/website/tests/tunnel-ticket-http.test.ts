import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { base64url } from "jose";
import { POST as redeem } from "../src/routes/api/edge/tunnel-tickets/redeem/+server";
import { POST as issue } from "../src/routes/api/remote/devices/[device_id]/tunnel-tickets/+server";
import { sha256Base64url } from "../src/lib/server/crypto";
import { authSessions, devices, edges, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const callIssue = issue as unknown as (event: RequestEvent) => Promise<Response>;
const callRedeem = redeem as unknown as (event: RequestEvent) => Promise<Response>;
const database = testDatabase();
const sessionId = "0195e7d1-1b22-7c33-9d44-123456789abc";
const deviceId = "0195e7d3-1b22-7c33-9d44-123456789abc";
const edgeId = "0195e7d2-1b22-7c33-9d44-123456789abc";
const cliSecret = base64url.encode(new Uint8Array(32).fill(31));
const edgeSecret = base64url.encode(new Uint8Array(32).fill(32));
const cliCredential = `ptr_v1_${sessionId}_${cliSecret}`;
const edgeCredential = `pec_v1_${edgeId}_${edgeSecret}`;

async function responseBody(response: Response) {
  return (await response.json()) as Record<string, string>;
}

function event(path: string, options: { token?: string; body?: unknown; binding?: unknown } = {}) {
  const url = new URL(path, "https://example.com");
  return {
    url,
    params: { device_id: deviceId },
    request: new Request(url, {
      method: "POST",
      headers: options.token
        ? {
            Authorization: `Bearer ${options.token}`,
            "Content-Type": "application/json",
          }
        : undefined,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
    }),
    platform: { env: { DB: options.binding ?? database.binding } },
  } as unknown as RequestEvent;
}

async function seedHttpTicketFixture() {
  await database.db.insert(users).values({ id: "user-http" });
  await database.db.insert(authSessions).values({
    id: sessionId,
    userId: "user-http",
    kind: "cli",
    tokenHash: await sha256Base64url(cliSecret),
  });
  await database.db.insert(edges).values({
    id: edgeId,
    userId: "user-http",
    name: "HTTP Edge",
    tunnelUrl: "wss://edge-http.example/tunnel",
    serviceCredentialHash: await sha256Base64url(edgeSecret),
  });
  await database.db.insert(devices).values({
    id: deviceId,
    userId: "user-http",
    edgeId,
    handle: "http-device",
  });
}

async function issueTicket() {
  const response = await callIssue(
    event(`/api/remote/devices/${deviceId}/tunnel-tickets`, {
      token: cliCredential,
    }),
  );
  expect(response.status).toBe(200);
  return (await response.json()) as {
    ticket: string;
    tunnel_url: string;
    expires_at: string;
  };
}

test("ticket issue endpoint authenticates the CLI owner and returns its edge target", async () => {
  await seedHttpTicketFixture();

  const unauthenticated = await callIssue(event(`/api/remote/devices/${deviceId}/tunnel-tickets`));
  expect(unauthenticated.status).toBe(401);
  expect(await responseBody(unauthenticated)).toEqual({
    error: "invalid_credentials",
  });

  const result = await issueTicket();
  expect(result.ticket).toMatch(/^pet_v1_[A-Za-z0-9_-]{43}$/);
  expect(result.tunnel_url).toBe("wss://edge-http.example/tunnel");
  expect(Date.parse(result.expires_at)).toBeGreaterThan(Date.now());
});

test("ticket issue endpoint does not reveal devices owned by another user", async () => {
  await seedHttpTicketFixture();
  await database.db.insert(users).values({ id: "user-other" });
  const otherSession = "0195e7d4-1b22-7c33-9d44-123456789abc";
  const otherSecret = base64url.encode(new Uint8Array(32).fill(33));
  await database.db.insert(authSessions).values({
    id: otherSession,
    userId: "user-other",
    kind: "cli",
    tokenHash: await sha256Base64url(otherSecret),
  });

  const response = await callIssue(
    event(`/api/remote/devices/${deviceId}/tunnel-tickets`, {
      token: `ptr_v1_${otherSession}_${otherSecret}`,
    }),
  );

  expect(response.status).toBe(404);
  expect(await responseBody(response)).toEqual({ error: "device_not_found" });
});

test("redeem endpoint authenticates the edge and returns only the trusted device ID", async () => {
  await seedHttpTicketFixture();
  const issued = await issueTicket();

  const invalidEdge = await callRedeem(
    event("/api/edge/tunnel-tickets/redeem", {
      token: `pec_v1_${edgeId}_${base64url.encode(new Uint8Array(32).fill(34))}`,
      body: { ticket: issued.ticket },
    }),
  );
  expect(invalidEdge.status).toBe(401);
  expect(await responseBody(invalidEdge)).toEqual({
    error: "invalid_edge_credentials",
  });

  const redeemed = await callRedeem(
    event("/api/edge/tunnel-tickets/redeem", {
      token: edgeCredential,
      body: { ticket: issued.ticket },
    }),
  );
  expect(redeemed.status).toBe(200);
  expect(await responseBody(redeemed)).toEqual({ device_id: deviceId });

  const replayed = await callRedeem(
    event("/api/edge/tunnel-tickets/redeem", {
      token: edgeCredential,
      body: { ticket: issued.ticket },
    }),
  );
  expect(replayed.status).toBe(401);
  expect(await responseBody(replayed)).toEqual({
    error: "invalid_tunnel_ticket",
  });
});

test("redeem endpoint maps malformed requests to the stable ticket error", async () => {
  await seedHttpTicketFixture();

  for (const body of [{}, { ticket: "invalid" }, { ticket: "invalid", edge_id: edgeId }]) {
    const response = await callRedeem(
      event("/api/edge/tunnel-tickets/redeem", {
        token: edgeCredential,
        body,
      }),
    );
    expect(response.status).toBe(401);
    expect(await responseBody(response)).toEqual({
      error: "invalid_tunnel_ticket",
    });
  }
});

test("ticket endpoints map database failures to service_unavailable", async () => {
  const unavailableBinding = {};
  const issueResponse = await callIssue(
    event(`/api/remote/devices/${deviceId}/tunnel-tickets`, {
      token: cliCredential,
      binding: unavailableBinding,
    }),
  );
  const redeemResponse = await callRedeem(
    event("/api/edge/tunnel-tickets/redeem", {
      token: edgeCredential,
      body: { ticket: "invalid" },
      binding: unavailableBinding,
    }),
  );

  expect(issueResponse.status).toBe(503);
  expect(await responseBody(issueResponse)).toEqual({
    error: "service_unavailable",
  });
  expect(redeemResponse.status).toBe(503);
  expect(await responseBody(redeemResponse)).toEqual({
    error: "service_unavailable",
  });
});
