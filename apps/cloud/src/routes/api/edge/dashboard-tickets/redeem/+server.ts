import { json } from "@sveltejs/kit";
import { redeemDashboardAccess } from "$lib/server/remote-access/dashboard-access";
import { authenticateEdgeTicketRequest } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  try {
    const request = await authenticateEdgeTicketRequest(event);
    if (request.status === "invalid_edge_credentials") {
      return json({ error: "invalid_edge_credentials" }, { status: 401 });
    }
    if (request.status === "invalid_ticket") {
      return json({ error: "invalid_dashboard_ticket" }, { status: 401 });
    }

    const result = await redeemDashboardAccess(request.db, request.edgeId, request.ticket);
    if (!result) return json({ error: "invalid_dashboard_ticket" }, { status: 401 });
    return json({
      device_id: result.deviceId,
      device_handle: result.deviceHandle,
      expires_at: result.expiresAt,
    });
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
