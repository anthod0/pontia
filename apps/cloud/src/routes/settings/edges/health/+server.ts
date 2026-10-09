import { json } from "@sveltejs/kit";
import { eq, or } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { edges } from "$lib/server/db/schema";
import { verifyEdgeHealth } from "$lib/server/edge-network";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async (event) => {
  const user = await currentLogin(event);
  if (!user) return json({ error: "authentication_required" }, { status: 401 });

  const visibleEdges = await database(environment(event).DB)
    .select({ id: edges.id, tunnelUrl: edges.tunnelUrl })
    .from(edges)
    .where(or(eq(edges.userId, user.user_id), eq(edges.accessScope, "public")));

  const results = await Promise.all(
    visibleEdges.map(async (edge) => [edge.id, await verifyEdgeHealth(edge, fetch)] as const),
  );

  return json(
    {
      edges: Object.fromEntries(
        results.map(([edgeId, healthy]) => [edgeId, healthy ? "healthy" : "unreachable"]),
      ),
    },
    { headers: { "Cache-Control": "no-store" } },
  );
};
