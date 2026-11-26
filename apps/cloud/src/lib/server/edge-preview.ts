import { json, type RequestEvent } from "@sveltejs/kit";
import { eq } from "drizzle-orm";
import { environment } from "./auth/http";
import { edges } from "./db/schema";
import {
  CloudflareDnsProvider,
  isLegacyEdgeDnsOwner,
  managedEdgeDns,
  previewBase,
  previewDns,
  probePreview,
} from "./edge-network";
import { bearerCredential, remoteDatabase } from "./remote-access/http";
import { authenticateEdgeCredential } from "./remote-access/resources";

export async function manageEdgePreview(event: RequestEvent, operation: "provision" | "confirm") {
  if (event.url.protocol !== "https:") return json({ error: "https_required" }, { status: 400 });
  const env = environment(event);
  const db = remoteDatabase(event);
  const credential = bearerCredential(event.request);
  const principal = credential && (await authenticateEdgeCredential(db, credential));
  if (!principal) return json({ error: "invalid_edge_credentials" }, { status: 401 });
  const edge = await db
    .select({ userId: edges.userId, tunnelUrl: edges.tunnelUrl })
    .from(edges)
    .where(eq(edges.id, principal.edgeId))
    .get();
  if (!edge) return json({ error: "invalid_edge_credentials" }, { status: 401 });
  if (!isLegacyEdgeDnsOwner(env, edge.userId)) {
    return json({ error: "forbidden" }, { status: 403 });
  }
  if (!env.PREVIEW_ZONE) return json({ error: "edge_preview_unavailable" }, { status: 409 });
  const managedDns = managedEdgeDns(edge.tunnelUrl);
  if (!managedDns) return json({ error: "invalid_edge" }, { status: 409 });
  if (!(await env.EDGE_DNS_RATE_LIMIT.limit({ key: principal.edgeId })).success)
    return json({ error: "rate_limited" }, { status: 429 });
  const base = previewBase(managedDns.label, env.PREVIEW_ZONE);
  try {
    if (operation === "confirm") {
      if (!(await probePreview(base, edge.tunnelUrl)))
        return json({ error: "preview_unreachable" }, { status: 422 });
      await db.update(edges).set({ previewEnabled: 1 }).where(eq(edges.id, principal.edgeId));
    } else {
      const dns = new CloudflareDnsProvider(env.CLOUDFLARE_DNS_TOKEN, env.CLOUDFLARE_DNS_ZONE_ID);
      await previewDns(env).ensureA(`*.${base}`, await dns.ipv4(managedDns.hostname));
    }
    return json({ preview_base_hostname: base, manage_certificate: true });
  } catch {
    return json({ error: "service_unavailable" }, { status: 503 });
  }
}
