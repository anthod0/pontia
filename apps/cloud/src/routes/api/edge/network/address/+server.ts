import { json } from "@sveltejs/kit";
import { isGlobalUnicastIpv4 } from "$lib/server/edge-network";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = (event) => {
  // The Cloudflare adapter supplies CF-Connecting-IP through getClientAddress().
  // Do not use caller-controlled forwarding headers or a client-provided address.
  const headers = { "Cache-Control": "no-store" };
  if (event.url.protocol !== "https:") {
    return json({ error: "https_required" }, { status: 400, headers });
  }
  const ipv4 = event.getClientAddress();
  if (!isGlobalUnicastIpv4(ipv4)) {
    return json({ error: "global_ipv4_required" }, { status: 400, headers });
  }
  return json({ ipv4 }, { headers });
};
