import type { RequestEvent } from "@sveltejs/kit";
import { authenticateCliCredential } from "../auth/cli";
import { environment } from "../auth/http";
import { database } from "../db";

export function bearerCredential(request: Request) {
  const authorization = request.headers.get("authorization");
  return authorization && /^Bearer ([^\s]+)$/.exec(authorization)?.[1];
}

export async function cliPrincipal(event: RequestEvent) {
  const credential = bearerCredential(event.request);
  if (!credential) return null;
  return authenticateCliCredential(database(environment(event).DB), credential);
}

export function remoteDatabase(event: RequestEvent) {
  return database(environment(event).DB);
}
