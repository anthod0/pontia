import { redirect } from "@sveltejs/kit";
import { currentLogin } from "$lib/server/auth/http";
import { localizeHref } from "$lib/paraglide/runtime";
import type { LayoutServerLoad } from "./$types";

export const load: LayoutServerLoad = async (event) => {
  if (!(await currentLogin(event))) redirect(303, localizeHref("/login"));
};
