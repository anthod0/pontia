import { redirect } from "@sveltejs/kit";
import { currentLogin } from "$lib/server/auth/http";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = async (event) => {
  if (await currentLogin(event)) redirect(303, "/account");
  return {
    error: event.url.searchParams.get("error"),
    hasCredential: !!event.cookies.get("_at"),
  };
};
