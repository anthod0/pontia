import { isHeroName } from "./hero-name";

const EDGE_ZONE = "edge.pontia.dev";
const CHALLENGE_PREFIX = "/.well-known/pontia-edge-address/";
const PROBE_TIMEOUT_MS = 5_000;
const MAX_PROBE_BYTES = 128;
const MAX_PROBE_RESPONSE_BYTES = 4_096;
const HEALTH_RESPONSE = "ok";

export type EdgeNetworkIdentity = {
  tunnelUrl: string;
};

export interface DnsProvider {
  ensureA(hostname: string, address: string): Promise<void>;
}

type TcpSocket = {
  readable: ReadableStream<Uint8Array>;
  writable: WritableStream<Uint8Array>;
  opened: Promise<unknown>;
  close(): Promise<void>;
};

export type EdgeNetworkDependencies = {
  randomBytes(length: number): Uint8Array;
  connect(address: { hostname: string; port: number }): TcpSocket;
  dns: DnsProvider;
};

type HttpFetcher = (input: string, init: RequestInit) => Promise<Response>;

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
    parsed.port !== "" ||
    parsed.pathname !== "/tunnel" ||
    parsed.search !== "" ||
    parsed.hash !== ""
  ) {
    return null;
  }
  const suffix = `.${EDGE_ZONE}`;
  if (!parsed.hostname.endsWith(suffix)) return null;
  const label = parsed.hostname.slice(0, -suffix.length);
  if (!/^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(label) || label.length > 63) return null;
  return parsed.hostname;
}

export function edgeApiOrigin(tunnelUrl: string): string | null {
  const hostname = edgeHostname(tunnelUrl);
  if (!hostname || tunnelUrl !== `wss://${hostname}/tunnel`) return null;
  const hero = hostname.slice(0, -`.${EDGE_ZONE}`.length);
  if (!isHeroName(hero)) return null;
  return `https://${hostname}`;
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

function nonce(randomBytes: (length: number) => Uint8Array) {
  const bytes = randomBytes(32);
  if (bytes.length !== 32) throw new Error("nonce generator returned the wrong length");
  return btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}

async function readSocket(socket: TcpSocket, maximum: number) {
  const reader = socket.readable.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > maximum) throw new Error("response too large");
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

async function probeIpv4(
  candidateIpv4: string,
  challenge: string,
  connect: EdgeNetworkDependencies["connect"],
) {
  const socket = connect({ hostname: candidateIpv4, port: 80 });
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    const probe = (async () => {
      await socket.opened;
      const writer = socket.writable.getWriter();
      await writer.write(
        new TextEncoder().encode(
          `GET ${CHALLENGE_PREFIX}${challenge} HTTP/1.1\r\nHost: ${candidateIpv4}\r\nAccept: text/plain\r\nConnection: close\r\n\r\n`,
        ),
      );
      writer.releaseLock();
      return readSocket(socket, MAX_PROBE_RESPONSE_BYTES);
    })();
    const response = await Promise.race([
      probe,
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error("probe timed out")), PROBE_TIMEOUT_MS);
      }),
    ]);
    const separator = response.indexOf("\r\n\r\n");
    if (separator < 0) return false;
    const headers = response.slice(0, separator);
    const body = response.slice(separator + 4);
    return /^HTTP\/1\.[01] 200(?: |$)/.test(headers.split("\r\n", 1)[0]) && body === challenge;
  } finally {
    if (timeout !== undefined) clearTimeout(timeout);
    await socket.close().catch(() => undefined);
  }
}

export async function configureEdgeNetwork(
  identity: EdgeNetworkIdentity,
  candidateIpv4: string,
  dependencies: EdgeNetworkDependencies,
) {
  const hostname = edgeHostname(identity.tunnelUrl);
  if (!hostname || !isGlobalUnicastIpv4(candidateIpv4)) return { status: "invalid" as const };

  const challenge = nonce(dependencies.randomBytes);
  try {
    if (!(await probeIpv4(candidateIpv4, challenge, dependencies.connect))) {
      return { status: "unreachable" as const };
    }
  } catch {
    return { status: "unreachable" as const };
  }

  await dependencies.dns.ensureA(hostname, candidateIpv4);
  return { status: "configured" as const, hostname };
}

export async function verifyEdgeHealth(identity: EdgeNetworkIdentity, fetcher: HttpFetcher) {
  const hostname = edgeHostname(identity.tunnelUrl);
  if (!hostname) return false;
  try {
    const response = await fetcher(`https://${hostname}/healthz`, {
      method: "GET",
      redirect: "manual",
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
      headers: { Accept: "text/plain" },
    });
    return (
      response.status === 200 && (await boundedText(response, MAX_PROBE_BYTES)) === HEALTH_RESPONSE
    );
  } catch {
    return false;
  }
}

export class CloudflareDnsProvider implements DnsProvider {
  constructor(
    private readonly token: string,
    private readonly zoneId: string,
    private readonly fetcher: (input: string, init: RequestInit) => Promise<Response> = fetch,
  ) {}

  async ensureA(hostname: string, address: string) {
    const base = `https://api.cloudflare.com/client/v4/zones/${encodeURIComponent(this.zoneId)}/dns_records`;
    const headers = { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json" };
    const lookup = await this.fetcher(`${base}?type=A&name=${encodeURIComponent(hostname)}`, {
      method: "GET",
      headers,
      redirect: "manual",
    });
    if (!lookup.ok) throw new Error("DNS provider lookup failed");
    const body = (await lookup.json()) as {
      success?: unknown;
      result?: Array<{
        id?: unknown;
        name?: unknown;
        type?: unknown;
        content?: unknown;
        proxied?: unknown;
      }>;
    };
    if (body.success !== true || !Array.isArray(body.result)) {
      throw new Error("DNS provider returned an invalid lookup response");
    }
    const records = body.result;
    if (records.length > 1) throw new Error("DNS provider returned duplicate A records");
    const record = records[0];
    if (
      record &&
      (record.name !== hostname ||
        record.type !== "A" ||
        typeof record.id !== "string" ||
        typeof record.content !== "string")
    ) {
      throw new Error("DNS provider returned an unexpected record");
    }
    if (record?.content !== undefined && record.content !== address) {
      throw new Error("changing an edge public IPv4 address is unsupported");
    }
    if (record?.content === address && record.proxied === false) return;
    const payload = JSON.stringify({
      type: "A",
      name: hostname,
      content: address,
      ttl: 60,
      proxied: false,
    });
    const saved = await this.fetcher(
      record ? `${base}/${encodeURIComponent(record.id as string)}` : base,
      {
        method: record ? "PUT" : "POST",
        headers,
        body: payload,
        redirect: "manual",
      },
    );
    if (!saved.ok) throw new Error("DNS provider update failed");
    const savedBody = (await saved.json()) as { success?: unknown };
    if (savedBody.success !== true) throw new Error("DNS provider rejected update");
  }
}
