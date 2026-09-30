import { json } from "@sveltejs/kit";
import { environment } from "$lib/server/auth/http";
import { authorizeEdgeDeployment } from "$lib/server/edge-deployment";
import { CloudflareDnsProvider, configureEdgeNetwork } from "$lib/server/edge-network";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  if (event.url.protocol !== "https:") {
    return json({ error: "https_required" }, { status: 400 });
  }

  try {
    const body = (await event.request.json()) as unknown;
    if (!body || typeof body !== "object" || Array.isArray(body)) {
      return json({ error: "invalid_network_configuration" }, { status: 400 });
    }
    const input = body as Record<string, unknown>;
    if (
      Object.keys(input).length !== 3 ||
      typeof input.ticket !== "string" ||
      typeof input.service_credential !== "string" ||
      typeof input.candidate_ipv4 !== "string"
    ) {
      return json({ error: "invalid_network_configuration" }, { status: 400 });
    }

    const db = remoteDatabase(event);
    const identity = await authorizeEdgeDeployment(db, input.ticket, input.service_credential);
    if (!identity) return json({ error: "invalid_deployment_authorization" }, { status: 401 });

    const env = environment(event);
    const allowed = await env.EDGE_NETWORK_RATE_LIMIT.limit({ key: identity.edgeId });
    if (!allowed.success) return json({ error: "rate_limited" }, { status: 429 });

    const { connect } = await import("cloudflare:sockets");
    const result = await configureEdgeNetwork(identity, input.candidate_ipv4, {
      randomBytes: (length) => crypto.getRandomValues(new Uint8Array(length)),
      connect,
      dns: new CloudflareDnsProvider(env.CLOUDFLARE_DNS_TOKEN, env.CLOUDFLARE_DNS_ZONE_ID),
    });
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
