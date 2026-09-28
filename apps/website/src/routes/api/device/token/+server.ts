import { json } from "@sveltejs/kit";
import { pollDeviceAuthorization, recordPollAttempt } from "$lib/server/auth/device";
import { environment, origin } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  origin(event);
  const db = database(environment(event).DB);
  if (!(await recordPollAttempt(db, event.getClientAddress()))) return json({ error: "slow_down" });
  let deviceCode = "";
  try {
    const body = (await event.request.json()) as Record<string, unknown>;
    if (typeof body.device_code === "string") deviceCode = body.device_code;
  } catch {
    // Invalid bodies receive the same terminal response as unknown device codes.
  }
  const result = await pollDeviceAuthorization(db, deviceCode);
  const body =
    result.status === "authorized"
      ? { access_token: result.token, token_type: "Bearer" }
      : { error: result.status };
  return json(body);
};
