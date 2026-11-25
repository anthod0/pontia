import { base64url, SignJWT } from "jose";
import {
  DnsProviderError,
  readDnsProviderResponse,
  type DnsOperation,
} from "./cloudflare-dns-errors";
import { isEdgePort, edgeAuthority } from "../edge-port";
import { capabilitySigningKey } from "./remote-access/device-connect";

const EDGE_ZONE = "edge.pontia.dev";
const PROBE_TIMEOUT_MS = 5_000;
const MAX_PROBE_BYTES = 128;
const HEALTH_RESPONSE = "ok";
const DOH_ENDPOINT = "https://cloudflare-dns.com/dns-query";
const RELEASE_VERSION =
  /^\d+\.\d+\.\d+(?:-[\da-z-]+(?:\.[\da-z-]+)*)?(?:\+[\da-z-]+(?:\.[\da-z-]+)*)?$/i;
const DNS_LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
const TOP_LEVEL_LABEL = /^(?:[a-z]{2,63}|xn--[a-z0-9-]{1,59}[a-z0-9])$/;
// Local, private, and special-use names (RFC 6761, RFC 6762, RFC 8375, RFC 9476).
const RESERVED_TOP_LEVEL_LABELS = new Set([
  "localhost",
  "local",
  "internal",
  "arpa",
  "localdomain",
  "invalid",
  "onion",
  "alt",
]);
const OWNERSHIP_KEY_LABEL = "pontia-edge-ownership-key-v1\0";
const OWNERSHIP_PROOF_LABEL = "pontia-edge-ownership-v1\0";

export type EdgeNetworkIdentity = {
  tunnelUrl: string;
};

export type EdgeHealth =
  | { status: "healthy"; version: string | null; updateNonce?: string }
  | { status: "unreachable" };

type HttpFetcher = (input: string, init: RequestInit) => Promise<Response>;

/** A canonical lowercase public DNS name with at least two labels and no IP literal. */
export function isValidEdgeHostname(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > 253) return false;
  const labels = value.split(".");
  const topLevel = labels.at(-1) ?? "";
  return (
    labels.length >= 2 &&
    labels.every((label) => DNS_LABEL.test(label)) &&
    TOP_LEVEL_LABEL.test(topLevel) &&
    !RESERVED_TOP_LEVEL_LABELS.has(topLevel)
  );
}

export function edgeHostname(tunnelUrl: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(tunnelUrl);
  } catch {
    return null;
  }
  if (
    parsed.protocol !== "wss:" ||
    parsed.username !== "" ||
    parsed.password !== "" ||
    !isEdgePort(Number(parsed.port || 443)) ||
    parsed.pathname !== "/tunnel" ||
    parsed.search !== "" ||
    parsed.hash !== "" ||
    !isValidEdgeHostname(parsed.hostname)
  ) {
    return null;
  }
  return parsed.hostname;
}

export function edgeApiOrigin(tunnelUrl: string): string | null {
  const hostname = edgeHostname(tunnelUrl);
  if (!hostname) return null;
  const authority = edgeAuthority(hostname, Number(new URL(tunnelUrl).port || 443));
  if (tunnelUrl !== `wss://${authority}/tunnel`) return null;
  return `https://${authority}`;
}

/** The Pontia-managed control hostname that Cloud may write DNS records for. */
export function managedEdgeHostname(edge: {
  dnsLabel: string | null;
  tunnelUrl: string;
}): string | null {
  if (edge.dnsLabel === null || !DNS_LABEL.test(edge.dnsLabel)) return null;
  const hostname = `${edge.dnsLabel}.${EDGE_ZONE}`;
  return edgeHostname(edge.tunnelUrl) === hostname ? hostname : null;
}

/** Existing edges whose DNS records Pontia manages; unset or empty allows nobody. */
export function isLegacyEdgeDnsOwner(env: App.Platform["env"], userId: string): boolean {
  const raw = env.LEGACY_EDGE_DNS_OWNER_IDS;
  if (!raw) return false;
  return raw
    .split(",")
    .map((id) => id.trim())
    .filter(Boolean)
    .includes(userId);
}

