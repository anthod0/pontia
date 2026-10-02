import { json } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { environment } from "$lib/server/auth/http";
import { edges } from "$lib/server/db/schema";
import { authorizeEdgeDeployment } from "$lib/server/edge-deployment";
import { CloudflareDnsProvider, edgeHostname, validDnsChallenge } from "$lib/server/edge-network";
import { remoteDatabase } from "$lib/server/remote-access/http";
import { authenticateEdgeCredential } from "$lib/server/remote-access/resources";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) => {
  if (event.url.protocol !== "https:") return json({ error: "https_required" }, { status: 400 });
  let edgeId: string | undefined;
  let hostname: string | null = null;
  let operation: "publish" | "cleanup" | undefined;
  try {
    const input: unknown = await event.request.json();
    if (!input || typeof input !== "object" || Array.isArray(input))
      return json({ error: "invalid_dns_challenge" }, { status: 400 });
    const body = input as Record<string, unknown>;
    if (
      Object.keys(body).some((key) => !["operation", "value", "ticket"].includes(key)) ||
      (body.operation !== "publish" && body.operation !== "cleanup") ||
      !validDnsChallenge(body.value) ||
      (body.ticket !== undefined && typeof body.ticket !== "string")
    ) {
      return json({ error: "invalid_dns_challenge" }, { status: 400 });
    }
    operation = body.operation;
    const credential = event.request.headers.get("authorization")?.match(/^Bearer (.+)$/)?.[1];
    if (!credential) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const db = remoteDatabase(event);
    const principal = await authenticateEdgeCredential(db, credential);
    if (principal) {
      const edge = await db
        .select({ tunnelUrl: edges.tunnelUrl, accessScope: edges.accessScope })
        .from(edges)
        .where(eq(edges.id, principal.edgeId))
        .get();
      if (edge?.accessScope === "private") {
        edgeId = principal.edgeId;
        hostname = edgeHostname(edge.tunnelUrl);
      }
    } else if (typeof body.ticket === "string" && operation === "cleanup") {
      // Initial publication only occurs after the network API has verified IP:port.
      const identity = await authorizeEdgeDeployment(db, body.ticket, credential);
      if (identity) {
        edgeId = identity.edgeId;
        hostname = edgeHostname(identity.tunnelUrl);
      }
    }
    if (!edgeId || !hostname) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const env = environment(event);
    if (!(await env.EDGE_DNS_RATE_LIMIT.limit({ key: edgeId })).success)
      return json({ error: "rate_limited" }, { status: 429 });
    const dns = new CloudflareDnsProvider(env.CLOUDFLARE_DNS_TOKEN, env.CLOUDFLARE_DNS_ZONE_ID);
    if (operation === "publish") await dns.publishTxt(hostname, body.value);
    else await dns.cleanupTxt(hostname, body.value);
    console.info({
      event: "edge_dns_challenge",
      edge_id: edgeId,
      hostname,
      operation,
      result: "success",
      time: new Date().toISOString(),
    });
    return json({ hostname });
  } catch {
    console.error({
      event: "edge_dns_challenge",
      edge_id: edgeId,
      hostname,
      operation,
      result: "failed",
      time: new Date().toISOString(),
    });
    return json({ error: "service_unavailable" }, { status: 503 });
  }
};
