import { and, eq, gt, isNull, sql } from "drizzle-orm";
import { v7 as uuidv7 } from "uuid";
import { sha256Base64url } from "./crypto";
import type { Database } from "./db";
import { edgeTickets, edges } from "./db/schema";
import { issueEdgeTicket, parseEdgeTicket, type EdgeTicketDependencies } from "./edge-tickets";
import { generateHeroName, isHeroName } from "./hero-name";
import { parseEdgeCredential } from "./remote-access/resources";
import { isUuidV7 } from "./uuid";

const DEPLOYMENT_LIFETIME_MS = 60 * 60 * 1000;
const MAX_NAME_ATTEMPTS = 32;
const databaseTimestamp = sql<string>`(strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))`;

type DeploymentPayload = { name: string };

export type DeploymentDependencies = EdgeTicketDependencies & {
  edgeId(): string;
  heroName(): string;
};

const defaultDependencies: DeploymentDependencies = {
  now: () => new Date(),
  randomBytes: (length) => crypto.getRandomValues(new Uint8Array(length)),
  edgeId: uuidv7,
  heroName: generateHeroName,
};

export function decodeDeploymentPayload(value: unknown): DeploymentPayload | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const keys = Object.keys(value);
  if (keys.length !== 1 || keys[0] !== "name") return null;
  const name = (value as Record<string, unknown>).name;
  return isHeroName(name) ? { name } : null;
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

export function deploymentCommand(origin: string, edgeId: string, ticket: string) {
  return `curl -fsSL ${shellQuote(`${origin}/install-edge.sh`)} | sudo sh &&\nsudo pontia-edge init \\\n  --website-origin ${shellQuote(origin)} \\\n  --edge-id ${shellQuote(edgeId)} \\\n  --ticket ${shellQuote(ticket)}`;
}

async function unusedHeroName(db: Database, dependencies: DeploymentDependencies) {
  for (let attempt = 0; attempt < MAX_NAME_ATTEMPTS; attempt += 1) {
    const name = dependencies.heroName();
    if (!isHeroName(name)) throw new Error("Hero name generator returned an invalid name");
    const payload = JSON.stringify({ name });
    const [edge, ticket] = await Promise.all([
      db.select({ id: edges.id }).from(edges).where(eq(edges.name, name)).get(),
      db
        .select({ id: edgeTickets.id })
        .from(edgeTickets)
        .where(
          and(
            eq(edgeTickets.purpose, "edge_deployment"),
            eq(edgeTickets.payload, payload),
            gt(edgeTickets.expiresAt, databaseTimestamp),
          ),
        )
        .get(),
    ]);
    if (!edge && !ticket) return name;
  }
  throw new Error("Unable to allocate an edge name");
}

export async function issueEdgeDeployment(
  db: Database,
  userId: string,
  origin: string,
  dependencies: DeploymentDependencies = defaultDependencies,
) {
  const edgeId = dependencies.edgeId();
  if (!isUuidV7(edgeId) || edgeId !== edgeId.toLowerCase()) {
    throw new Error("Edge ID generator returned an invalid UUID v7");
  }
  const name = await unusedHeroName(db, dependencies);
  const expiresAt = new Date(dependencies.now().getTime() + DEPLOYMENT_LIFETIME_MS);
  const ticket = await issueEdgeTicket(
    db,
    {
      purpose: "edge_deployment",
      userId,
      expectedEdgeId: edgeId,
      payload: { name },
      expiresAt,
      decodePayload: decodeDeploymentPayload,
    },
    dependencies,
  );
  return {
    edgeId,
    name,
    expiresAt: expiresAt.toISOString(),
    command: deploymentCommand(origin, edgeId, ticket),
  };
}

export type EnrollmentResult =
  | { status: "created" | "existing"; edge: { edgeId: string; name: string } }
  | { status: "invalid" };

function storedEdge(db: Database, edgeId: string) {
  return db
    .select({
      edgeId: edges.id,
      userId: edges.userId,
      name: edges.name,
      tunnelUrl: edges.tunnelUrl,
      accessScope: edges.accessScope,
      serviceCredentialHash: edges.serviceCredentialHash,
    })
    .from(edges)
    .where(eq(edges.id, edgeId))
    .get();
}

