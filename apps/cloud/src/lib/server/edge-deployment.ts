import { isEdgePort, edgeAuthority } from "../edge-port";
import { and, eq, gt, isNull, like, ne, or, sql } from "drizzle-orm";
import { v7 as uuidv7 } from "uuid";
import { sha256Base64url } from "./crypto";
import type { Database } from "./db";
import { edgeTickets, edges } from "./db/schema";
import { edgeHostname, isValidEdgeHostname } from "./edge-network";
import { issueEdgeTicket, parseEdgeTicket, type EdgeTicketDependencies } from "./edge-tickets";
import { parseEdgeCredential } from "./remote-access/resources";
import { isUuidV7 } from "./uuid";

const DEPLOYMENT_LIFETIME_MS = 60 * 60 * 1000;
const databaseTimestamp = sql<string>`(strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))`;

type DeploymentPayload = { hostname: string };

export type DeploymentDependencies = EdgeTicketDependencies & {
  edgeId(): string;
};

const defaultDependencies: DeploymentDependencies = {
  now: () => new Date(),
  randomBytes: (length) => crypto.getRandomValues(new Uint8Array(length)),
  edgeId: uuidv7,
};

export function decodeDeploymentPayload(value: unknown): DeploymentPayload | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const keys = Object.keys(value);
  if (keys.length !== 1 || keys[0] !== "hostname") return null;
  const hostname = (value as Record<string, unknown>).hostname;
  return isValidEdgeHostname(hostname) ? { hostname } : null;
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

export function deploymentCommand(edgeId: string, ticket: string) {
  return `curl -fsSL ${shellQuote("https://get.pontia.dev/install-edge.sh")} | sudo sh &&\nsudo pontia-edge init \\\n  --edge-id ${shellQuote(edgeId)} \\\n  --ticket ${shellQuote(ticket)}`;
}

function tunnelUrl(hostname: string, port: number) {
  return `wss://${edgeAuthority(hostname, port)}/tunnel`;
}

/** Any registered endpoint on this hostname, regardless of its port. */
function registeredHostname(hostname: string) {
  return or(
    eq(edges.tunnelUrl, tunnelUrl(hostname, 443)),
    like(edges.tunnelUrl, `wss://${hostname}:%/tunnel`),
  );
}

export type DeploymentIssue =
  | { status: "invalid" }
  | { status: "conflict" }
  | {
      status: "issued";
      deployment: { edgeId: string; hostname: string; expiresAt: string; command: string };
    };

export async function issueEdgeDeployment(
  db: Database,
  userId: string,
  hostname: string,
  dependencies: DeploymentDependencies = defaultDependencies,
): Promise<DeploymentIssue> {
  if (!isValidEdgeHostname(hostname)) return { status: "invalid" };
  if (await db.select({ id: edges.id }).from(edges).where(registeredHostname(hostname)).get()) {
    return { status: "conflict" };
  }
  const edgeId = dependencies.edgeId();
  if (!isUuidV7(edgeId) || edgeId !== edgeId.toLowerCase()) {
    throw new Error("Edge ID generator returned an invalid UUID v7");
  }
  const expiresAt = new Date(dependencies.now().getTime() + DEPLOYMENT_LIFETIME_MS);
  const ticket = await issueEdgeTicket(
    db,
    {
      purpose: "edge_deployment",
      userId,
      expectedEdgeId: edgeId,
      payload: { hostname },
      expiresAt,
      decodePayload: decodeDeploymentPayload,
    },
    dependencies,
  );
  return {
    status: "issued",
    deployment: {
      edgeId,
      hostname,
      expiresAt: expiresAt.toISOString(),
      command: deploymentCommand(edgeId, ticket),
    },
  };
}

export type DeploymentIdentity = { edgeId: string; name: string; tunnelUrl: string };

type DeploymentAuthorization = {
  ticketId: number;
  ticketSecretHash: string;
  userId: string;
  hostname: string;
  edgeId: string;
  credentialSecret: string;
  payload: string;
  serviceCredentialHash: string;
};

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

async function completedDeployment(db: Database, deployment: DeploymentAuthorization) {
  const [edge, ticket] = await Promise.all([
    storedEdge(db, deployment.edgeId),
    db
      .select({ consumedAt: edgeTickets.consumedAt })
      .from(edgeTickets)
      .where(eq(edgeTickets.id, deployment.ticketId))
      .get(),
  ]);
  if (
    !ticket?.consumedAt ||
    !edge ||
    edge.userId !== deployment.userId ||
    edge.accessScope !== "private" ||
    edge.serviceCredentialHash !== deployment.serviceCredentialHash ||
    edgeHostname(edge.tunnelUrl) !== deployment.hostname
  ) {
    return null;
  }
  return { edgeId: edge.edgeId, name: edge.name, tunnelUrl: edge.tunnelUrl };
}