export function isGlobalUnicastIpv4(value: string): boolean {
  const parts = value.split(".");
  if (parts.length !== 4 || parts.some((part) => !/^(?:0|[1-9][0-9]{0,2})$/.test(part))) {
    return false;
  }
  const octets = parts.map(Number);
  if (octets.some((part) => part > 255)) return false;
  const [a, b, c] = octets;
  return !(
    a === 0 ||
    a === 10 ||
    a === 127 ||
    a >= 224 ||
    (a === 100 && b >= 64 && b <= 127) ||
    (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 0 && c === 0 && octets[3] !== 9 && octets[3] !== 10) ||
    (a === 192 && b === 0 && c === 2) ||
    (a === 192 && b === 88 && c === 99) ||
    (a === 192 && b === 168) ||
    (a === 198 && (b === 18 || b === 19)) ||
    (a === 198 && b === 51 && c === 100) ||
    (a === 203 && b === 0 && c === 113)
  );
}

function ipv6Segments(value: string): number[] | null {
  if (!/^[0-9a-f:]+$/i.test(value) || value.split("::").length > 2) return null;
  const [head, tail] = value.includes("::") ? value.split("::") : [value, undefined];
  const parse = (part: string) => (part === "" ? [] : part.split(":"));
  const left = parse(head);
  const right = tail === undefined ? [] : parse(tail);
  const missing = 8 - left.length - right.length;
  if (tail === undefined ? missing !== 0 : missing < 1) return null;
  const groups = [...left, ...Array<string>(tail === undefined ? 0 : missing).fill("0"), ...right];
  if (groups.some((group) => !/^[0-9a-f]{1,4}$/i.test(group))) return null;
  return groups.map((group) => Number.parseInt(group, 16));
}

/** Allocated global unicast space (2000::/3) without special-purpose or address-embedding blocks. */
export function isGlobalUnicastIpv6(value: string): boolean {
  const segments = ipv6Segments(value);
  if (!segments) return false;
  const [a, b] = segments;
  return (
    (a & 0xe000) === 0x2000 &&
    !(a === 0x2001 && b < 0x0200) &&
    !(a === 0x2001 && b === 0x0db8) &&
    a !== 0x2002 &&
    !(a === 0x3fff && b < 0x1000)
  );
}

export function isGlobalUnicastIp(value: string): boolean {
  return value.includes(":") ? isGlobalUnicastIpv6(value) : isGlobalUnicastIpv4(value);
}

export type PublicDnsResult = "public" | "non_public";

/** Requires at least one A/AAAA answer and only public unicast addresses. */
export async function resolvesToPublicAddresses(
  hostname: string,
  fetcher: HttpFetcher,
): Promise<PublicDnsResult> {
  const answers = await Promise.all(
    (["A", "AAAA"] as const).map(async (type) => {
      const response = await fetcher(
        `${DOH_ENDPOINT}?name=${encodeURIComponent(hostname)}&type=${type}`,
        {
          method: "GET",
          redirect: "error",
          signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
          headers: { Accept: "application/dns-json" },
        },
      );
      if (response.status !== 200) throw new Error("DNS-over-HTTPS lookup failed");
      const body = JSON.parse(await boundedText(response, 65_536)) as unknown;
      if (typeof body !== "object" || body === null || !("Status" in body)) {
        throw new Error("invalid DNS-over-HTTPS response");
      }
      const { Status: status, Answer: answer } = body as { Status: unknown; Answer?: unknown };
      // NXDOMAIN is an empty answer; other failures must not be read as "no records".
      if (status === 3) return [];
      if (status !== 0) throw new Error("DNS-over-HTTPS lookup failed");
      if (answer === undefined) return [];
      if (!Array.isArray(answer)) throw new Error("invalid DNS-over-HTTPS response");
      const recordType = type === "A" ? 1 : 28;
      return answer
        .filter((record) => (record as { type?: unknown }).type === recordType)
        .map((record) => (record as { data?: unknown }).data);
    }),
  );
  const addresses = answers.flat();
  return addresses.length > 0 &&
    addresses.every((address) => typeof address === "string" && isGlobalUnicastIp(address))
    ? "public"
    : "non_public";
}

