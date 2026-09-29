import { json } from "@sveltejs/kit";
import { edgeNetworkIdentity, verifyEdgeHealth } from "$lib/server/edge-network";
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
    const identity = await edgeNetworkIdentity(db, principal.edgeId);
    if (!identity) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    if (!(await verifyEdgeHealth(identity, fetch))) {
      return json({ error: "health_verification_failed" }, { status: 422 });
    }
    return json({ hostname: new URL(identity.tunnelUrl).hostname });
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
