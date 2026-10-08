import { isEdgePort, edgeAuthority } from "../edge-port";
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
  heroName(db: Database): Promise<string>;
};

const defaultDependencies: DeploymentDependencies = {
  now: () => new Date(),
  randomBytes: (length) => crypto.getRandomValues(new Uint8Array(length)),
  edgeId: uuidv7,
  heroName: generateHeroName,
};

export async function decodeDeploymentPayload(
  db: Database,
  value: unknown,
): Promise<DeploymentPayload | null> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const keys = Object.keys(value);
  if (keys.length !== 1 || keys[0] !== "name") return null;
  const name = (value as Record<string, unknown>).name;
  return typeof name === "string" && (await isHeroName(db, name)) ? { name } : null;
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

export function deploymentCommand(origin: string, edgeId: string, ticket: string) {
  return `curl -fsSL ${shellQuote("https://get.pontia.dev/install-edge.sh")} | sudo sh &&\nsudo pontia-edge init \\\n  --cloud-origin ${shellQuote(origin)} \\\n  --edge-id ${shellQuote(edgeId)} \\\n  --ticket ${shellQuote(ticket)}`;
}

async function unusedHeroName(db: Database, dependencies: DeploymentDependencies) {
  for (let attempt = 0; attempt < MAX_NAME_ATTEMPTS; attempt += 1) {
    const name = await dependencies.heroName(db);
    if (!(await isHeroName(db, name))) {
      throw new Error("Hero name generator returned an invalid name");
    }
    const payload = JSON.stringify({ name });
    const [edge, ticket] = await Promise.all([
      db.select({ id: edges.id }).from(edges).where(eq(edges.dnsLabel, name)).get(),
      db
        .select({ id: edgeTickets.id })
        .from(edgeTickets)
        .where(and(eq(edgeTickets.purpose, "edge_deployment"), eq(edgeTickets.payload, payload)))
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
      decodePayload: (value) => decodeDeploymentPayload(db, value),
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

export type DeploymentIdentity = { edgeId: string; name: string; tunnelUrl: string };

type DeploymentAuthorization = {
  ticketId: number;
  ticketSecretHash: string;
  userId: string;
  identity: DeploymentIdentity;
  payload: string;
  serviceCredentialHash: string;
};

type StoredEdge = Awaited<ReturnType<typeof storedEdge>>;

function storedEdge(db: Database, edgeId: string) {
  return db
    .select({
      edgeId: edges.id,
      userId: edges.userId,
      name: edges.name,
      dnsLabel: edges.dnsLabel,
      tunnelUrl: edges.tunnelUrl,
      accessScope: edges.accessScope,
      serviceCredentialHash: edges.serviceCredentialHash,
    })
    .from(edges)
    .where(eq(edges.id, edgeId))
    .get();
}

function matchesDeployment(edge: StoredEdge, deployment: DeploymentAuthorization) {
  return (
    edge !== undefined &&
    edge.edgeId === deployment.identity.edgeId &&
    edge.userId === deployment.userId &&
    edge.dnsLabel === deployment.identity.name &&
    edge.accessScope === "private" &&
    edge.serviceCredentialHash === deployment.serviceCredentialHash
  );
}

async function completedDeployment(db: Database, deployment: DeploymentAuthorization) {
  const [edge, ticket] = await Promise.all([
    storedEdge(db, deployment.identity.edgeId),
    db
      .select({ consumedAt: edgeTickets.consumedAt })
      .from(edgeTickets)
      .where(eq(edgeTickets.id, deployment.ticketId))
      .get(),
  ]);
  return ticket?.consumedAt && matchesDeployment(edge, deployment) && edge
    ? { edgeId: edge.edgeId, name: edge.name, tunnelUrl: edge.tunnelUrl }
    : null;
}

async function deploymentAuthorization(
  db: Database,
  ticketValue: string,
  credentialValue: string,
  includeConsumed: boolean,
): Promise<{ deployment: DeploymentAuthorization; consumed: boolean } | null> {
  const ticket = parseEdgeTicket(ticketValue);
  const credential = parseEdgeCredential(credentialValue);
  if (!ticket || !credential) return null;

  const ticketSecretHash = await sha256Base64url(ticket.secret);
  const candidate = await db
    .select({
      id: edgeTickets.id,
      userId: edgeTickets.userId,
      expectedEdgeId: edgeTickets.expectedEdgeId,
      payload: edgeTickets.payload,
      consumedAt: edgeTickets.consumedAt,
    })
    .from(edgeTickets)
    .where(
      and(
        eq(edgeTickets.secretHash, ticketSecretHash),
        eq(edgeTickets.purpose, "edge_deployment"),
        eq(edgeTickets.expectedEdgeId, credential.edgeId),
        includeConsumed ? undefined : isNull(edgeTickets.consumedAt),
        includeConsumed ? undefined : gt(edgeTickets.expiresAt, databaseTimestamp),
      ),
    )
    .get();
  if (!candidate) return null;

  let payload: DeploymentPayload | null;
  try {
    payload = await decodeDeploymentPayload(db, JSON.parse(candidate.payload));
  } catch {
    payload = null;
  }
  if (!payload) return null;

  return {
    consumed: candidate.consumedAt !== null,
    deployment: {
      ticketId: candidate.id,
      ticketSecretHash,
      userId: candidate.userId,
      identity: {
        edgeId: credential.edgeId,
        name: payload.name,
        tunnelUrl: `wss://${payload.name}.edge.pontia.dev/tunnel`,
      },
      payload: candidate.payload,
      serviceCredentialHash: await sha256Base64url(credential.secret),
    },
  };
}

export async function authorizeEdgeDeployment(
  db: Database,
  ticketValue: string,
  credentialValue: string,
): Promise<DeploymentIdentity | null> {
  const authorized = await deploymentAuthorization(db, ticketValue, credentialValue, false);
  return authorized?.deployment.identity ?? null;
}

export async function enrollEdge(
  db: Database,
  ticketValue: string,
  credentialValue: string,
): Promise<
  { status: "authorized" | "existing"; edge: DeploymentIdentity } | { status: "invalid" }
> {
  const authorized = await deploymentAuthorization(db, ticketValue, credentialValue, true);
  if (!authorized) return { status: "invalid" };
  if (authorized.consumed) {
    const existing = await completedDeployment(db, authorized.deployment);
    return existing ? { status: "existing", edge: existing } : { status: "invalid" };
  }
  const active = await authorizeEdgeDeployment(db, ticketValue, credentialValue);
  if (active) return { status: "authorized", edge: active };
  const existing = await completedDeployment(db, authorized.deployment);
  return existing ? { status: "existing", edge: existing } : { status: "invalid" };
}

export type ConfirmationResult =
  | { status: "created" | "existing"; edge: DeploymentIdentity }
  | { status: "invalid" }
  | { status: "unhealthy"; edge: DeploymentIdentity };

export async function confirmEdgeDeployment(
  db: Database,
  ticketValue: string,
  credentialValue: string,
  verifyHealth: (identity: DeploymentIdentity) => Promise<boolean>,
  port = 443,
): Promise<ConfirmationResult> {
  if (!isEdgePort(port)) return { status: "invalid" };
  const authorized = await deploymentAuthorization(db, ticketValue, credentialValue, true);
  if (!authorized) return { status: "invalid" };
  const { deployment } = authorized;
  if (authorized.consumed) {
    const existing = await completedDeployment(db, deployment);
    return existing ? { status: "existing", edge: existing } : { status: "invalid" };
  }

  const existing = await storedEdge(db, deployment.identity.edgeId);
  if (existing) {
    const completed = await completedDeployment(db, deployment);
    return completed ? { status: "existing", edge: completed } : { status: "invalid" };
  }

  const active = await authorizeEdgeDeployment(db, ticketValue, credentialValue);
  if (!active) return { status: "invalid" };
  active.tunnelUrl = `wss://${edgeAuthority(`${active.name}.edge.pontia.dev`, port)}/tunnel`;
  deployment.identity.tunnelUrl = active.tunnelUrl;
  if (!(await verifyHealth(active))) return { status: "unhealthy", edge: active };

  const condition = and(
    eq(edgeTickets.id, deployment.ticketId),
    eq(edgeTickets.secretHash, deployment.ticketSecretHash),
    eq(edgeTickets.purpose, "edge_deployment"),
    eq(edgeTickets.userId, deployment.userId),
    eq(edgeTickets.expectedEdgeId, deployment.identity.edgeId),
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
              id: sql<string>`${deployment.identity.edgeId}`.as("id"),
              userId: edgeTickets.userId,
              accessScope: sql<"private">`'private'`.as("access_scope"),
              name: sql<string>`${deployment.identity.name}`.as("name"),
              dnsLabel: sql<string>`${deployment.identity.name}`.as("dns_label"),
              tunnelUrl: sql<string>`${deployment.identity.tunnelUrl}`.as("tunnel_url"),
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

  const completed = await completedDeployment(db, deployment);
  return completed ? { status: "existing", edge: completed } : { status: "invalid" };
}
