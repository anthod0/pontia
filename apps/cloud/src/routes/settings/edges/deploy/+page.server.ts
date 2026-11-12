import { fail, redirect } from "@sveltejs/kit";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { m } from "$lib/paraglide/messages";
import { localizeHref } from "$lib/paraglide/runtime";
import { issueEdgeDeployment } from "$lib/server/edge-deployment";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = async (event) => {
  if (!(await currentLogin(event))) redirect(303, localizeHref("/login"));
};

export const actions: Actions = {
  default: async (event) => {
    const user = await currentLogin(event);
    if (!user) return fail(401, { error: m.deploy_error_sign_in() });
    try {
      return {
        deployment: await issueEdgeDeployment(database(environment(event).DB), user.user_id),
      };
    } catch {
      return fail(503, { error: m.deploy_error_unavailable() });
    }
  },
};
