import { error, isRedirect, redirect, type Cookies, type RequestEvent } from "@sveltejs/kit";
import { database, type Database } from "../db";
import {
  accountById,
  accountForProfile,
  activeLogin,
  bindAccount,
  completeIndependentAccount,
  completePendingBinding,
  login,
  logout,
  verifiedEmailCandidate,
} from "./identity";
import { issueLogin, signedLogin, verifyLogin } from "./jwt";
import { beginOAuth, exchangeAccount, OAUTH_SECONDS, readOAuth } from "./oauth";
import { normalizeLoginReturnTo } from "./return-to";
import { issuePendingAccount, PENDING_ACCOUNT_SECONDS, readPendingAccount } from "./pending";
import {
  createBrowserRefreshToken,
  deleteBrowserSessionByRefreshToken,
  refreshBrowserSession,
} from "./refresh";
import { AuthError, type Provider } from "./types";

const LOGIN_COOKIE = "_at";
const REFRESH_COOKIE = "_rt";
const OAUTH_COOKIE = "_oauth";
const PENDING_ACCOUNT_COOKIE = "_pending_account";
const cookieOptions = {
  path: "/",
  httpOnly: true,
  secure: true,
  sameSite: "lax" as const,
};

export function environment(event: Pick<RequestEvent, "platform">) {
  if (!event.platform) error(503, "Authentication is unavailable");
  return event.platform.env;
}

function provider(event: RequestEvent): Provider {
  if (event.params.provider !== "google" && event.params.provider !== "github")
    error(404, "Unknown provider");
  return event.params.provider;
}

export function origin(event: RequestEvent) {
  const configuredUrl = new URL(environment(event).AUTH_ORIGIN);
  if (configuredUrl.protocol !== "https:") error(503, "Authentication requires HTTPS");
  const configured = configuredUrl.origin;
  if (event.url.origin !== configured) error(400, "Use the configured cloud address to sign in");
  return configured;
}

export function loginReturnTo(event: RequestEvent) {
  const value = event.url.searchParams.get("return_to");
  if (!value) return undefined;
  const destination = normalizeLoginReturnTo(value, origin(event));
  if (!destination) error(400, "Invalid sign-in destination");
  return destination;
}

function sameOriginPost(event: RequestEvent) {
  if (event.request.headers.get("origin") !== origin(event)) error(403, "Invalid request origin");
}

export async function currentLogin(event: Pick<RequestEvent, "platform" | "cookies">) {
  const token = event.cookies.get(LOGIN_COOKIE);
  const refreshToken = event.cookies.get(REFRESH_COOKIE);
  if (!token && !refreshToken) return null;
  const env = environment(event);
  if (token) {
    try {
      return await verifyLogin(token, env.JWT_SECRET);
    } catch (cause) {
      if (!(cause instanceof AuthError)) throw cause;
    }
  }

  if (!refreshToken) return null;
  try {
    const db = database(env.DB);
    const session = await refreshBrowserSession(db, refreshToken);
    const credential = await issueLogin(
      db,
      session.sessionId,
      env.JWT_SECRET,
      new Date(),
      session.userId,
    );
    event.cookies.set(LOGIN_COOKIE, credential.token, {
      ...cookieOptions,
      expires: credential.expiresAt,
    });
    event.cookies.set(REFRESH_COOKIE, refreshToken, {
      ...cookieOptions,
      expires: session.expiresAt,
    });
    return await verifyLogin(credential.token, env.JWT_SECRET);
  } catch (cause) {
    if (!(cause instanceof AuthError)) throw cause;
    event.cookies.delete(LOGIN_COOKIE, cookieOptions);
    event.cookies.delete(REFRESH_COOKIE, cookieOptions);
    return null;
  }
}

export function clearPendingAccount(cookies: Cookies) {
  cookies.delete(PENDING_ACCOUNT_COOKIE, cookieOptions);
}

function clearCookies(cookies: Cookies) {
  cookies.delete(LOGIN_COOKIE, cookieOptions);
  cookies.delete(REFRESH_COOKIE, cookieOptions);
  cookies.delete(OAUTH_COOKIE, cookieOptions);
  clearPendingAccount(cookies);
}

