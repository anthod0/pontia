import { json, type RequestEvent } from "@sveltejs/kit";
import { currentLogin } from "../auth/http";
import { activeLogin } from "../auth/identity";
import { PUBLIC_DASHBOARD_ORIGIN } from "../auth/return-to";
import { remoteDatabase } from "./http";

export { PUBLIC_DASHBOARD_ORIGIN } from "../auth/return-to";
export const CLOUD_ORIGIN = "https://pontia.dev";

function corsHeaders(): Headers {
  return new Headers({
    "Access-Control-Allow-Credentials": "true",
    "Access-Control-Allow-Methods": "GET, OPTIONS",
    "Access-Control-Allow-Origin": PUBLIC_DASHBOARD_ORIGIN,
    Vary: "Origin",
  });
}

function rejectedCorsHeaders(): Headers {
  return new Headers({ Vary: "Origin" });
}

export function dashboardPreflight(request: Request): Response {
  return request.headers.get("origin") === PUBLIC_DASHBOARD_ORIGIN
    ? new Response(null, { status: 204, headers: corsHeaders() })
    : new Response(null, { status: 403, headers: rejectedCorsHeaders() });
}

async function authenticatedUser(event: RequestEvent): Promise<string | null> {
  const claims = await currentLogin(event);
  if (!claims) return null;
  const login = await activeLogin(remoteDatabase(event), claims.sub, claims.user_id);
  return login?.userId ?? null;
}

export async function authenticateDashboardRequest(
  event: RequestEvent,
): Promise<{ userId: string; headers: Headers } | { response: Response }> {
  if (event.request.headers.get("origin") !== PUBLIC_DASHBOARD_ORIGIN) {
    return {
      response: json({ error: "invalid_origin" }, { status: 403, headers: rejectedCorsHeaders() }),
    };
  }
  const headers = corsHeaders();
  const userId = await authenticatedUser(event);
  if (userId) return { userId, headers };
  return {
    response: json({ error: "invalid_credentials" }, { status: 401, headers }),
  };
}

export async function authenticateDashboardBootstrap(
  event: RequestEvent,
): Promise<{ userId: string } | { response: Response }> {
  const requestOrigin = event.request.headers.get("origin");
  if (requestOrigin !== CLOUD_ORIGIN && requestOrigin !== PUBLIC_DASHBOARD_ORIGIN) {
    return { response: json({ error: "invalid_origin" }, { status: 403 }) };
  }
  const userId = await authenticatedUser(event);
  return userId
    ? { userId }
    : { response: json({ error: "invalid_credentials" }, { status: 401 }) };
}
