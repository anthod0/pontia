import { json } from "@sveltejs/kit";
import { confirmEdgeDeployment } from "$lib/server/edge-deployment";
import { verifyEdgeHealth } from "$lib/server/edge-network";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  if (event.url.protocol !== "https:") {
    return json({ error: "https_required" }, { status: 400 });
  }

  try {
    const body = (await event.request.json()) as unknown;
    if (!body || typeof body !== "object" || Array.isArray(body)) {
      return json({ error: "invalid_health_confirmation" }, { status: 400 });
    }
    const input = body as Record<string, unknown>;
    if (
      Object.keys(input).length !== 2 ||
      typeof input.ticket !== "string" ||
      typeof input.service_credential !== "string"
    ) {
      return json({ error: "invalid_health_confirmation" }, { status: 400 });
    }

    const result = await confirmEdgeDeployment(
      remoteDatabase(event),
      input.ticket,
      input.service_credential,
      (identity) => verifyEdgeHealth(identity, fetch),
    );
    if (result.status === "invalid") {
      return json({ error: "invalid_deployment_authorization" }, { status: 401 });
    }
    if (result.status === "unhealthy") {
      return json({ error: "health_verification_failed" }, { status: 422 });
    }
    return json({ hostname: new URL(result.edge.tunnelUrl).hostname });
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
