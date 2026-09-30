import type { RequestEvent } from "@sveltejs/kit";
import { authenticateCliCredential } from "../auth/cli";
import { environment } from "../auth/http";
import { database, type Database } from "../db";
import { authenticateEdgeCredential } from "./resources";

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

export type EdgeTicketRequest =
  | { status: "authenticated"; db: Database; edgeId: string; ticket: string }
  | { status: "invalid_edge_credentials" }
  | { status: "invalid_ticket" };

export async function authenticateEdgeTicketRequest(
  event: RequestEvent,
): Promise<EdgeTicketRequest> {
  const credential = bearerCredential(event.request);
  const db = remoteDatabase(event);
  const principal = credential && (await authenticateEdgeCredential(db, credential));
  if (!principal) return { status: "invalid_edge_credentials" };

  let body: unknown;
  try {
    body = await event.request.json();
  } catch {
    return { status: "invalid_ticket" };
  }
  if (
    !body ||
    typeof body !== "object" ||
    Array.isArray(body) ||
    typeof (body as Record<string, unknown>).ticket !== "string" ||
    Object.keys(body).some((key) => key !== "ticket")
  ) {
    return { status: "invalid_ticket" };
  }
  return {
    status: "authenticated",
    db,
    edgeId: principal.edgeId,
    ticket: (body as { ticket: string }).ticket,
  };
}
