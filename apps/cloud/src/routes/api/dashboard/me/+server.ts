import { json } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { users } from "$lib/server/db/schema";
import {
  authenticateDashboardRequest,
  dashboardPreflight,
} from "$lib/server/remote-access/dashboard-http";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const OPTIONS: RequestHandler = ({ request }) => dashboardPreflight(request);

export const GET: RequestHandler = async (event) => {
  const authentication = await authenticateDashboardRequest(event);
  if ("response" in authentication) return authentication.response;
  const user = await remoteDatabase(event)
    .select({ display_name: users.displayName, avatar_url: users.avatarUrl })
    .from(users)
    .where(eq(users.id, authentication.userId))
    .get();
  if (!user) {
    return json({ error: "invalid_credentials" }, { status: 401, headers: authentication.headers });
  }
  return json(user, { headers: authentication.headers });
};
