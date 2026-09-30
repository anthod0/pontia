import { redirect } from "@sveltejs/kit";
import { currentLogin, loginReturnTo } from "$lib/server/auth/http";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = async (event) => {
  const returnTo = loginReturnTo(event);
  if (await currentLogin(event)) redirect(303, returnTo ?? "/account");
  return {
    error: event.url.searchParams.get("error"),
    hasCredential: !!event.cookies.get("_at"),
    returnTo,
  };
};
