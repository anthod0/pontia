import { fail, redirect } from "@sveltejs/kit";
import { currentLogin, environment, origin } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { issueEdgeDeployment } from "$lib/server/edge-deployment";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = async (event) => {
  if (!(await currentLogin(event))) redirect(303, "/login");
};

export const actions: Actions = {
  default: async (event) => {
    const user = await currentLogin(event);
    if (!user) return fail(401, { error: "Sign in to create an edge deployment." });
    try {
      return {
        deployment: await issueEdgeDeployment(
          database(environment(event).DB),
          user.user_id,
          origin(event),
        ),
      };
    } catch {
      return fail(503, { error: "Edge deployment is temporarily unavailable." });
    }
  },
};
