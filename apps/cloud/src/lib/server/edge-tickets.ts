import { and, eq, gt, isNull, type SQL, sql } from "drizzle-orm";
import { base64url } from "jose";
import { sha256Base64url } from "./crypto";
import type { Database } from "./db";
import { edgeTickets } from "./db/schema";

const TICKET_PREFIX = "pet_v1_";
const SECRET_BYTES = 32;
const SECRET_LENGTH = 43;
const databaseTimestamp = sql<string>`(strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))`;

export type EdgeTicketPurpose = "edge_deployment" | "device_tunnel";
export type EdgeTicketPayloadDecoder<T> = (value: unknown) => T | null | Promise<T | null>;

export type EdgeTicketDependencies = {
  now(): Date;
  randomBytes(length: number): Uint8Array;
};

const defaultDependencies: EdgeTicketDependencies = {
  now: () => new Date(),
  randomBytes: (length) => crypto.getRandomValues(new Uint8Array(length)),
};

export function parseEdgeTicket(ticket: string): { secret: string } | null {
  if (!ticket.startsWith(TICKET_PREFIX)) return null;
  const secret = ticket.slice(TICKET_PREFIX.length);
  if (secret.length !== SECRET_LENGTH || !/^[A-Za-z0-9_-]+$/.test(secret)) return null;
  try {
    const decoded = base64url.decode(secret);
    if (decoded.length !== SECRET_BYTES || base64url.encode(decoded) !== secret) return null;
  } catch {
    return null;
  }
  return { secret };
}

export async function issueEdgeTicket<T>(
  db: Database,
  input: {
    purpose: EdgeTicketPurpose;
    userId: string;
    expectedEdgeId: string;
    payload: T;
    expiresAt: Date;
    createdAt?: Date;
    decodePayload: EdgeTicketPayloadDecoder<T>;
  },
  dependencies: EdgeTicketDependencies = defaultDependencies,
): Promise<string> {
  const payload = JSON.stringify(input.payload);
  if ((await input.decodePayload(JSON.parse(payload))) === null) {
    throw new Error("Edge ticket payload encoder produced an invalid payload");
  }

  const bytes = dependencies.randomBytes(SECRET_BYTES);
  if (bytes.length !== SECRET_BYTES) {
    throw new Error("Edge ticket secret generator returned an invalid length");
  }
  const secret = base64url.encode(bytes);

  await db.insert(edgeTickets).values({
    purpose: input.purpose,
    secretHash: await sha256Base64url(secret),
    userId: input.userId,
    expectedEdgeId: input.expectedEdgeId,
    payload,
    expiresAt: input.expiresAt.toISOString(),
    createdAt: (input.createdAt ?? dependencies.now()).toISOString(),
  });

  return `${TICKET_PREFIX}${secret}`;
}

export async function consumeEdgeTicket<T>(
  db: Database,
  input: {
    ticket: string;
    purpose: EdgeTicketPurpose;
    expectedEdgeId: string;
    decodePayload: EdgeTicketPayloadDecoder<T>;
    additionalCondition?(ticket: {
      userId: string;
      expectedEdgeId: string;
      payload: T;
      createdAt: string;
    }): SQL;
  },
): Promise<{
  userId: string;
  expectedEdgeId: string;
  payload: T;
  createdAt: string;
} | null> {
  const parsed = parseEdgeTicket(input.ticket);
  if (!parsed) return null;
  const secretHash = await sha256Base64url(parsed.secret);
  const candidate = await db
    .select({
      id: edgeTickets.id,
      userId: edgeTickets.userId,
      expectedEdgeId: edgeTickets.expectedEdgeId,
      payload: edgeTickets.payload,
      createdAt: edgeTickets.createdAt,
    })
    .from(edgeTickets)
    .where(
      and(
        eq(edgeTickets.secretHash, secretHash),
        eq(edgeTickets.purpose, input.purpose),
        eq(edgeTickets.expectedEdgeId, input.expectedEdgeId),
        isNull(edgeTickets.consumedAt),
        gt(edgeTickets.expiresAt, databaseTimestamp),
      ),
    )
    .get();
  if (!candidate) return null;

  let decoded: T | null;
  try {
    decoded = await input.decodePayload(JSON.parse(candidate.payload));
  } catch {
    return null;
  }
  if (decoded === null) return null;

  const ticket = {
    userId: candidate.userId,
    expectedEdgeId: candidate.expectedEdgeId,
    payload: decoded,
    createdAt: candidate.createdAt,
  };
  const consumed = await db
    .update(edgeTickets)
    .set({ consumedAt: databaseTimestamp })
    .where(
      and(
        eq(edgeTickets.id, candidate.id),
        eq(edgeTickets.secretHash, secretHash),
        eq(edgeTickets.purpose, input.purpose),
        eq(edgeTickets.expectedEdgeId, input.expectedEdgeId),
        eq(edgeTickets.userId, candidate.userId),
        eq(edgeTickets.payload, candidate.payload),
        eq(edgeTickets.createdAt, candidate.createdAt),
        isNull(edgeTickets.consumedAt),
        gt(edgeTickets.expiresAt, databaseTimestamp),
        input.additionalCondition?.(ticket),
      ),
    )
    .returning({ id: edgeTickets.id })
    .get();

  return consumed ? ticket : null;
}
