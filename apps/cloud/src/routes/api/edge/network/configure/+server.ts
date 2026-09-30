import { json } from "@sveltejs/kit";
import { environment } from "$lib/server/auth/http";
import {
  CloudflareDnsProvider,
  configureEdgeNetwork,
  edgeNetworkIdentity,
} from "$lib/server/edge-network";
import { bearerCredential, remoteDatabase } from "$lib/server/remote-access/http";
import { authenticateEdgeCredential } from "$lib/server/remote-access/resources";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  try {
    const credential = bearerCredential(event.request);
    if (!credential) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const db = remoteDatabase(event);
    const principal = await authenticateEdgeCredential(db, credential);
    if (!principal) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const env = environment(event);
    const allowed = await env.EDGE_NETWORK_RATE_LIMIT.limit({ key: principal.edgeId });
    if (!allowed.success) return json({ error: "rate_limited" }, { status: 429 });

    const body = (await event.request.json()) as unknown;
    if (
      !body ||
      typeof body !== "object" ||
      Array.isArray(body) ||
      Object.keys(body).length !== 1 ||
      typeof (body as Record<string, unknown>).candidate_ipv4 !== "string"
    ) {
      return json({ error: "invalid_network_configuration" }, { status: 400 });
    }
    const identity = await edgeNetworkIdentity(db, principal.edgeId);
    if (!identity) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const { connect } = await import("cloudflare:sockets");
    const result = await configureEdgeNetwork(
      identity,
      (body as { candidate_ipv4: string }).candidate_ipv4,
      {
        randomBytes: (length) => crypto.getRandomValues(new Uint8Array(length)),
        connect,
        dns: new CloudflareDnsProvider(env.CLOUDFLARE_DNS_TOKEN, env.CLOUDFLARE_DNS_ZONE_ID),
      },
    );
    if (result.status === "invalid") {
      return json({ error: "invalid_network_configuration" }, { status: 400 });
    }
    if (result.status === "unreachable") {
      return json({ error: "address_verification_failed" }, { status: 422 });
    }
    return json({ hostname: result.hostname });
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
