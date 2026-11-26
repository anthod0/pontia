import { gt } from "drizzle-orm";
import type { Database } from "./db";
import { edges } from "./db/schema";
import { edgeApiOrigin, probeEdgeHealth } from "./edge-network";
import { capabilitySigningKey, signEdgeControlToken } from "./edge-auth";

// Each page fits within the Worker's concurrent outbound connection limit.
const pageSize = 5;

export async function notifyEdgeUpdates(
  db: Database,
  after: string,
  version: string,
  signingKey: string,
  fetcher: (input: string, init: RequestInit) => Promise<Response>,
) {
  const page = await db
    .select({ id: edges.id, tunnelUrl: edges.tunnelUrl })
    .from(edges)
    .where(gt(edges.id, after))
    .orderBy(edges.id)
    .limit(pageSize);
  const key = await capabilitySigningKey(signingKey);
  const outcomes = await Promise.all(
    page.map(async (edge) => {
      const health = await probeEdgeHealth(edge, key, fetcher);
      if (health.status !== "healthy" || health.version === version || !health.updateNonce)
        return "skipped";
      const token = await signEdgeControlToken(key, {
        purpose: "update",
        edgeId: edge.id,
        nonce: health.updateNonce,
      });
      try {
        const response = await fetcher(edgeApiOrigin(edge.tunnelUrl) + "/__pontia/notify-update", {
          method: "POST",
          redirect: "error",
          signal: AbortSignal.timeout(5_000),
          headers: { Authorization: "Bearer " + token },
        });
        await response.body?.cancel();
        if (response.status === 202) return "accepted";
        if (response.status === 204) return "skipped";
      } catch {
        // Offline after the health check, or exceeded the notification timeout.
      }
      console.warn({ event: "edge_update_notification_failed", edge_id: edge.id });
      return "failed";
    }),
  );
  return {
    next: page.length === pageSize ? page[page.length - 1].id : null,
    accepted: outcomes.filter((result) => result === "accepted").length,
    skipped: outcomes.filter((result) => result === "skipped").length,
    failed: outcomes.filter((result) => result === "failed").length,
  };
}
