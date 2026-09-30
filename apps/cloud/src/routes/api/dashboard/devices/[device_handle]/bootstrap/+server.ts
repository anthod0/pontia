import { base64url } from "jose";
import { authenticateDashboardBootstrap } from "$lib/server/remote-access/dashboard-http";
import { issueDashboardAccess } from "$lib/server/remote-access/dashboard-access";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

function htmlAttribute(value: string) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function bootstrapPage(action: string, ticket: string) {
  const nonce = base64url.encode(crypto.getRandomValues(new Uint8Array(32)));
  const actionOrigin = new URL(action).origin;
  const headers = new Headers({
    "Cache-Control": "no-store",
    "Content-Security-Policy": `default-src 'none'; script-src 'nonce-${nonce}'; form-action ${actionOrigin}; base-uri 'none'; frame-ancestors 'none'`,
    "Content-Type": "text/html; charset=utf-8",
    "Referrer-Policy": "no-referrer",
  });
  const body = `<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Opening Pontia Dashboard</title></head>
<body>
<form id="dashboard-bootstrap" method="post" action="${htmlAttribute(action)}">
<input type="hidden" name="ticket" value="${htmlAttribute(ticket)}">
<noscript><button type="submit">Continue to Dashboard</button></noscript>
</form>
<script nonce="${nonce}">document.getElementById("dashboard-bootstrap").submit();</script>
</body>
</html>`;
  return new Response(body, { status: 200, headers });
}

export const POST: RequestHandler = async (event) => {
  try {
    const authentication = await authenticateDashboardBootstrap(event);
    if ("response" in authentication) return authentication.response;
    if (event.url.search !== "" || (await event.request.text()) !== "") {
      return new Response("device access request failed", { status: 400 });
    }
    const result = await issueDashboardAccess(
      remoteDatabase(event),
      authentication.userId,
      event.params.device_handle,
    );
    if (result.status === "device_not_found") {
      return new Response("device access request failed", { status: 404 });
    }
    return bootstrapPage(result.value.bootstrapUrl, result.value.ticket);
  } catch {
    return new Response("device access request failed", { status: 503 });
  }
};