function concat(...parts: Uint8Array[]) {
  const output = new Uint8Array(parts.reduce((size, part) => size + part.byteLength, 0));
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.byteLength;
  }
  return output;
}

function buffer(value: Uint8Array): ArrayBuffer {
  return value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength) as ArrayBuffer;
}

async function ownershipKey(credentialSecret: string) {
  const encoder = new TextEncoder();
  const digest = await crypto.subtle.digest(
    "SHA-256",
    buffer(concat(encoder.encode(OWNERSHIP_KEY_LABEL), base64url.decode(credentialSecret))),
  );
  return crypto.subtle.importKey("raw", digest, { name: "HMAC", hash: "SHA-256" }, false, [
    "verify",
  ]);
}

export function ownershipMessage(edgeId: string, hostname: string, port: number, nonce: string) {
  return new TextEncoder().encode(
    `${OWNERSHIP_PROOF_LABEL}${edgeId}\0${hostname}\0${port}\0${nonce}`,
  );
}

/** Constant-time check of the edge's HMAC proof over this challenge. */
export async function verifyOwnershipProof(
  credentialSecret: string,
  message: Uint8Array,
  proof: string,
): Promise<boolean> {
  if (!/^[A-Za-z0-9_-]{43}$/.test(proof)) return false;
  let signature: Uint8Array;
  try {
    signature = base64url.decode(proof);
  } catch {
    return false;
  }
  if (signature.byteLength !== 32 || base64url.encode(signature) !== proof) return false;
  return crypto.subtle.verify(
    "HMAC",
    await ownershipKey(credentialSecret),
    buffer(signature),
    buffer(message),
  );
}

export type OwnershipChallenge = {
  edgeId: string;
  hostname: string;
  port: number;
  credentialSecret: string;
};

/** Asks the endpoint to prove it holds this edge's service credential over WebPKI HTTPS. */
export async function verifyEdgeOwnership(
  challenge: OwnershipChallenge,
  signingKey: string,
  fetcher: HttpFetcher,
  randomBytes: (length: number) => Uint8Array = (length) =>
    crypto.getRandomValues(new Uint8Array(length)),
): Promise<boolean> {
  const { edgeId, hostname, port, credentialSecret } = challenge;
  if (!isValidEdgeHostname(hostname) || !isEdgePort(port)) return false;
  const bytes = randomBytes(32);
  if (bytes.length !== 32) throw new Error("nonce generator returned the wrong length");
  const nonce = base64url.encode(bytes);
  const now = Math.floor(Date.now() / 1000);
  const token = await new SignJWT({ host: hostname, port })
    .setProtectedHeader({ alg: "EdDSA", typ: "pontia-edge-ownership+jwt" })
    .setAudience(edgeId)
    .setJti(nonce)
    .setIssuedAt(now)
    .setExpirationTime(now + 60)
    .sign(await capabilitySigningKey(signingKey));
  try {
    const response = await fetcher(
      `https://${edgeAuthority(hostname, port)}/__pontia/verify-ownership`,
      {
        method: "GET",
        redirect: "manual",
        signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
        headers: { Authorization: `Bearer ${token}`, Accept: "text/plain" },
      },
    );
    if (response.status !== 200) {
      await response.body?.cancel();
      return false;
    }
    const proof = await boundedText(response, MAX_PROBE_BYTES);
    return await verifyOwnershipProof(
      credentialSecret,
      ownershipMessage(edgeId, hostname, port, nonce),
      proof,
    );
  } catch {
    return false;
  }
}

async function boundedText(response: Response, maximum: number) {
  const declared = response.headers.get("content-length");
  if (declared !== null && Number(declared) > maximum) throw new Error("response too large");
  if (!response.body) return "";
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > maximum) {
      await reader.cancel();
      throw new Error("response too large");
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder().decode(bytes);
}

