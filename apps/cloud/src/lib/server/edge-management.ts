import { and, eq, inArray } from "drizzle-orm";
import type { Database } from "./db";
import { devices, edges, edgeTickets } from "./db/schema";
import { managedEdgeDns } from "./edge-network";

export type EdgeDnsCleanup = {
  cleanupHostname(hostname: string, label: string): Promise<void>;
};

export async function removeOwnedEdge(
  db: Database,
  userId: string,
  edgeId: string,
  dns: EdgeDnsCleanup,
  managesDns: boolean,
) {
  const ownedPrivateEdge = and(
    eq(edges.id, edgeId),
    eq(edges.userId, userId),
    eq(edges.accessScope, "private"),
  );
  const edge = await db
    .select({ tunnelUrl: edges.tunnelUrl })
    .from(edges)
    .where(ownedPrivateEdge)
    .get();
  if (!edge) return { status: "not_found" as const };
  // Only edges under the managed zone have Cloud-owned DNS records.
  if (managesDns) {
    const managedDns = managedEdgeDns(edge.tunnelUrl);
    if (managedDns) {
      // Keep the database registration available for a retry when external DNS cleanup fails.
      await dns.cleanupHostname(managedDns.hostname, managedDns.label);
    }
  }
  const removableId = db.select({ id: edges.id }).from(edges).where(ownedPrivateEdge);
  const [, , removed] = await db.batch([
    db.delete(devices).where(inArray(devices.edgeId, removableId)).returning({ id: devices.id }),
    db
      .delete(edgeTickets)
      .where(and(inArray(edgeTickets.expectedEdgeId, removableId), eq(edgeTickets.userId, userId)))
      .returning({ id: edgeTickets.id }),
    db.delete(edges).where(ownedPrivateEdge).returning({ id: edges.id }),
  ]);
  return { status: removed.length === 1 ? ("deleted" as const) : ("not_found" as const) };
}
