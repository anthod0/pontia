import { fail, redirect, type RequestEvent } from "@sveltejs/kit";
import {
  decideDeviceAuthorization,
  displayUserCode,
  parseUserCode,
  recordUserCodeAttempt,
} from "$lib/server/auth/device";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { m } from "$lib/paraglide/messages";
import { localizeHref } from "$lib/paraglide/runtime";
import type { Actions, PageServerLoad } from "./$types";

async function browserLogin(event: RequestEvent) {
  return currentLogin(event);
}

export const load: PageServerLoad = async (event) => {
  const user = await browserLogin(event);
  const normalized = parseUserCode(event.url.searchParams.get("user_code") ?? "");
  return {
    user,
    userCode: normalized ? displayUserCode(normalized) : "",
    result: event.url.searchParams.get("result"),
  };
};

async function decide(event: RequestEvent, decision: "approved" | "denied") {
  const login = await browserLogin(event);
  if (!login) return fail(401, { error: m.device_error_sign_in() });
  const db = database(environment(event).DB);
  if (!(await recordUserCodeAttempt(db, login.sub)))
    return fail(429, {
      error: m.device_error_rate_limited(),
    });
  const data = await event.request.formData();
  const code = data.get("user_code");
  if (
    typeof code !== "string" ||
    !(await decideDeviceAuthorization(db, code, login.user_id, decision))
  )
    redirect(303, localizeHref("/device?result=invalid"));
  redirect(303, localizeHref(`/device?result=${decision}`));
}

export const actions: Actions = {
  approve: (event) => decide(event, "approved"),
  deny: (event) => decide(event, "denied"),
};
