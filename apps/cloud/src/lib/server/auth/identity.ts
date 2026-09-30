import { and, eq, gt, ne, sql } from "drizzle-orm";
import { v7 as uuidv7 } from "uuid";
import type { Database } from "../db";
import { accounts, authSessions, users } from "../db/schema";
import { AuthError, type AccountProfile } from "./types";

export const AUTH_SESSION_SECONDS = 30 * 24 * 60 * 60;

function sessionExpiry(now: Date) {
  return new Date(now.getTime() + AUTH_SESSION_SECONDS * 1000).toISOString();
}

export async function accountForProfile(db: Database, profile: AccountProfile) {
  return db
    .select()
    .from(accounts)
    .where(
      and(
        eq(accounts.provider, profile.provider),
        eq(accounts.providerSubject, profile.providerSubject),
      ),
    )
    .get();
}

export async function accountById(db: Database, id: string) {
  return db.select().from(accounts).where(eq(accounts.id, id)).get();
}

async function accountForUserProvider(
  db: Database,
  userId: string,
  provider: AccountProfile["provider"],
) {
  return db
    .select({ id: accounts.id })
    .from(accounts)
    .where(and(eq(accounts.userId, userId), eq(accounts.provider, provider)))
    .get();
}

export async function verifiedEmailCandidate(db: Database, profile: AccountProfile) {
  if (!profile.email || !profile.emailVerified) return null;
  const matches = await db
    .select({
      id: accounts.id,
      provider: accounts.provider,
      providerSubject: accounts.providerSubject,
      userId: accounts.userId,
    })
    .from(accounts)
    .where(
      and(
        ne(accounts.provider, profile.provider),
        eq(accounts.email, profile.email),
        eq(accounts.emailVerified, true),
      ),
    );
  if (matches.length > 1) throw new AuthError("account_conflict");
  return matches[0] ?? null;
}

export async function login(db: Database, profile: AccountProfile, now = new Date()) {
  const subject = and(
    eq(accounts.provider, profile.provider),
    eq(accounts.providerSubject, profile.providerSubject),
  );
  let account = await db.select().from(accounts).where(subject).get();
  if (!account) {
    const userId = uuidv7();
    const accountId = uuidv7();
    try {
      await db.batch([
        db.insert(users).values({
          id: userId,
          displayName: profile.displayName,
          avatarUrl: profile.avatarUrl,
        }),
        db.insert(accounts).values({
          id: accountId,
          userId,
          provider: profile.provider,
          providerSubject: profile.providerSubject,
          email: profile.email,
          emailVerified: profile.emailVerified,
        }),
      ]);
    } catch (cause) {
      // A concurrent first login can win the unique subject; the failed batch rolls back its user.
      if (!(await db.select({ id: accounts.id }).from(accounts).where(subject).get())) throw cause;
    }
    account = await db.select().from(accounts).where(subject).get();
  }
  if (!account) throw new AuthError("invalid_credentials");
  const id = uuidv7();
  await db.batch([
    db
      .update(accounts)
      .set({
        email: profile.email,
        emailVerified: profile.emailVerified,
        updatedAt: now.toISOString(),
      })
      .where(eq(accounts.id, account.id)),
    db.insert(authSessions).values({
      id,
      userId: account.userId,
      accountId: account.id,
      createdAt: now.toISOString(),
      expiresAt: sessionExpiry(now),
    }),
  ]);
  return id;
}

export async function completeIndependentAccount(
  db: Database,
  profile: AccountProfile,
  now = new Date(),
) {
  const userId = uuidv7();
  const accountId = uuidv7();
  const loginId = uuidv7();
  try {
    await db.batch([
      db.insert(users).values({
        id: userId,
        displayName: profile.displayName,
        avatarUrl: profile.avatarUrl,
      }),
      db.insert(accounts).values({
        id: accountId,
        userId,
        provider: profile.provider,
        providerSubject: profile.providerSubject,
        email: profile.email,
        emailVerified: profile.emailVerified,
      }),
      db.insert(authSessions).values({
        id: loginId,
        userId,
        accountId,
        createdAt: now.toISOString(),
        expiresAt: sessionExpiry(now),
      }),
    ]);
  } catch (cause) {
    if (await accountForProfile(db, profile)) throw new AuthError("account_conflict");
    throw cause;
  }
  return loginId;
}

