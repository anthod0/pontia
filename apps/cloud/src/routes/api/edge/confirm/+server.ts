import { json } from "@sveltejs/kit";
import { isEdgePort } from "$lib/edge-port";
import { environment } from "$lib/server/auth/http";
import { logDeploymentEvent } from "$lib/server/deployment-observability";
import { confirmEdgeDeployment, type OwnershipVerification } from "$lib/server/edge-deployment";
import { resolvesToPublicAddresses, verifyEdgeOwnership } from "$lib/server/edge-network";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

const failures = {
  invalid: [401, "invalid_deployment_authorization"],
  conflict: [409, "endpoint_already_registered"],
  non_public_ip: [422, "non_public_ip"],
  failed: [422, "ownership_verification_failed"],
  rate_limited: [429, "rate_limited"],
} as const;

export const POST: RequestHandler = async (event) => {
  if (event.url.protocol !== "https:") {
    return json({ error: "https_required" }, { status: 400 });
  }

  let body: unknown;
  try {
    body = await event.request.json();
  } catch {
    return json({ error: "invalid_confirmation" }, { status: 400 });
  }
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    return json({ error: "invalid_confirmation" }, { status: 400 });
  }
  const input = body as Record<string, unknown>;
  if (
    Object.keys(input).length !== 3 ||
    !isEdgePort(input.port) ||
    typeof input.ticket !== "string" ||
    typeof input.service_credential !== "string"
  ) {
    return json({ error: "invalid_confirmation" }, { status: 400 });
  }

  const env = environment(event);
  let stage = "deployment_authorization";
  try {
    const result = await confirmEdgeDeployment(
      remoteDatabase(event),
      input.ticket,
      input.service_credential,
      async (candidate): Promise<OwnershipVerification> => {
        if (!(await env.EDGE_CONFIRM_RATE_LIMIT.limit({ key: candidate.edgeId })).success) {
          return "rate_limited";
        }
        stage = "dns_verification";
        if ((await resolvesToPublicAddresses(candidate.hostname, fetch)) !== "public") {
          return "non_public_ip";
        }
        stage = "ownership_verification";
        const verified = await verifyEdgeOwnership(
          candidate,
          env.E2E_CAPABILITY_SIGNING_KEY,
          fetch,
        );
        stage = "registration_commit";
        return verified ? "verified" : "failed";
      },
      input.port,
    );
    if (result.status === "created" || result.status === "existing") {
      const hostname = new URL(result.edge.tunnelUrl).hostname;
      logDeploymentEvent("info", {
        event: "edge_registration_succeeded",
        stage: "registration_commit",
        edge_id: result.edge.edgeId,
        hostname,
        result: result.status,
      });
      return json({
        edge_id: result.edge.edgeId,
        hostname,
        tunnel_url: result.edge.tunnelUrl,
      });
    }
    const [status, error] = failures[result.status];
    logDeploymentEvent(status === 401 ? "error" : "warn", {
      event: "edge_registration_failed",
      stage,
      ...("edge" in result ? { edge_id: result.edge.edgeId, hostname: result.edge.name } : {}),
      error,
    });
    return json({ error }, { status });
  } catch {
    logDeploymentEvent("error", {
      event: "edge_registration_failed",
      stage,
      error: "unexpected_error",
    });
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
