import { json, type RequestEvent } from "@sveltejs/kit";
import { currentLogin } from "../auth/http";
import { activeLogin } from "../auth/identity";
import { remoteDatabase } from "./http";

export const PUBLIC_DASHBOARD_ORIGIN = "https://app.pontia.dev";

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

export async function authenticateDashboardRequest(
  event: RequestEvent,
): Promise<{ userId: string; headers: Headers } | { response: Response }> {
  if (event.request.headers.get("origin") !== PUBLIC_DASHBOARD_ORIGIN) {
    return {
      response: json({ error: "invalid_origin" }, { status: 403, headers: rejectedCorsHeaders() }),
    };
  }
  const headers = corsHeaders();
  const claims = await currentLogin(event);
  if (claims) {
    const login = await activeLogin(remoteDatabase(event), claims.sub, claims.user_id);
    if (login) return { userId: login.userId, headers };
  }
  return {
    response: json({ error: "invalid_credentials" }, { status: 401, headers }),
  };
}
