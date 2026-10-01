import { fail, redirect, type RequestEvent } from "@sveltejs/kit";
import { currentLogin, environment, origin } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { issueEdgeDeployment } from "$lib/server/edge-deployment";
import type { Actions, PageServerLoad } from "./$types";

async function activeUser(event: Pick<RequestEvent, "platform" | "cookies">) {
  const user = await currentLogin(event);
  return user ? { user, db: database(environment(event).DB) } : null;
}

export const load: PageServerLoad = async (event) => {
  const authenticated = await activeUser(event);
  if (!authenticated) redirect(303, "/login");
  return { user: authenticated.user };
};

export const actions: Actions = {
  default: async (event) => {
    const authenticated = await activeUser(event);
    if (!authenticated) return fail(401, { error: "Sign in to create an edge deployment." });
    try {
      return {
        deployment: await issueEdgeDeployment(
          authenticated.db,
          authenticated.user.user_id,
          origin(event),
        ),
      };
    } catch {
      return fail(503, { error: "Edge deployment is temporarily unavailable." });
    }
  },
};
