import { json } from "@sveltejs/kit";
import {
  authenticateDashboardRequest,
  dashboardPreflight,
} from "$lib/server/remote-access/dashboard-http";
import { issueE2eCapability } from "$lib/server/remote-access/e2e-capability";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const OPTIONS: RequestHandler = ({ request }) => dashboardPreflight(request);

export const POST: RequestHandler = async (event) => {
  const authentication = await authenticateDashboardRequest(event);
  if ("response" in authentication) return authentication.response;
  let body: unknown;
  try {
    body = await event.request.json();
  } catch {
    return json({ error: "invalid_request" }, { status: 400, headers: authentication.headers });
  }
  if (
    !body ||
    typeof body !== "object" ||
    Array.isArray(body) ||
    typeof (body as Record<string, unknown>).browser_public_key !== "string" ||
    Object.keys(body).some((key) => key !== "browser_public_key")
  ) {
    return json({ error: "invalid_request" }, { status: 400, headers: authentication.headers });
  }
  const signingKey = event.platform?.env.E2E_CAPABILITY_SIGNING_KEY;
  if (!signingKey)
    return json({ error: "unavailable" }, { status: 503, headers: authentication.headers });
  const capability = await issueE2eCapability(
    remoteDatabase(event),
    authentication.userId,
    event.params.device_handle,
    (body as { browser_public_key: string }).browser_public_key,
    signingKey,
  );
  return capability
    ? json(capability, { headers: authentication.headers })
    : json({ error: "device_not_found" }, { status: 404, headers: authentication.headers });
};