export async function probeEdgeHealth(
  identity: EdgeNetworkIdentity,
  fetcher: HttpFetcher,
): Promise<EdgeHealth> {
  const origin = edgeApiOrigin(identity.tunnelUrl);
  if (!origin) return { status: "unreachable" };
  try {
    const response = await fetcher(`${origin}/healthz`, {
      method: "GET",
      redirect: "manual",
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
      headers: { Accept: "text/plain" },
    });
    if (
      response.status !== 200 ||
      (await boundedText(response, MAX_PROBE_BYTES)) !== HEALTH_RESPONSE
    ) {
      return { status: "unreachable" };
    }
    const nonce = response.headers.get("Pontia-Update-Nonce");
    const version = response.headers.get("Pontia-Edge-Version");
    return {
      status: "healthy",
      ...(nonce &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(nonce)
        ? { updateNonce: nonce }
        : {}),
      version: version && version.length <= 128 && RELEASE_VERSION.test(version) ? version : null,
    };
  } catch {
    return { status: "unreachable" };
  }
}

export function validDnsChallenge(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_-]{43}$/.test(value);
}

export class CloudflareDnsProvider {
  constructor(
    private readonly token: string,
    private readonly zoneId: string,
    private readonly domain = EDGE_ZONE,
  ) {}

