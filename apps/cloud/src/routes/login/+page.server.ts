import { redirect } from "@sveltejs/kit";
import { currentLogin, loginReturnTo } from "$lib/server/auth/http";
import { localizeHref } from "$lib/paraglide/runtime";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = async (event) => {
  const requested = loginReturnTo(event);
  const returnTo =
    !requested || requested.startsWith("/")
      ? localizeHref(requested ?? "/settings/account")
      : requested;
  if (await currentLogin(event)) redirect(303, returnTo);
  return {
    error: event.url.searchParams.get("error"),
    hasCredential: !!event.cookies.get("_at"),
    returnTo,
  };
};
