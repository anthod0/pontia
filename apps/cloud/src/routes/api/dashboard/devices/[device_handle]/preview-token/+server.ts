import { json } from "@sveltejs/kit";
import {
  authenticateDashboardRequest,
  dashboardPreflight,
} from "$lib/server/remote-access/dashboard-http";
import { issuePreviewToken, validPreviewPort } from "$lib/server/remote-access/preview-token";
import { remoteDatabase } from "$lib/server/remote-access/http";
import { environment } from "$lib/server/auth/http";
import type { RequestHandler } from "./$types";

export const OPTIONS: RequestHandler = ({ request }) => dashboardPreflight(request);
export const POST: RequestHandler = async (event) => {
  const auth = await authenticateDashboardRequest(event);
  if ("response" in auth) return auth.response;
  const { headers } = auth;
  headers.set("Cache-Control", "no-store");
  const body = await event.request.json().catch(() => null);
  if (
    !body ||
    typeof body !== "object" ||
    Object.keys(body).length !== 1 ||
    !("port" in body) ||
    !validPreviewPort(body.port)
  )
    return json({ error: "invalid_request" }, { status: 400, headers });
  try {
    const result = await issuePreviewToken(
      remoteDatabase(event),
      auth.userId,
      event.params.device_handle,
      body.port,
      environment(event),
    );
    return json(result.body, { status: result.status, headers });
  } catch {
    return json({ error: "unavailable" }, { status: 503, headers });
  }
};
