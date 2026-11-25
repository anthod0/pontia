import { fail, redirect, type RequestEvent } from "@sveltejs/kit";
import {
  decideDeviceAuthorization,
  displayUserCode,
  parseUserCode,
  recordUserCodeAttempt,
} from "$lib/server/auth/device";
import { currentLogin, environment } from "$lib/server/auth/http";
import { isUuidV7 } from "$lib/server/uuid";
import { PUBLIC_DASHBOARD_ORIGIN } from "$lib/server/auth/return-to";
import { database } from "$lib/server/db";
import { m } from "$lib/paraglide/messages";
import { localizeHref } from "$lib/paraglide/runtime";
import type { Actions, PageServerLoad } from "./$types";

const REGISTRATION_WAIT_MS = 5 * 60_000;

async function browserLogin(event: RequestEvent) {
  return currentLogin(event);
}

export const load: PageServerLoad = async (event) => {
  const user = await browserLogin(event);
  const normalized = parseUserCode(event.url.searchParams.get("user_code") ?? "");
  const id = event.url.searchParams.get("init_device") ?? "";
  const initDevice = isUuidV7(id) ? id : null;
  const userCode = normalized ? displayUserCode(normalized) : "";
  const params = new URLSearchParams();
  if (userCode) params.set("user_code", userCode);
  if (initDevice) params.set("init_device", initDevice);
  const approvedAt = Number(event.url.searchParams.get("approved_at"));
  const result = event.url.searchParams.get("result");
  const registration =
    result === "approved" && initDevice && Number.isSafeInteger(approvedAt) && approvedAt > 0
      ? {
          id: initDevice,
          approvedAt,
          remainingMs: Math.max(
            0,
            Math.min(REGISTRATION_WAIT_MS, approvedAt + REGISTRATION_WAIT_MS - Date.now()),
          ),
        }
      : null;
  return {
    user,
    initDevice,
    returnTo: localizeHref(params.size ? `/device?${params}` : "/device"),
    dashboard: PUBLIC_DASHBOARD_ORIGIN,
    registration,
    userCode,
    result,
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
  const approvedAt = new Date();
  if (
    typeof code !== "string" ||
    !(await decideDeviceAuthorization(db, code, login.user_id, decision, approvedAt))
  )
    redirect(303, localizeHref("/device?result=invalid"));
  const id = data.get("init_device");
  const result = new URLSearchParams({ result: decision });
  if (decision === "approved" && typeof id === "string" && isUuidV7(id)) {
    result.set("init_device", id);
    result.set("approved_at", String(approvedAt.getTime()));
  }
  redirect(303, localizeHref(`/device?${result}`));
}

export const actions: Actions = {
  approve: (event) => decide(event, "approved"),
  deny: (event) => decide(event, "denied"),
};
