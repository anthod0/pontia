import { and, eq } from "drizzle-orm";
import type { Database } from "./db";
import { devices, edges, edgeTickets } from "./db/schema";
import { edgeHostname } from "./edge-network";

export type EdgeDnsCleanup = {
  cleanupHostname(hostname: string): Promise<void>;
};

export async function removeOwnedEdge(
  db: Database,
  userId: string,
  edgeId: string,
  dns: EdgeDnsCleanup,
) {
  const edge = await db
    .select({ id: edges.id, tunnelUrl: edges.tunnelUrl })
    .from(edges)
    .where(and(eq(edges.id, edgeId), eq(edges.userId, userId)))
    .get();
  if (!edge) return { status: "not_found" as const };
  const hostname = edgeHostname(edge.tunnelUrl);
  if (!hostname) return { status: "invalid_edge" as const };

  // Keep the database registration available for a retry when external DNS cleanup fails.
  await dns.cleanupHostname(hostname);
  const [, , removed] = await db.batch([
    db.delete(devices).where(eq(devices.edgeId, edge.id)).returning({ id: devices.id }),
    db
      .delete(edgeTickets)
      .where(and(eq(edgeTickets.expectedEdgeId, edge.id), eq(edgeTickets.userId, userId)))
      .returning({ id: edgeTickets.id }),
    db
      .delete(edges)
      .where(and(eq(edges.id, edge.id), eq(edges.userId, userId)))
      .returning({ id: edges.id }),
  ]);
  return { status: removed.length === 1 ? ("deleted" as const) : ("not_found" as const) };
}