export async function enrollEdge(
  db: Database,
  ticketValue: string,
  credentialValue: string,
): Promise<EnrollmentResult> {
  const ticket = parseEdgeTicket(ticketValue);
  const credential = parseEdgeCredential(credentialValue);
  if (!ticket || !credential) return { status: "invalid" };

  const secretHash = await sha256Base64url(ticket.secret);
  const serviceCredentialHash = await sha256Base64url(credential.secret);
  const candidate = await db
    .select({
      id: edgeTickets.id,
      userId: edgeTickets.userId,
      expectedEdgeId: edgeTickets.expectedEdgeId,
      payload: edgeTickets.payload,
      expiresAt: edgeTickets.expiresAt,
      consumedAt: edgeTickets.consumedAt,
    })
    .from(edgeTickets)
    .where(
      and(
        eq(edgeTickets.secretHash, secretHash),
        eq(edgeTickets.purpose, "edge_deployment"),
        eq(edgeTickets.expectedEdgeId, credential.edgeId),
      ),
    )
    .get();
  if (!candidate) return { status: "invalid" };

  let payload: DeploymentPayload | null;
  try {
    payload = decodeDeploymentPayload(JSON.parse(candidate.payload));
  } catch {
    payload = null;
  }
  if (!payload) return { status: "invalid" };

  const existing = await storedEdge(db, credential.edgeId);
  const tunnelUrl = `wss://${payload.name}.edge.pontia.dev/tunnel`;
  const isMatchingEdge = (edge: typeof existing): edge is NonNullable<typeof existing> =>
    edge !== undefined &&
    edge.userId === candidate.userId &&
    edge.name === payload.name &&
    edge.tunnelUrl === tunnelUrl &&
    edge.accessScope === "private" &&
    edge.serviceCredentialHash === serviceCredentialHash;
  if (candidate.consumedAt !== null) {
    if (isMatchingEdge(existing)) {
      return { status: "existing", edge: { edgeId: existing.edgeId, name: existing.name } };
    }
    return { status: "invalid" };
  }
  if (existing) return { status: "invalid" };

  const condition = and(
    eq(edgeTickets.id, candidate.id),
    eq(edgeTickets.secretHash, secretHash),
    eq(edgeTickets.purpose, "edge_deployment"),
    eq(edgeTickets.userId, candidate.userId),
    eq(edgeTickets.expectedEdgeId, credential.edgeId),
    eq(edgeTickets.payload, candidate.payload),
    isNull(edgeTickets.consumedAt),
    gt(edgeTickets.expiresAt, databaseTimestamp),
  );
  try {
    const [inserted, consumed] = await db.batch([
      db
        .insert(edges)
        .select(
          db
            .select({
              id: sql<string>`${credential.edgeId}`.as("id"),
              userId: edgeTickets.userId,
              accessScope: sql<"private">`'private'`.as("access_scope"),
              name: sql<string>`${payload.name}`.as("name"),
              tunnelUrl: sql<string>`${tunnelUrl}`.as("tunnel_url"),
              serviceCredentialHash: sql<string>`${serviceCredentialHash}`.as(
                "service_credential_hash",
              ),
              createdAt: databaseTimestamp.as("created_at"),
              updatedAt: databaseTimestamp.as("updated_at"),
            })
            .from(edgeTickets)
            .where(condition),
        )
        .returning({ edgeId: edges.id, name: edges.name }),
      db
        .update(edgeTickets)
        .set({ consumedAt: databaseTimestamp })
        .where(condition)
        .returning({ id: edgeTickets.id }),
    ]);
    if (inserted.length === 1 && consumed.length === 1) {
      return { status: "created", edge: inserted[0] };
    }
  } catch {
    // A concurrent identical request may have completed the atomic batch first.
  }

  const [retryEdge, retryTicket] = await Promise.all([
    storedEdge(db, credential.edgeId),
    db
      .select({ consumedAt: edgeTickets.consumedAt })
      .from(edgeTickets)
      .where(eq(edgeTickets.id, candidate.id))
      .get(),
  ]);
  if (retryTicket?.consumedAt && isMatchingEdge(retryEdge)) {
    return { status: "existing", edge: { edgeId: retryEdge.edgeId, name: retryEdge.name } };
  }
  return { status: "invalid" };
}
