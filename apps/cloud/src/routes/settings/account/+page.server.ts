import { fail, redirect, type RequestEvent } from "@sveltejs/kit";
import { and, asc, eq, sql } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { accounts, authSessions, users } from "$lib/server/db/schema";
import type { Actions, PageServerLoad } from "./$types";

async function authenticated(event: RequestEvent) {
  const login = await currentLogin(event);
  if (!login) return null;
  return { login, db: database(environment(event).DB) };
}

export const load: PageServerLoad = async (event) => {
  const authentication = await authenticated(event);
  if (!authentication) redirect(303, "/login");
  const { login, db } = authentication;
  const [profile, linked] = await Promise.all([
    db
      .select({
        displayName: users.displayName,
        avatarUrl: users.avatarUrl,
        createdAt: users.createdAt,
      })
      .from(users)
      .where(eq(users.id, login.user_id))
      .get(),
    db
      .select({
        id: accounts.id,
        provider: accounts.provider,
        email: accounts.email,
        emailVerified: accounts.emailVerified,
      })
      .from(accounts)
      .where(eq(accounts.userId, login.user_id))
      .orderBy(asc(accounts.provider)),
  ]);
  if (!profile) redirect(303, "/login");
  return { profile, accounts: linked, error: event.url.searchParams.get("error") };
};

function formText(data: FormData, name: string) {
  const value = data.get(name);
  return typeof value === "string" ? value : null;
}

export const actions: Actions = {
  unlinkProvider: async (event) => {
    const authentication = await authenticated(event);
    if (!authentication) return fail(401, { error: "Sign in to manage sign-in methods." });
    const provider = formText(await event.request.formData(), "provider");
    if (provider !== "google" && provider !== "github") {
      return fail(400, { error: "Select a sign-in method to unlink." });
    }
    const userId = authentication.login.user_id;
    const canUnlink = sql`EXISTS (
      SELECT 1 FROM ${accounts} other
      WHERE other.user_id = ${userId}
        AND other.provider <> ${provider}
    )`;
    const [revoked, removed] = await authentication.db.batch([
      authentication.db
        .delete(authSessions)
        .where(
          and(
            eq(authSessions.userId, userId),
            sql`${authSessions.accountId} IN (
              SELECT id FROM ${accounts}
              WHERE user_id = ${userId} AND provider = ${provider}
            )`,
            canUnlink,
          ),
        )
        .returning({ id: authSessions.id }),
      authentication.db
        .delete(accounts)
        .where(and(eq(accounts.userId, userId), eq(accounts.provider, provider), canUnlink))
        .returning({ id: accounts.id }),
    ]);
    if (removed.length !== 1) {
      return fail(409, { error: "Keep at least one sign-in method linked to your account." });
    }
    return { success: "provider_unlinked", revokedSessions: revoked.length };
  },
};
