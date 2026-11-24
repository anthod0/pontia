import { json } from "@sveltejs/kit";
import { createRemoteJWKSet, jwtVerify } from "jose";
import { environment } from "$lib/server/auth/http";
import { notifyEdgeUpdates } from "$lib/server/edge-update";
import { bearerCredential, remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

const issuer = "https://token.actions.githubusercontent.com";
const keys = createRemoteJWKSet(new URL(issuer + "/.well-known/jwks"));
const audience = "https://pontia.dev/api/internal/notify-update";
const workflow = "anthod0/pontia/.github/workflows/release.yml@refs/heads/main";

export const POST: RequestHandler = async (event) => {
  const token = bearerCredential(event.request);
  if (!token) return new Response(null, { status: 401 });
  try {
    const { payload } = await jwtVerify(token, keys, {
      issuer,
      audience,
      algorithms: ["RS256"],
      requiredClaims: ["exp"],
    });
    if (
      payload.workflow_ref !== workflow ||
      payload.repository !== "anthod0/pontia" ||
      payload.ref !== "refs/heads/main" ||
      !["push", "workflow_dispatch"].includes(String(payload.event_name))
    ) {
      return new Response(null, { status: 403 });
    }
  } catch {
    return new Response(null, { status: 401 });
  }
  const after = event.url.searchParams.get("after") ?? "";
  const version = event.url.searchParams.get("version");
  if (!version || version.length > 128 || after.length > 36)
    return new Response(null, { status: 400 });
  return json(
    await notifyEdgeUpdates(
      remoteDatabase(event),
      after,
      version,
      environment(event).E2E_CAPABILITY_SIGNING_KEY,
      fetch,
    ),
  );
};