async function establishBrowserLogin(
  event: RequestEvent,
  db: Database,
  loginId: string,
  secret: string,
) {
  const [credential, refresh] = await Promise.all([
    issueLogin(db, loginId, secret),
    createBrowserRefreshToken(db, loginId),
  ]);
  event.cookies.set(LOGIN_COOKIE, credential.token, {
    ...cookieOptions,
    expires: credential.expiresAt,
  });
  event.cookies.set(REFRESH_COOKIE, refresh.token, {
    ...cookieOptions,
    expires: refresh.expiresAt,
  });
}

export async function startLogin(event: RequestEvent) {
  sameOriginPost(event);
  clearPendingAccount(event.cookies);
  const selected = provider(event);
  const result = await beginOAuth(
    environment(event),
    selected,
    `${origin(event)}/api/auth/${selected}/callback`,
    { kind: "login", returnTo: loginReturnTo(event) },
  );
  event.cookies.set(OAUTH_COOKIE, result.cookie, {
    ...cookieOptions,
    maxAge: OAUTH_SECONDS,
  });
  redirect(303, result.url);
}

export async function startBinding(event: RequestEvent) {
  sameOriginPost(event);
  const selected = provider(event);
  const claims = await currentLogin(event);
  const env = environment(event);
  if (!claims || !(await activeLogin(database(env.DB), claims.sub, claims.user_id)))
    redirect(303, "/login?error=invalid_credentials");
  const result = await beginOAuth(env, selected, `${origin(event)}/api/auth/${selected}/callback`, {
    kind: "bind",
    loginId: claims.sub,
    userId: claims.user_id,
  });
  event.cookies.set(OAUTH_COOKIE, result.cookie, {
    ...cookieOptions,
    maxAge: OAUTH_SECONDS,
  });
  redirect(303, result.url);
}

export async function pendingAccountSummary(event: RequestEvent) {
  const token = event.cookies.get(PENDING_ACCOUNT_COOKIE);
  if (!token) throw new AuthError("invalid_oauth");
  const env = environment(event);
  const pending = await readPendingAccount(env.OAUTH_COOKIE_SECRET, token);
  const target = await accountById(database(env.DB), pending.targetAccountId);
  if (!target || target.provider === pending.profile.provider) throw new AuthError("invalid_oauth");
  return { email: pending.profile.email!, provider: target.provider };
}

export async function startPendingBinding(event: RequestEvent) {
  sameOriginPost(event);
  const token = event.cookies.get(PENDING_ACCOUNT_COOKIE);
  try {
    if (!token) throw new AuthError("invalid_oauth");
    const env = environment(event);
    const pending = await readPendingAccount(env.OAUTH_COOKIE_SECRET, token);
    const target = await accountById(database(env.DB), pending.targetAccountId);
    if (!target || target.provider === pending.profile.provider)
      throw new AuthError("invalid_oauth");
    const result = await beginOAuth(
      env,
      target.provider,
      `${origin(event)}/api/auth/${target.provider}/callback`,
      { kind: "pending_bind", pendingJti: pending.jti },
      new Date(),
      pending.exp,
    );
    event.cookies.set(OAUTH_COOKIE, result.cookie, {
      ...cookieOptions,
      maxAge: Math.min(OAUTH_SECONDS, Math.max(1, pending.exp - Math.floor(Date.now() / 1000))),
    });
    redirect(303, result.url);
  } catch (cause) {
    if (isRedirect(cause)) throw cause;
    clearPendingAccount(event.cookies);
    if (!(cause instanceof AuthError)) throw cause;
    redirect(303, `/login?error=${cause.code}`);
  }
}

export async function createIndependentAccount(event: RequestEvent) {
  sameOriginPost(event);
  const token = event.cookies.get(PENDING_ACCOUNT_COOKIE);
  try {
    if (!token) throw new AuthError("invalid_oauth");
    const env = environment(event);
    const pending = await readPendingAccount(env.OAUTH_COOKIE_SECRET, token);
    const db = database(env.DB);
    const loginId = await completeIndependentAccount(db, pending.profile);
    await establishBrowserLogin(event, db, loginId, env.JWT_SECRET);
    clearPendingAccount(event.cookies);
    redirect(303, pending.returnTo ?? "/account");
  } catch (cause) {
    clearPendingAccount(event.cookies);
    if (!(cause instanceof AuthError)) throw cause;
    redirect(303, `/login?error=${cause.code}`);
  }
}

