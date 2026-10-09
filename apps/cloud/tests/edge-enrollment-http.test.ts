import { expect, spyOn, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { base64url } from "jose";
import { POST as enroll } from "../src/routes/api/edge/enroll/+server";
import { GET as me } from "../src/routes/api/edge/me/+server";
import {
  issueEdgeDeployment,
  type DeploymentDependencies,
} from "../src/lib/server/edge-deployment";
import { edgeTickets, edges, users } from "../src/lib/server/db/schema";
import { testDatabase } from "./database";

const database = testDatabase();
const callEnroll = enroll as unknown as (event: RequestEvent) => Promise<Response>;
const callMe = me as unknown as (event: RequestEvent) => Promise<Response>;
const edgeId = "0199791c-6600-7000-8000-000000000001";
const credential = `pec_v1_${edgeId}_${base64url.encode(new Uint8Array(32).fill(52))}`;

function event(
  path: string,
  options: { method?: string; token?: string; body?: unknown; protocol?: "http" | "https" } = {},
) {
  const url = new URL(path, `${options.protocol ?? "https"}://pontia.example`);
  return {
    url,
    request: new Request(url, {
      method: options.method ?? "GET",
      headers: options.token
        ? { Authorization: `Bearer ${options.token}`, "Content-Type": "application/json" }
        : options.body === undefined
          ? undefined
          : { "Content-Type": "application/json" },
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
    }),
    platform: { env: { DB: database.binding } },
  } as unknown as RequestEvent;
}

async function ticket() {
  await database.db.insert(users).values({ id: "user-http" });
  const dependencies: DeploymentDependencies = {
    now: () => new Date("2099-01-01T00:00:00.000Z"),
    randomBytes: () => new Uint8Array(32).fill(51),
    edgeId: () => edgeId,
    heroName: async () => "silent-orion",
  };
  const deployment = await issueEdgeDeployment(database.db, "user-http", dependencies);
  const match = /--ticket '(pet_v1_[A-Za-z0-9_-]{43})'/.exec(deployment.command);
  if (!match) throw new Error("Expected ticket");
  return match[1];
}

test("edge enrollment requires HTTPS and accepts only ticket and credential", async () => {
  const deploymentTicket = await ticket();
  const insecure = await callEnroll(
    event("/api/edge/enroll", {
      method: "POST",
      protocol: "http",
      body: { ticket: deploymentTicket, service_credential: credential },
    }),
  );
  expect(insecure.status).toBe(400);

  const extra = await callEnroll(
    event("/api/edge/enroll", {
      method: "POST",
      body: {
        ticket: deploymentTicket,
        service_credential: credential,
        user_id: "attacker",
      },
    }),
  );
  expect(extra.status).toBe(401);

  const info = spyOn(console, "info").mockImplementation(() => undefined);
  const response = await callEnroll(
    event("/api/edge/enroll", {
      method: "POST",
      body: { ticket: deploymentTicket, service_credential: credential },
    }),
  );
  const calls = [...info.mock.calls];
  info.mockRestore();
  expect(response.status).toBe(200);
  expect((await response.json()) as Record<string, string>).toEqual({
    edge_id: edgeId,
    name: "silent-orion",
    tunnel_url: "wss://silent-orion.edge.pontia.dev/tunnel",
  });
  expect(calls).toContainEqual([
    {
      event: "edge_enrollment_succeeded",
      stage: "enrollment",
      edge_id: edgeId,
      hostname: "silent-orion.edge.pontia.dev",
    },
  ]);
  expect(JSON.stringify(calls)).not.toContain(deploymentTicket);
  expect(JSON.stringify(calls)).not.toContain(credential);
});

test("enrollment does not establish the long-lived edge identity", async () => {
  const deploymentTicket = await ticket();
  await callEnroll(
    event("/api/edge/enroll", {
      method: "POST",
      body: { ticket: deploymentTicket, service_credential: credential },
    }),
  );

  expect((await callMe(event("/api/edge/me"))).status).toBe(401);
  expect((await callMe(event("/api/edge/me", { token: credential }))).status).toBe(401);
  expect(await database.db.select().from(edges)).toHaveLength(0);
  expect((await database.db.select().from(edgeTickets).get())?.consumedAt).toBeNull();
});
