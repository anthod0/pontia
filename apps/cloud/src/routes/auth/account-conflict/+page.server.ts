import { redirect } from "@sveltejs/kit";
import { clearPendingAccount, pendingAccountSummary } from "$lib/server/auth/http";
import { AuthError } from "$lib/server/auth/types";
import { localizeHref } from "$lib/paraglide/runtime";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = async (event) => {
  try {
    return await pendingAccountSummary(event);
  } catch (cause) {
    if (!(cause instanceof AuthError)) throw cause;
    clearPendingAccount(event.cookies);
    redirect(303, localizeHref(`/login?error=${cause.code}`));
  }
};