export async function callback(event: RequestEvent) {
  const selected = provider(event);
  const env = environment(event);
  const callbackUri = `${origin(event)}/api/auth/${selected}/callback`;
  const cookie = event.cookies.get(OAUTH_COOKIE);
  event.cookies.delete(OAUTH_COOKIE, cookieOptions);
  let destination = "/login";
  try {
    if (!cookie) throw new AuthError("invalid_oauth");
    const oauth = await readOAuth(
      env,
      cookie,
      selected,
      event.url.searchParams.get("state") ?? "",
      callbackUri,
    );
    destination = oauth.intent.kind === "bind" ? "/account" : "/login";
    const code = event.url.searchParams.get("code");
    if (!code || event.url.searchParams.has("error")) throw new AuthError("invalid_oauth");
    const db = database(env.DB);
    if (oauth.intent.kind === "bind") {
      const claims = await currentLogin(event);
      if (
        !claims ||
        claims.sub !== oauth.intent.loginId ||
        claims.user_id !== oauth.intent.userId ||
        !(await activeLogin(db, claims.sub, claims.user_id))
      )
        throw new AuthError("invalid_credentials");
      const profile = await exchangeAccount(env, oauth, code);
      await bindAccount(db, profile, claims.sub, claims.user_id);
    } else if (oauth.intent.kind === "pending_bind") {
      const pendingToken = event.cookies.get(PENDING_ACCOUNT_COOKIE);
      if (!pendingToken) throw new AuthError("invalid_oauth");
      const pending = await readPendingAccount(env.OAUTH_COOKIE_SECRET, pendingToken);
      if (pending.jti !== oauth.intent.pendingJti) throw new AuthError("invalid_oauth");
      const verifiedProfile = await exchangeAccount(env, oauth, code);
      const loginId = await completePendingBinding(
        db,
        pending.profile,
        pending.targetAccountId,
        verifiedProfile,
      );
      await establishBrowserLogin(event, db, loginId, env.JWT_SECRET);
      clearPendingAccount(event.cookies);
      destination = pending.returnTo ?? "/account";
    } else {
      const profile = await exchangeAccount(env, oauth, code);
      if (!(await accountForProfile(db, profile))) {
        const candidate = await verifiedEmailCandidate(db, profile);
        if (candidate) {
          const pending = await issuePendingAccount(
            env.OAUTH_COOKIE_SECRET,
            profile,
            candidate.id,
            oauth.intent.returnTo,
          );
          event.cookies.set(PENDING_ACCOUNT_COOKIE, pending.token, {
            ...cookieOptions,
            maxAge: PENDING_ACCOUNT_SECONDS,
          });
          redirect(303, "/auth/account-conflict");
        }
      }
      const loginId = await login(db, profile);
      await establishBrowserLogin(event, db, loginId, env.JWT_SECRET);
      destination = oauth.intent.returnTo ?? "/account";
    }
  } catch (cause) {
    if (isRedirect(cause)) throw cause;
    clearPendingAccount(event.cookies);
    if (!(cause instanceof AuthError)) throw cause;
    redirect(303, `${destination}?error=${cause.code}`);
  }
  redirect(303, destination);
}

export async function endLogin(event: RequestEvent) {
  sameOriginPost(event);
  const token = event.cookies.get(LOGIN_COOKIE);
  const refreshToken = event.cookies.get(REFRESH_COOKIE);
  try {
    const env = environment(event);
    const db = database(env.DB);
    if (token) {
      let claims;
      try {
        claims = await signedLogin(token, env.JWT_SECRET);
      } catch (cause) {
        if (!(cause instanceof AuthError)) throw cause;
      }
      if (claims) await logout(db, claims.sub, claims.user_id);
    }
    if (refreshToken) await deleteBrowserSessionByRefreshToken(db, refreshToken);
  } finally {
    clearCookies(event.cookies);
  }
  redirect(303, "/login");
}