export async function completePendingBinding(
  db: Database,
  profile: AccountProfile,
  targetAccountId: string,
  verifiedProfile: AccountProfile,
  now = new Date(),
) {
  const accountId = uuidv7();
  const loginId = uuidv7();
  const target = and(
    eq(accounts.id, targetAccountId),
    eq(accounts.provider, verifiedProfile.provider),
    eq(accounts.providerSubject, verifiedProfile.providerSubject),
  );
  try {
    const [linked, session] = await db.batch([
      db
        .insert(accounts)
        .select(
          db
            .select({
              id: sql<string>`${accountId}`.as("id"),
              userId: accounts.userId,
              provider: sql<AccountProfile["provider"]>`${profile.provider}`.as("provider"),
              providerSubject: sql<string>`${profile.providerSubject}`.as("provider_subject"),
              email: sql<string | null>`${profile.email}`.as("email"),
              emailVerified: sql<boolean>`${profile.emailVerified ? 1 : 0}`.as("email_verified"),
              createdAt: sql<string>`${now.toISOString()}`.as("created_at"),
              updatedAt: sql<string>`${now.toISOString()}`.as("updated_at"),
            })
            .from(accounts)
            .where(target),
        )
        .returning({ id: accounts.id }),
      db
        .insert(authSessions)
        .select(
          db
            .select({
              id: sql<string>`${loginId}`.as("id"),
              userId: accounts.userId,
              accountId: accounts.id,
              kind: sql<"browser">`'browser'`.as("kind"),
              tokenHash: sql<string | null>`NULL`.as("token_hash"),
              expiresAt: sql<string>`${sessionExpiry(now)}`.as("expires_at"),
              createdAt: sql<string>`${now.toISOString()}`.as("created_at"),
            })
            .from(accounts)
            .where(target),
        )
        .returning({ id: authSessions.id }),
    ]);
    if (linked.length !== 1 || session.length !== 1) throw new AuthError("invalid_credentials");
  } catch (cause) {
    if (cause instanceof AuthError) throw cause;
    if (await accountForProfile(db, profile)) throw new AuthError("account_conflict");
    const currentTarget = await accountById(db, targetAccountId);
    if (currentTarget && (await accountForUserProvider(db, currentTarget.userId, profile.provider)))
      throw new AuthError("account_conflict");
    throw cause;
  }
  return loginId;
}

export async function activeLogin(db: Database, id: string, userId?: string, now = new Date()) {
  return db
    .select({
      id: authSessions.id,
      userId: authSessions.userId,
      expiresAt: authSessions.expiresAt,
      displayName: users.displayName,
      avatarUrl: users.avatarUrl,
    })
    .from(authSessions)
    .innerJoin(users, eq(users.id, authSessions.userId))
    .where(
      and(
        eq(authSessions.id, id),
        eq(authSessions.kind, "browser"),
        gt(authSessions.expiresAt, now.toISOString()),
        userId === undefined ? undefined : eq(authSessions.userId, userId),
      ),
    )
    .get();
}

export async function bindAccount(
  db: Database,
  profile: AccountProfile,
  loginId: string,
  userId: string,
  now = new Date(),
) {
  // Check revocation in the INSERT itself, so logout cannot race a separate validation query.
  const inserted = await db
    .insert(accounts)
    .select(
      db
        .select({
          id: sql<string>`${uuidv7()}`.as("id"),
          userId: authSessions.userId,
          provider: sql<AccountProfile["provider"]>`${profile.provider}`.as("provider"),
          providerSubject: sql<string>`${profile.providerSubject}`.as("provider_subject"),
          email: sql<string | null>`${profile.email}`.as("email"),
          emailVerified: sql<boolean>`${profile.emailVerified ? 1 : 0}`.as("email_verified"),
          createdAt: sql<string>`${now.toISOString()}`.as("created_at"),
          updatedAt: sql<string>`${now.toISOString()}`.as("updated_at"),
        })
        .from(authSessions)
        .where(
          and(
            eq(authSessions.id, loginId),
            eq(authSessions.userId, userId),
            eq(authSessions.kind, "browser"),
            gt(authSessions.expiresAt, now.toISOString()),
          ),
        ),
    )
    .onConflictDoNothing()
    .returning({ id: accounts.id });
  if (!inserted.length) {
    if (!(await activeLogin(db, loginId, userId, now))) throw new AuthError("invalid_credentials");
    throw new AuthError("account_conflict");
  }
}

export async function logout(db: Database, id: string, userId: string) {
  await db
    .delete(authSessions)
    .where(
      and(
        eq(authSessions.id, id),
        eq(authSessions.userId, userId),
        eq(authSessions.kind, "browser"),
      ),
    );
}
