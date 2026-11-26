import { base64url, SignJWT } from "jose";

function buffer(value: Uint8Array): ArrayBuffer {
  return value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength) as ArrayBuffer;
}

export function capabilitySigningKey(signingKeyPkcs8: string) {
  return crypto.subtle.importKey(
    "pkcs8",
    buffer(base64url.decode(signingKeyPkcs8)),
    { name: "Ed25519" },
    false,
    ["sign"],
  );
}

export type EdgeControlToken =
  | { purpose: "version"; edgeId: string }
  | { purpose: "update"; edgeId: string; nonce: string }
  | {
      purpose: "ownership";
      edgeId: string;
      nonce: string;
      hostname: string;
      port: number;
    };

const tokenTypes = {
  version: "pontia-edge-version+jwt",
  update: "pontia-edge-update+jwt",
  ownership: "pontia-edge-ownership+jwt",
} as const;

export async function signEdgeControlToken(key: CryptoKey, token: EdgeControlToken) {
  const now = Math.floor(Date.now() / 1000);
  const claims = token.purpose === "ownership" ? { host: token.hostname, port: token.port } : {};
  let jwt = new SignJWT(claims)
    .setProtectedHeader({ alg: "EdDSA", typ: tokenTypes[token.purpose] })
    .setAudience(token.edgeId)
    .setIssuedAt(now)
    .setExpirationTime(now + 60);
  if (token.purpose !== "version") jwt = jwt.setJti(token.nonce);
  return jwt.sign(key);
}
