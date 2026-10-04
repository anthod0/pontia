import { base64url } from "jose";

const LABEL = new TextEncoder().encode("pontia-device-key-registration-v1\0");

function buffer(value: Uint8Array): ArrayBuffer {
  return value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength) as ArrayBuffer;
}

function message(deviceId: string, keyVersion: number, publicKey: Uint8Array): Uint8Array {
  const id = new TextEncoder().encode(deviceId);
  const output = new Uint8Array(id.length + 1 + 8 + publicKey.length);
  output.set(id);
  new DataView(output.buffer).setBigUint64(id.length + 1, BigInt(keyVersion));
  output.set(publicKey, id.length + 9);
  return output;
}

export async function verifyDeviceKeyProof(
  deviceId: string,
  keyVersion: number,
  publicKeyWire: string,
  proofWire: string,
  cloudPrivateKeyPkcs8: string,
): Promise<boolean> {
  try {
    const publicKey = base64url.decode(publicKeyWire);
    const proof = base64url.decode(proofWire);
    if (publicKey.length !== 32 || proof.length !== 32) return false;
    const privateKey = await crypto.subtle.importKey(
      "pkcs8",
      buffer(base64url.decode(cloudPrivateKeyPkcs8)),
      { name: "X25519" },
      false,
      ["deriveBits"],
    );
    const peer = await crypto.subtle.importKey(
      "raw",
      buffer(publicKey),
      { name: "X25519" },
      false,
      [],
    );
    const shared = await crypto.subtle.deriveBits(
      { name: "X25519", public: peer },
      privateKey,
      256,
    );
    const hkdf = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveKey"]);
    const key = await crypto.subtle.deriveKey(
      { name: "HKDF", hash: "SHA-256", salt: new ArrayBuffer(0), info: buffer(LABEL) },
      hkdf,
      { name: "HMAC", hash: "SHA-256", length: 256 },
      false,
      ["verify"],
    );
    return crypto.subtle.verify(
      "HMAC",
      key,
      buffer(proof),
      buffer(message(deviceId, keyVersion, publicKey)),
    );
  } catch {
    return false;
  }
}
