import { and, eq, gt } from "drizzle-orm";
import { base64url } from "jose";
import { sha256Base64url } from "../crypto";
import type { Database } from "../db";
import { authSessions } from "../db/schema";
import { sessionExpiry } from "./identity";
import { AuthError } from "./types";

const REFRESH_TOKEN_PREFIX = "ptb_";
const REFRESH_TOKEN_SECRET_BYTES = 32;

function parseRefreshToken(token: string): string | null {
  if (!token.startsWith(REFRESH_TOKEN_PREFIX)) return null;
  const secret = token.slice(REFRESH_TOKEN_PREFIX.length);
  try {
    const decoded = base64url.decode(secret);
    return decoded.length === REFRESH_TOKEN_SECRET_BYTES && base64url.encode(decoded) === secret
      ? secret
      : null;
  } catch {
    return null;
  }
}

export async function createBrowserRefreshToken(
  db: Database,
  sessionId: string,
  now = new Date(),
): Promise<{ token: string; expiresAt: Date }> {
  const secret = base64url.encode(
    crypto.getRandomValues(new Uint8Array(REFRESH_TOKEN_SECRET_BYTES)),
  );
  const token = `${REFRESH_TOKEN_PREFIX}${secret}`;
  const updated = await db
    .update(authSessions)
    .set({ tokenHash: await sha256Base64url(token) })
    .where(
      and(
        eq(authSessions.id, sessionId),
        eq(authSessions.kind, "browser"),
        gt(authSessions.expiresAt, now.toISOString()),
      ),
    )
    .returning({ expiresAt: authSessions.expiresAt });
  const expiresAt = updated[0]?.expiresAt;
  if (!expiresAt) throw new AuthError("invalid_credentials");
  return { token, expiresAt: new Date(expiresAt) };
}

export async function refreshBrowserSession(
  db: Database,
  token: string,
  now = new Date(),
): Promise<{ sessionId: string; userId: string; expiresAt: Date }> {
  if (!parseRefreshToken(token)) throw new AuthError("invalid_credentials");
  const expiresAt = sessionExpiry(now);
  const refreshed = await db
    .update(authSessions)
    .set({ expiresAt })
    .where(
      and(
        eq(authSessions.tokenHash, await sha256Base64url(token)),
        eq(authSessions.kind, "browser"),
        gt(authSessions.expiresAt, now.toISOString()),
      ),
    )
    .returning({ sessionId: authSessions.id, userId: authSessions.userId });
  const session = refreshed[0];
  if (!session) throw new AuthError("invalid_credentials");
  return { ...session, expiresAt: new Date(expiresAt) };
}

export async function deleteBrowserSessionByRefreshToken(
  db: Database,
  token: string,
): Promise<void> {
  if (!parseRefreshToken(token)) return;
  await db
    .delete(authSessions)
    .where(
      and(
        eq(authSessions.tokenHash, await sha256Base64url(token)),
        eq(authSessions.kind, "browser"),
      ),
    );
}
