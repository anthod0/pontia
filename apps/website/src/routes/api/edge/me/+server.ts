import { json } from "@sveltejs/kit";
import { bearerCredential, remoteDatabase } from "$lib/server/remote-access/http";
import { authenticateEdgeCredential } from "$lib/server/remote-access/resources";
import { edges } from "$lib/server/db/schema";
import { eq } from "drizzle-orm";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async (event) => {
  try {
    const credential = bearerCredential(event.request);
    if (!credential) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const db = remoteDatabase(event);
    const principal = await authenticateEdgeCredential(db, credential);
    if (!principal) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const edge = await db
      .select({ edgeId: edges.id, name: edges.name })
      .from(edges)
      .where(eq(edges.id, principal.edgeId))
      .get();
    if (!edge) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    return json({ edge_id: edge.edgeId, name: edge.name });
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
