import { json } from "@sveltejs/kit";
import { cliPrincipal, remoteDatabase } from "$lib/server/remote-access/http";
import { issueTunnelTicket } from "$lib/server/remote-access/tickets";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  try {
    const principal = await cliPrincipal(event);
    if (!principal) return json({ error: "invalid_credentials" }, { status: 401 });
    const result = await issueTunnelTicket(
      remoteDatabase(event),
      principal.userId,
      event.params.device_id,
    );
    if (result.status === "device_not_found")
      return json({ error: result.status }, { status: 404 });
    return json({
      ticket: result.value.ticket,
      tunnel_url: result.value.tunnelUrl,
      expires_at: result.value.expiresAt,
    });
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
