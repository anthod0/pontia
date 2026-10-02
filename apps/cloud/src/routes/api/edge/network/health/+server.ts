import { isEdgePort } from "../../../../../../../../shared/edge-port";
import { json } from "@sveltejs/kit";
import { confirmEdgeDeployment, type DeploymentIdentity } from "$lib/server/edge-deployment";
import { logDeploymentEvent } from "$lib/server/deployment-observability";
import { edgeHostname, verifyEdgeHealth } from "$lib/server/edge-network";
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
    return json({ error: "invalid_health_confirmation" }, { status: 400 });
  }

  const verification: { identity?: DeploymentIdentity } = {};
  try {
    const input = body as Record<string, unknown>;
    if (
      Object.keys(input).length !== 3 ||
      !isEdgePort(input.port) ||
      typeof input.ticket !== "string" ||
      typeof input.service_credential !== "string"
    ) {
      return json({ error: "invalid_health_confirmation" }, { status: 400 });
    }

    const result = await confirmEdgeDeployment(
      remoteDatabase(event),
      input.ticket,
      input.service_credential,
      async (identity) => {
        const healthy = await verifyEdgeHealth(identity, fetch);
        logDeploymentEvent(healthy ? "info" : "warn", {
          event: healthy ? "edge_health_verification_succeeded" : "edge_health_verification_failed",
          stage: "health_verification",
          edge_id: identity.edgeId,
          hostname: edgeHostname(identity.tunnelUrl),
          ...(healthy ? {} : { error: "health_verification_failed" }),
        });
        if (healthy) verification.identity = identity;
        return healthy;
      },
      input.port,
    );
    if (result.status === "invalid") {
      if (verification.identity) {
        logDeploymentEvent("error", {
          event: "edge_registration_failed",
          stage: "registration_commit",
          edge_id: verification.identity.edgeId,
          hostname: edgeHostname(verification.identity.tunnelUrl),
          error: "registration_commit_failed",
        });
      }
      return json({ error: "invalid_deployment_authorization" }, { status: 401 });
    }
    if (result.status === "unhealthy") {
      return json({ error: "health_verification_failed" }, { status: 422 });
    }
    logDeploymentEvent("info", {
      event: "edge_registration_succeeded",
      stage: "registration_commit",
      edge_id: result.edge.edgeId,
      hostname: edgeHostname(result.edge.tunnelUrl),
      result: result.status,
    });
    return json({ hostname: new URL(result.edge.tunnelUrl).hostname });
  } catch {
    logDeploymentEvent("error", {
      event: verification.identity ? "edge_registration_failed" : "edge_health_verification_failed",
      stage: verification.identity ? "registration_commit" : "deployment_authorization",
      ...(verification.identity
        ? {
            edge_id: verification.identity.edgeId,
            hostname: edgeHostname(verification.identity.tunnelUrl),
          }
        : {}),
      error: "unexpected_error",
    });
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
