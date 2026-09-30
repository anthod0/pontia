import { base64url, EncryptJWT, jwtDecrypt } from "jose";
import type { AccountProfile, Provider } from "./types";
import { isValidLoginReturnTo } from "./return-to";
import { AuthError } from "./types";

export interface PendingAccountLink {
  profile: AccountProfile;
  targetAccountId: string;
  returnTo?: string;
  jti: string;
  iat: number;
  exp: number;
}

export const PENDING_ACCOUNT_SECONDS = 5 * 60;

async function encryptionKey(secret: string) {
  const bytes = new TextEncoder().encode(secret);
  if (bytes.length < 32)
    throw new Error("Authentication signing keys must contain at least 32 bytes");
  return new Uint8Array(
    await crypto.subtle.digest(
      "SHA-256",
      new TextEncoder().encode(`pontia:pending-account:${secret}`),
    ),
  );
}

function isProvider(value: unknown): value is Provider {
  return value === "google" || value === "github";
}

function nullableText(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

export async function issuePendingAccount(
  secret: string,
  profile: AccountProfile,
  targetAccountId: string,
  returnTo: string | undefined,
  now = new Date(),
) {
  const issuedAt = Math.floor(now.getTime() / 1000);
  const jti = base64url.encode(crypto.getRandomValues(new Uint8Array(32)));
  const token = await new EncryptJWT({
    profile,
    targetAccountId,
    returnTo,
  })
    .setProtectedHeader({
      alg: "dir",
      enc: "A256GCM",
      typ: "pending-account+jwe",
    })
    .setJti(jti)
    .setIssuedAt(issuedAt)
    .setExpirationTime(issuedAt + PENDING_ACCOUNT_SECONDS)
    .encrypt(await encryptionKey(secret));
  return { token, jti, expiresAt: issuedAt + PENDING_ACCOUNT_SECONDS };
}

export async function readPendingAccount(
  secret: string,
  token: string,
  now = new Date(),
): Promise<PendingAccountLink> {
  try {
    const { payload } = await jwtDecrypt(token, await encryptionKey(secret), {
      keyManagementAlgorithms: ["dir"],
      contentEncryptionAlgorithms: ["A256GCM"],
      typ: "pending-account+jwe",
      currentDate: now,
      maxTokenAge: PENDING_ACCOUNT_SECONDS,
      requiredClaims: ["exp", "iat", "jti"],
    });
    const profile = payload.profile as Record<string, unknown> | undefined;
    if (
      !profile ||
      !isProvider(profile.provider) ||
      typeof profile.providerSubject !== "string" ||
      !profile.providerSubject ||
      typeof profile.email !== "string" ||
      !profile.email ||
      profile.emailVerified !== true ||
      !nullableText(profile.displayName) ||
      !nullableText(profile.avatarUrl) ||
      typeof payload.targetAccountId !== "string" ||
      !payload.targetAccountId ||
      !isValidLoginReturnTo(payload.returnTo) ||
      typeof payload.jti !== "string" ||
      !payload.jti ||
      !Number.isSafeInteger(payload.iat) ||
      !Number.isSafeInteger(payload.exp) ||
      (payload.exp as number) <= (payload.iat as number) ||
      (payload.exp as number) > (payload.iat as number) + PENDING_ACCOUNT_SECONDS
    )
      throw new AuthError("invalid_oauth");
    return {
      profile: profile as unknown as AccountProfile,
      targetAccountId: payload.targetAccountId,
      returnTo: payload.returnTo,
      jti: payload.jti,
      iat: payload.iat as number,
      exp: payload.exp as number,
    };
  } catch {
    throw new AuthError("invalid_oauth");
  }
}
