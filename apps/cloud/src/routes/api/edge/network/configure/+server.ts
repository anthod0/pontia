import { isEdgePort } from "../../../../../../../../shared/edge-port";
import { json } from "@sveltejs/kit";
import { environment } from "$lib/server/auth/http";
import { authorizeEdgeDeployment, type DeploymentIdentity } from "$lib/server/edge-deployment";
import { DnsProviderError } from "$lib/server/cloudflare-dns-errors";
import { logDeploymentEvent } from "$lib/server/deployment-observability";
import {
  CloudflareDnsProvider,
  configureEdgeNetwork,
  edgeHostname,
  validDnsChallenge,
} from "$lib/server/edge-network";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  if (event.url.protocol !== "https:") {
    return json({ error: "https_required" }, { status: 400 });
  }

  let body: unknown;
  try {
    body = await event.request.json();
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    return json({ error: "invalid_network_configuration" }, { status: 400 });
  }

  let trustedIdentity: DeploymentIdentity | null = null;
  try {
    const input = body as Record<string, unknown>;
    if (
      Object.keys(input).some(
        (key) =>
          !["ticket", "service_credential", "candidate_ipv4", "port", "dns_challenge"].includes(
            key,
          ),
      ) ||
      !isEdgePort(input.port) ||
      (input.dns_challenge !== undefined && !validDnsChallenge(input.dns_challenge)) ||
      typeof input.ticket !== "string" ||
      typeof input.service_credential !== "string" ||
      typeof input.candidate_ipv4 !== "string"
    ) {
      return json({ error: "invalid_network_configuration" }, { status: 400 });
    }

    const db = remoteDatabase(event);
    const identity = await authorizeEdgeDeployment(db, input.ticket, input.service_credential);
    if (!identity) return json({ error: "invalid_deployment_authorization" }, { status: 401 });
    trustedIdentity = identity;

    const env = environment(event);
    const allowed = await env.EDGE_NETWORK_RATE_LIMIT.limit({ key: identity.edgeId });
    if (!allowed.success) return json({ error: "rate_limited" }, { status: 429 });

    const { connect } = await import("cloudflare:sockets");
    const dns = new CloudflareDnsProvider(env.CLOUDFLARE_DNS_TOKEN, env.CLOUDFLARE_DNS_ZONE_ID);
    const result = await configureEdgeNetwork(
      identity,
      input.candidate_ipv4,
      {
        randomBytes: (length) => crypto.getRandomValues(new Uint8Array(length)),
        connect,
        dns,
      },
      input.port,
      input.dns_challenge as string | undefined,
    );
    if (result.status === "invalid") {
      return json({ error: "invalid_network_configuration" }, { status: 400 });
    }
    if (result.status === "unreachable") {
      logDeploymentEvent("warn", {
        event: "edge_network_configuration_failed",
        stage: "address_verification",
        edge_id: identity.edgeId,
        hostname: edgeHostname(identity.tunnelUrl),
        candidate_ipv4: input.candidate_ipv4,
        error: "address_verification_failed",
      });
      return json({ error: "address_verification_failed" }, { status: 422 });
    }
    logDeploymentEvent("info", {
      event: "edge_network_configuration_succeeded",
      stage: "dns_configuration",
      edge_id: identity.edgeId,
      hostname: result.hostname,
      candidate_ipv4: input.candidate_ipv4,
    });
    return json({ hostname: result.hostname });
  } catch (error) {
    logDeploymentEvent("error", {
      event: "edge_network_configuration_failed",
      stage:
        error instanceof DnsProviderError
          ? "dns_provider"
          : trustedIdentity
            ? "network_configuration"
            : "deployment_authorization",
      ...(trustedIdentity
        ? {
            edge_id: trustedIdentity.edgeId,
            hostname: edgeHostname(trustedIdentity.tunnelUrl),
          }
        : {}),
      ...(error instanceof DnsProviderError ? error.fields() : { error: "unexpected_error" }),
    });
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
