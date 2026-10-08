import { fail, redirect, type RequestEvent } from "@sveltejs/kit";
import { and, asc, eq, ne } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { edges } from "$lib/server/db/schema";
import { removeOwnedEdge } from "$lib/server/edge-management";
import { CloudflareDnsProvider } from "$lib/server/edge-network";
import type { Actions, PageServerLoad } from "./$types";

async function activeUser(event: Pick<RequestEvent, "platform" | "cookies">) {
  const user = await currentLogin(event);
  if (!user) return null;
  const env = environment(event);
  return { user, env, db: database(env.DB) };
}

export const load: PageServerLoad = async (event) => {
  const authenticated = await activeUser(event);
  if (!authenticated) redirect(303, "/login");
  const edgeFields = {
    id: edges.id,
    name: edges.name,
    tunnelUrl: edges.tunnelUrl,
    createdAt: edges.createdAt,
  };
  const [ownedEdges, publicEdges] = await Promise.all([
    authenticated.db
      .select(edgeFields)
      .from(edges)
      .where(eq(edges.userId, authenticated.user.user_id))
      .orderBy(asc(edges.name), asc(edges.id)),
    authenticated.db
      .select(edgeFields)
      .from(edges)
      .where(and(eq(edges.accessScope, "public"), ne(edges.userId, authenticated.user.user_id)))
      .orderBy(asc(edges.name), asc(edges.id)),
  ]);
  return { user: authenticated.user, ownedEdges, publicEdges };
};

export const actions: Actions = {
  rename: async (event) => {
    const authenticated = await activeUser(event);
    if (!authenticated) return fail(401, { error: "Sign in to manage edges." });
    const form = await event.request.formData();
    const edgeId = form.get("edge_id");
    const nameValue = form.get("name");
    if (typeof edgeId !== "string" || !edgeId) {
      return fail(400, { error: "Select an edge to rename." });
    }
    if (typeof nameValue !== "string") {
      return fail(400, { error: "Enter an edge name." });
    }
    const name = nameValue.trim();
    if (!name || name.length > 100 || /[\u0000-\u001f\u007f-\u009f]/.test(name)) {
      return fail(400, { error: "Enter an edge name between 1 and 100 characters." });
    }
    const renamed = await authenticated.db
      .update(edges)
      .set({ name, updatedAt: new Date().toISOString() })
      .where(and(eq(edges.id, edgeId), eq(edges.userId, authenticated.user.user_id)))
      .returning({ id: edges.id });
    if (renamed.length === 0) return fail(404, { error: "Edge not found." });
    return { success: "edge_renamed" };
  },

  delete: async (event) => {
    const authenticated = await activeUser(event);
    if (!authenticated) return fail(401, { error: "Sign in to delete this edge." });
    const edgeId = (await event.request.formData()).get("edge_id");
    if (typeof edgeId !== "string" || !edgeId) {
      return fail(400, { error: "Select an edge to delete." });
    }
    try {
      const result = await removeOwnedEdge(
        authenticated.db,
        authenticated.user.user_id,
        edgeId,
        new CloudflareDnsProvider(
          authenticated.env.CLOUDFLARE_DNS_TOKEN,
          authenticated.env.CLOUDFLARE_DNS_ZONE_ID,
        ),
      );
      if (result.status === "not_found") return fail(404, { error: "Edge not found." });
      if (result.status === "invalid_edge") {
        return fail(409, { error: "This edge has an invalid network configuration." });
      }
    } catch (cause) {
      console.error({
        event: "edge_deletion_failed",
        edge_id: edgeId,
        error: cause instanceof Error ? cause.name : "unexpected_error",
        time: new Date().toISOString(),
      });
      return fail(503, {
        error: "DNS cleanup failed. The edge registration was kept so you can try again.",
      });
    }
    redirect(303, "/settings/edges");
  },
};
