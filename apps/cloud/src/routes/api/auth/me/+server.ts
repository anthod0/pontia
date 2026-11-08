import { json } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { users } from "$lib/server/db/schema";
import { cliPrincipal, remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async (event) => {
  const principal = await cliPrincipal(event);
  if (!principal) return json({ error: "invalid_credentials" }, { status: 401 });
  const user = await remoteDatabase(event)
    .select({ id: users.id, display_name: users.displayName })
    .from(users)
    .where(eq(users.id, principal.userId))
    .get();
  if (!user) return json({ error: "invalid_credentials" }, { status: 401 });
  return json(user);
};
