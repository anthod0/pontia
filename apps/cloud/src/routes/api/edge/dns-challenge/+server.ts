import { json } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { environment } from "$lib/server/auth/http";
import { edges } from "$lib/server/db/schema";
import {
  CloudflareDnsProvider,
  isLegacyEdgeDnsOwner,
  managedEdgeHostname,
  previewBase,
  previewDns,
  validDnsChallenge,
} from "$lib/server/edge-network";
import { bearerCredential, remoteDatabase } from "$lib/server/remote-access/http";
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
      Object.keys(body).some((key) => !["operation", "value", "scope"].includes(key)) ||
      (body.operation !== "publish" && body.operation !== "cleanup") ||
      (body.scope !== undefined && body.scope !== "control" && body.scope !== "preview") ||
      !validDnsChallenge(body.value)
    ) {
      return json({ error: "invalid_dns_challenge" }, { status: 400 });
    }
    operation = body.operation;
    const credential = bearerCredential(event.request);
    if (!credential) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const db = remoteDatabase(event);
    const principal = await authenticateEdgeCredential(db, credential);
    if (!principal) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    edgeId = principal.edgeId;
    const edge = await db
      .select({ userId: edges.userId, dnsLabel: edges.dnsLabel, tunnelUrl: edges.tunnelUrl })
      .from(edges)
      .where(eq(edges.id, principal.edgeId))
      .get();
    if (!edge) return json({ error: "invalid_edge_credentials" }, { status: 401 });
    const env = environment(event);
    if (!isLegacyEdgeDnsOwner(env, edge.userId)) {
      return json({ error: "forbidden" }, { status: 403 });
    }
    hostname = managedEdgeHostname(edge);
    if (!hostname || edge.dnsLabel === null) {
      return json({ error: "invalid_edge" }, { status: 409 });
    }
    if (body.scope === "preview" && !env.PREVIEW_ZONE) {
      return json({ error: "edge_preview_unavailable" }, { status: 409 });
    }
    if (!(await env.EDGE_DNS_RATE_LIMIT.limit({ key: edgeId })).success)
      return json({ error: "rate_limited" }, { status: 429 });
    if (body.scope === "preview") hostname = previewBase(edge.dnsLabel, env.PREVIEW_ZONE!);
    const dns =
      body.scope === "preview"
        ? previewDns(env)
        : new CloudflareDnsProvider(env.CLOUDFLARE_DNS_TOKEN, env.CLOUDFLARE_DNS_ZONE_ID);
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
