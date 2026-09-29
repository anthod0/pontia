import { json } from "@sveltejs/kit";
import { enrollEdge } from "$lib/server/edge-deployment";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  if (event.url.protocol !== "https:") {
    return json({ error: "https_required" }, { status: 400 });
  }

  let parsed: unknown;
  try {
    parsed = await event.request.json();
  } catch {
    return json({ error: "invalid_enrollment" }, { status: 401 });
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return json({ error: "invalid_enrollment" }, { status: 401 });
  }
  const body = parsed as Record<string, unknown>;
  if (
    typeof body.ticket !== "string" ||
    typeof body.service_credential !== "string" ||
    Object.keys(body).some((key) => key !== "ticket" && key !== "service_credential")
  ) {
    return json({ error: "invalid_enrollment" }, { status: 401 });
  }

  try {
    const result = await enrollEdge(remoteDatabase(event), body.ticket, body.service_credential);
    if (result.status === "invalid") {
      return json({ error: "invalid_enrollment" }, { status: 401 });
    }
    return json({
      edge_id: result.edge.edgeId,
      name: result.edge.name,
      tunnel_url: result.edge.tunnelUrl,
    });
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
