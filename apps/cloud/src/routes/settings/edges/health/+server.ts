import { json } from "@sveltejs/kit";
import { eq, or } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { edges } from "$lib/server/db/schema";
import { probeEdgeHealth } from "$lib/server/edge-network";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async (event) => {
  const user = await currentLogin(event);
  if (!user) return json({ error: "authentication_required" }, { status: 401 });
  const env = environment(event);
  if (!(await env.EDGE_HEALTH_RATE_LIMIT.limit({ key: user.user_id })).success) {
    return json({ error: "rate_limited" }, { status: 429 });
  }

  const visibleEdges = await database(env.DB)
    .select({ id: edges.id, tunnelUrl: edges.tunnelUrl })
    .from(edges)
    .where(or(eq(edges.userId, user.user_id), eq(edges.accessScope, "public")));

  const results = await Promise.all(
    visibleEdges.map(async (edge) => [edge.id, await probeEdgeHealth(edge, fetch)] as const),
  );

  return json({ edges: Object.fromEntries(results) }, { headers: { "Cache-Control": "no-store" } });
};
