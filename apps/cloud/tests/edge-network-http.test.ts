import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { base64url } from "jose";
import {
  issueEdgeDeployment,
  type DeploymentDependencies,
} from "../src/lib/server/edge-deployment";
import { edges, users } from "../src/lib/server/db/schema";
import { POST as configure } from "../src/routes/api/edge/network/configure/+server";
import { testDatabase } from "./database";

const database = testDatabase();
const callConfigure = configure as unknown as (event: RequestEvent) => Promise<Response>;
const edgeId = "0199791c-6600-7000-8000-000000000001";
const credential = `pec_v1_${edgeId}_${base64url.encode(new Uint8Array(32).fill(71))}`;

async function deploymentTicket() {
  await database.db.insert(users).values({ id: "network-owner" });
  const dependencies: DeploymentDependencies = {
    now: () => new Date("2099-01-01T00:00:00.000Z"),
    randomBytes: () => new Uint8Array(32).fill(72),
    edgeId: () => edgeId,
    heroName: async () => "brave-atlas",
  };
  const deployment = await issueEdgeDeployment(
    database.db,
    "network-owner",
    "https://pontia.example",
    dependencies,
  );
  const ticket = /--ticket '(pet_v1_[A-Za-z0-9_-]{43})'/.exec(deployment.command)?.[1];
  if (!ticket) throw new Error("Expected deployment ticket");
  return ticket;
}

function event(ticket: string, serviceCredential: string, allowed: boolean) {
  const url = new URL("https://pontia.example/api/edge/network/configure");
  return {
    url,
    request: new Request(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ticket,
        service_credential: serviceCredential,
        candidate_ipv4: "8.8.8.8",
      }),
    }),
    platform: {
      env: {
        DB: database.binding,
        CLOUDFLARE_DNS_TOKEN: "not-used",
        CLOUDFLARE_DNS_ZONE_ID: "not-used",
        EDGE_NETWORK_RATE_LIMIT: { limit: async () => ({ success: allowed }) },
      },
    },
  } as unknown as RequestEvent;
}

test("network configuration uses deployment authorization and its bound edge rate limit", async () => {
  const ticket = await deploymentTicket();

  expect((await callConfigure(event("invalid", credential, true))).status).toBe(401);
  const limited = await callConfigure(event(ticket, credential, false));
  expect(limited.status).toBe(429);
  expect((await limited.json()) as unknown).toEqual({ error: "rate_limited" });
  expect(await database.db.select().from(edges)).toHaveLength(0);
});
