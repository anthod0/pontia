import { json } from "@sveltejs/kit";
import { cliPrincipal, remoteDatabase } from "$lib/server/remote-access/http";
import { isUuidV7 } from "$lib/server/uuid";
import {
  bindAutomaticEdge,
  tunnelCandidates,
  issueTunnelTicket,
} from "$lib/server/remote-access/tickets";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  try {
    const principal = await cliPrincipal(event);
    if (!principal) return json({ error: "invalid_credentials" }, { status: 401 });
    const db = remoteDatabase(event);
    const deviceId = event.params.device_id;
    const selection = new URL(event.request.url).searchParams.get("edge_id");
    if (selection === "auto") {
      const candidates = await tunnelCandidates(db, principal.userId, deviceId);
      if (candidates) return json({ candidates });
    } else if (selection !== null) {
      if (!isUuidV7(selection)) return json({ error: "invalid_request" }, { status: 400 });
      if (!(await bindAutomaticEdge(db, principal.userId, deviceId, selection)))
        return json({ error: "selection_conflict" }, { status: 409 });
    }
    const result = await issueTunnelTicket(db, principal.userId, deviceId);
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
