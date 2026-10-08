import { fail, redirect, type RequestEvent } from "@sveltejs/kit";
import { and, asc, eq, gt, isNull, ne, or } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { accounts, authSessions } from "$lib/server/db/schema";
import type { Actions, PageServerLoad } from "./$types";

async function authenticated(event: RequestEvent) {
  const login = await currentLogin(event);
  if (!login) return null;
  return { login, db: database(environment(event).DB) };
}

export const load: PageServerLoad = async (event) => {
  const authentication = await authenticated(event);
  if (!authentication) redirect(303, "/login");
  const sessions = await authentication.db
    .select({
      id: authSessions.id,
      kind: authSessions.kind,
      provider: accounts.provider,
      createdAt: authSessions.createdAt,
      expiresAt: authSessions.expiresAt,
    })
    .from(authSessions)
    .leftJoin(accounts, eq(authSessions.accountId, accounts.id))
    .where(
      and(
        eq(authSessions.userId, authentication.login.user_id),
        or(isNull(authSessions.expiresAt), gt(authSessions.expiresAt, new Date().toISOString())),
      ),
    )
    .orderBy(asc(authSessions.createdAt), asc(authSessions.id));
  return {
    sessions: sessions.map((session) => ({
      ...session,
      current: session.id === authentication.login.sub,
    })),
  };
};

function sessionId(data: FormData) {
  const value = data.get("session_id");
  return typeof value === "string" ? value : null;
}

export const actions: Actions = {
  revokeSession: async (event) => {
    const authentication = await authenticated(event);
    if (!authentication) return fail(401, { error: "Sign in to manage sessions." });
    const id = sessionId(await event.request.formData());
    if (!id) return fail(400, { error: "Select a session to revoke." });
    await authentication.db
      .delete(authSessions)
      .where(and(eq(authSessions.id, id), eq(authSessions.userId, authentication.login.user_id)));
    return { success: "session_revoked" };
  },

  revokeOtherBrowsers: async (event) => {
    const authentication = await authenticated(event);
    if (!authentication) return fail(401, { error: "Sign in to manage sessions." });
    await authentication.db
      .delete(authSessions)
      .where(
        and(
          eq(authSessions.userId, authentication.login.user_id),
          eq(authSessions.kind, "browser"),
          ne(authSessions.id, authentication.login.sub),
        ),
      );
    return { success: "other_browsers_revoked" };
  },

  revokeCli: async (event) => {
    const authentication = await authenticated(event);
    if (!authentication) return fail(401, { error: "Sign in to manage credentials." });
    await authentication.db
      .delete(authSessions)
      .where(
        and(eq(authSessions.userId, authentication.login.user_id), eq(authSessions.kind, "cli")),
      );
    return { success: "cli_revoked" };
  },
};