  private async txtRequest(operation: DnsOperation, path: string, init: RequestInit = {}) {
    let response: Response;
    try {
      response = await fetch(
        `https://api.cloudflare.com./client/v4/zones/${encodeURIComponent(this.zoneId)}/dns_records${path}`,
        {
          ...init,
          signal: AbortSignal.timeout(10_000),
          headers: { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json" },
          redirect: "manual",
        },
      );
    } catch {
      throw new DnsProviderError(operation, "network_error");
    }
    // Deleting an already-removed record is an idempotent success.
    if (operation === "delete" && response.status === 404) return { result: [] };
    return readDnsProviderResponse(response, operation);
  }

  private managedHostname(hostname: string) {
    const suffix = `.${this.domain}`;
    return hostname.endsWith(suffix) && DNS_LABEL.test(hostname.slice(0, -suffix.length));
  }

  private async matchingTxt(hostname: string, value: string) {
    if (!this.managedHostname(hostname) || !validDnsChallenge(value)) {
      throw new DnsProviderError("lookup", "local_validation_error");
    }
    const name = `_acme-challenge.${hostname}`;
    const records: { id: string }[] = [];
    for (let page = 1; ; page++) {
      const body = await this.txtRequest(
        "lookup",
        `?type=TXT&name=${encodeURIComponent(name)}&per_page=20&page=${page}`,
      );
      if (!Array.isArray(body.result))
        throw new DnsProviderError("lookup", "invalid_provider_response");
      for (const record of body.result as Record<string, unknown>[]) {
        if (
          record.name !== name ||
          record.type !== "TXT" ||
          typeof record.id !== "string" ||
          typeof record.content !== "string"
        ) {
          throw new DnsProviderError("lookup", "invalid_provider_response");
        }
        if (record.content === value || record.content === JSON.stringify(value)) {
          records.push({ id: record.id });
        }
      }
      if (body.result.length < 20) break;
    }
    return records;
  }

  async publishTxt(hostname: string, value: string) {
    if ((await this.matchingTxt(hostname, value)).length) return;
    await this.txtRequest("create", "", {
      method: "POST",
      body: JSON.stringify({
        type: "TXT",
        name: `_acme-challenge.${hostname}`,
        content: JSON.stringify(value),
        ttl: 60,
      }),
    });
  }

  async cleanupTxt(hostname: string, value: string) {
    for (const record of await this.matchingTxt(hostname, value)) {
      await this.txtRequest("delete", `/${encodeURIComponent(record.id)}`, { method: "DELETE" });
    }
  }

  private async records(name: string, type: "A" | "TXT") {
    const records: { id: string; content: string }[] = [];
    for (let page = 1; ; page++) {
      const body = await this.txtRequest(
        "lookup",
        `?type=${type}&name=${encodeURIComponent(name)}&per_page=20&page=${page}`,
      );
      if (!Array.isArray(body.result)) {
        throw new DnsProviderError("lookup", "invalid_provider_response");
      }
      for (const record of body.result as Record<string, unknown>[]) {
        if (
          record.name !== name ||
          record.type !== type ||
          typeof record.id !== "string" ||
          typeof record.content !== "string"
        ) {
          throw new DnsProviderError("lookup", "invalid_provider_response");
        }
        records.push({ id: record.id, content: record.content });
      }
      if (body.result.length < 20) break;
    }
    return records;
  }

  async ipv4(hostname: string) {
    const records = await this.records(hostname, "A");
    if (records.length !== 1 || !isGlobalUnicastIpv4(records[0].content))
      throw new DnsProviderError("lookup", "local_validation_error");
    return records[0].content;
  }

  async cleanupHostname(hostname: string) {
    if (!this.managedHostname(hostname)) {
      throw new DnsProviderError("lookup", "local_validation_error");
    }
    const records = [
      ...(await this.records(this.domain === EDGE_ZONE ? hostname : `*.${hostname}`, "A")),
      ...(await this.records(`_acme-challenge.${hostname}`, "TXT")),
    ];
    for (const record of records) {
      await this.txtRequest("delete", `/${encodeURIComponent(record.id)}`, { method: "DELETE" });
    }
  }

  async ensureA(hostname: string, address: string) {
    const base = `https://api.cloudflare.com./client/v4/zones/${encodeURIComponent(this.zoneId)}/dns_records`;
    const headers = { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json" };
    let lookup: Response;
    try {
      lookup = await fetch(`${base}?type=A&name=${encodeURIComponent(hostname)}`, {
        method: "GET",
        headers,
        redirect: "manual",
      });
    } catch {
      throw new DnsProviderError("lookup", "network_error");
    }
    const body = await readDnsProviderResponse(lookup, "lookup");
    if (!Array.isArray(body.result)) {
      throw new DnsProviderError("lookup", "invalid_provider_response", lookup.status);
    }
    const records = body.result;
    if (records.length > 1) {
      throw new DnsProviderError("lookup", "local_validation_error", lookup.status);
    }
    const record = records[0] as Record<string, unknown> | undefined;
    if (
      record &&
      (record.name !== hostname ||
        record.type !== "A" ||
        typeof record.id !== "string" ||
        typeof record.content !== "string")
    ) {
      throw new DnsProviderError("lookup", "local_validation_error", lookup.status);
    }
    if (this.domain === EDGE_ZONE && record?.content !== undefined && record.content !== address) {
      throw new DnsProviderError("update", "local_validation_error");
    }
    if (record?.content === address && record.proxied === false) return;
    const payload = JSON.stringify({
      type: "A",
      name: hostname,
      content: address,
      ttl: 60,
      proxied: false,
    });
    const operation: DnsOperation = record ? "update" : "create";
    let saved: Response;
    try {
      saved = await fetch(record ? `${base}/${encodeURIComponent(record.id as string)}` : base, {
        method: record ? "PUT" : "POST",
        headers,
        body: payload,
        redirect: "manual",
      });
    } catch {
      throw new DnsProviderError(operation, "network_error");
    }
    await readDnsProviderResponse(saved, operation);
  }
}

export function previewDns(env: App.Platform["env"]) {
  return new CloudflareDnsProvider(
    env.CLOUDFLARE_DNS_TOKEN,
    env.CLOUDFLARE_PREVIEW_DNS_ZONE_ID,
    env.PREVIEW_ZONE,
  );
}

export function previewBase(dnsLabel: string, zone: string) {
  return `${dnsLabel}.${zone}`;
}

export async function probePreview(base: string, tunnelUrl: string) {
  const port = Number(new URL(tunnelUrl).port || 443);
  const response = await fetch(
    `https://${edgeAuthority("probe." + base, port)}/__pontia_preview_health`,
    {
      redirect: "manual",
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
      cache: "no-store",
    },
  );
  return (
    response.status === 200 && (await boundedText(response, MAX_PROBE_BYTES)) === HEALTH_RESPONSE
  );
}
