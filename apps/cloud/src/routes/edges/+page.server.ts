import { redirect, type RequestEvent } from "@sveltejs/kit";
import { asc, eq } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { edges } from "$lib/server/db/schema";
import type { PageServerLoad } from "./$types";

async function activeUser(event: Pick<RequestEvent, "platform" | "cookies">) {
  const user = await currentLogin(event);
  return user ? { user, db: database(environment(event).DB) } : null;
}

export const load: PageServerLoad = async (event) => {
  const authenticated = await activeUser(event);
  if (!authenticated) redirect(303, "/login");
  const ownedEdges = await authenticated.db
    .select({
      id: edges.id,
      name: edges.name,
      tunnelUrl: edges.tunnelUrl,
      createdAt: edges.createdAt,
    })
    .from(edges)
    .where(eq(edges.userId, authenticated.user.user_id))
    .orderBy(asc(edges.name), asc(edges.id));
  return { user: authenticated.user, edges: ownedEdges };
};