async function deploymentAuthorization(
  db: Database,
  ticketValue: string,
  credentialValue: string,
): Promise<{ deployment: DeploymentAuthorization; consumed: boolean } | null> {
  const ticket = parseEdgeTicket(ticketValue);
  const credential = parseEdgeCredential(credentialValue);
  if (!ticket || !credential) return null;

  const ticketSecretHash = await sha256Base64url(ticket.secret);
  const candidate = await db
    .select({
      id: edgeTickets.id,
      userId: edgeTickets.userId,
      payload: edgeTickets.payload,
      consumedAt: edgeTickets.consumedAt,
      expired: sql<number>`${edgeTickets.expiresAt} <= ${databaseTimestamp}`,
    })
    .from(edgeTickets)
    .where(
      and(
        eq(edgeTickets.secretHash, ticketSecretHash),
        eq(edgeTickets.purpose, "edge_deployment"),
        eq(edgeTickets.expectedEdgeId, credential.edgeId),
      ),
    )
    .get();
  if (!candidate) return null;
  const consumed = candidate.consumedAt !== null;
  // An unconsumed ticket is usable only until it expires; a consumed one stays a retry record.
  if (!consumed && candidate.expired) return null;

  let payload: DeploymentPayload | null;
  try {
    payload = decodeDeploymentPayload(JSON.parse(candidate.payload));
  } catch {
    payload = null;
  }
  if (!payload) return null;

  return {
    consumed,
    deployment: {
      ticketId: candidate.id,
      ticketSecretHash,
      userId: candidate.userId,
      hostname: payload.hostname,
      edgeId: credential.edgeId,
      credentialSecret: credential.secret,
      payload: candidate.payload,
      serviceCredentialHash: await sha256Base64url(credential.secret),
    },
  };
}

export async function enrollEdge(
  db: Database,
  ticketValue: string,
  credentialValue: string,
): Promise<
  { status: "authorized" | "existing"; edge: DeploymentIdentity } | { status: "invalid" }
> {
  const authorized = await deploymentAuthorization(db, ticketValue, credentialValue);
  if (!authorized) return { status: "invalid" };
  const { deployment } = authorized;
  if (!authorized.consumed) {
    return {
      status: "authorized",
      edge: {
        edgeId: deployment.edgeId,
        name: deployment.hostname,
        tunnelUrl: tunnelUrl(deployment.hostname, 443),
      },
    };
  }
  const existing = await completedDeployment(db, deployment);
  return existing ? { status: "existing", edge: existing } : { status: "invalid" };
}

export type OwnershipVerification = "verified" | "non_public_ip" | "failed" | "rate_limited";

export type OwnershipCandidate = {
  edgeId: string;
  hostname: string;
  port: number;
  credentialSecret: string;
};

export type ConfirmationResult =
  | { status: "created" | "existing"; edge: DeploymentIdentity }
  | { status: "invalid" | "conflict" }
  | { status: Exclude<OwnershipVerification, "verified">; edge: DeploymentIdentity };

export async function confirmEdgeDeployment(
  db: Database,
  ticketValue: string,
  credentialValue: string,
  verifyOwnership: (candidate: OwnershipCandidate) => Promise<OwnershipVerification>,
  port = 443,
): Promise<ConfirmationResult> {
  if (!isEdgePort(port)) return { status: "invalid" };
  const authorized = await deploymentAuthorization(db, ticketValue, credentialValue);
  if (!authorized) return { status: "invalid" };
  const { deployment } = authorized;
  const identity = {
    edgeId: deployment.edgeId,
    name: deployment.hostname,
    tunnelUrl: tunnelUrl(deployment.hostname, port),
  };
  const existing = async (): Promise<ConfirmationResult> => {
    const completed = await completedDeployment(db, deployment);
    return completed?.tunnelUrl === identity.tunnelUrl
      ? { status: "existing", edge: completed }
      : { status: "invalid" };
  };
  if (authorized.consumed || (await storedEdge(db, deployment.edgeId))) return existing();

  const occupied = await db
    .select({ id: edges.id })
    .from(edges)
    .where(and(eq(edges.tunnelUrl, identity.tunnelUrl), ne(edges.id, deployment.edgeId)))
    .get();
  if (occupied) return { status: "conflict" };

  const verification = await verifyOwnership({
    edgeId: deployment.edgeId,
    hostname: deployment.hostname,
    port,
    credentialSecret: deployment.credentialSecret,
  });
  if (verification !== "verified") return { status: verification, edge: identity };

  const condition = and(
    eq(edgeTickets.id, deployment.ticketId),
    eq(edgeTickets.secretHash, deployment.ticketSecretHash),
    eq(edgeTickets.purpose, "edge_deployment"),
    eq(edgeTickets.userId, deployment.userId),
    eq(edgeTickets.expectedEdgeId, deployment.edgeId),
    eq(edgeTickets.payload, deployment.payload),
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
              id: sql<string>`${deployment.edgeId}`.as("id"),
              userId: edgeTickets.userId,
              accessScope: sql<"private">`'private'`.as("access_scope"),
              name: sql<string>`${deployment.hostname}`.as("name"),
              previewEnabled: sql<number>`0`.as("preview_enabled"),
              dnsLabel: sql<null>`NULL`.as("dns_label"),
              tunnelUrl: sql<string>`${identity.tunnelUrl}`.as("tunnel_url"),
              serviceCredentialHash: sql<string>`${deployment.serviceCredentialHash}`.as(
                "service_credential_hash",
              ),
              createdAt: databaseTimestamp.as("created_at"),
              updatedAt: databaseTimestamp.as("updated_at"),
            })
            .from(edgeTickets)
            .where(condition),
        )
        .returning({ edgeId: edges.id, name: edges.name, tunnelUrl: edges.tunnelUrl }),
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
    // The batch rolls back on failure; an identical concurrent request may have won the race.
  }

  return existing();
}
