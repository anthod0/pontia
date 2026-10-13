import { and, eq, gt, isNull, or } from "drizzle-orm";
import { base64url } from "jose";
import { sha256Base64url } from "../crypto";
import { isUuidV7 } from "../uuid";
import type { Database } from "../db";
import { authSessions } from "../db/schema";

export type CliPrincipal = {
  userId: string;
  sessionId: string;
};

function parseCliCredential(credential: string) {
  const match = /^ptr_v1_([0-9a-f-]+)_([A-Za-z0-9_-]{43})$/.exec(credential);
  if (!match) return null;
  const [, sessionId, secret] = match;
  if (!isUuidV7(sessionId)) return null;
  try {
    const decoded = base64url.decode(secret);
    if (decoded.length !== 32 || base64url.encode(decoded) !== secret) return null;
  } catch {
    return null;
  }
  return { sessionId, secret };
}

export async function authenticateCliCredential(
  db: Database,
  credential: string,
  now = new Date(),
): Promise<CliPrincipal | null> {
  const parsed = parseCliCredential(credential);
  if (!parsed) return null;
  const session = await db
    .update(authSessions)
    .set({ lastSeenAt: now.toISOString() })
    .where(
      and(
        eq(authSessions.id, parsed.sessionId),
        eq(authSessions.kind, "cli"),
        eq(authSessions.tokenHash, await sha256Base64url(parsed.secret)),
        or(isNull(authSessions.expiresAt), gt(authSessions.expiresAt, now.toISOString())),
      ),
    )
    .returning({
      userId: authSessions.userId,
      sessionId: authSessions.id,
    })
    .get();
  return session ?? null;
}
